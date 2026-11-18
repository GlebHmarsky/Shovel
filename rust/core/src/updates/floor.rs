//! The channel's highest verified release, shared by this user's Rust profiles.
use super::{Channel, MAX_FEED_BYTES, UpdateError, VerifiedRelease, verify_feed};
use crate::process::Cancellation;
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
};

// Existing clients store the original JSON bytes and detached signature as
// two byte arrays in JSON. Keep the same encoding so they can read floor.json.
const MAX_RECORD_BYTES: usize = 8 * MAX_FEED_BYTES;

fn cancelled(cancel: &Cancellation) -> Result<(), UpdateError> {
    if cancel.is_cancelled() {
        return Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into());
    }
    Ok(())
}

fn lock_channel(directory: &Path) -> Result<File, UpdateError> {
    fs::create_dir_all(directory)?;
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("floor.lock"))?;
    lock.try_lock().map_err(std::io::Error::from)?;
    Ok(lock)
}

// Returns whether floor.json already contains this exact release. Validate
// legacy per-version records too, including records written by older clients.
fn check_records(
    directory: &Path,
    offer: &VerifiedRelease,
    cancel: &Cancellation,
) -> Result<bool, UpdateError> {
    let mut current = false;
    let mut legacy_count = 0;
    for entry in fs::read_dir(directory)? {
        cancelled(cancel)?;
        let path = entry?.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let is_current = path.file_name().is_some_and(|name| name == "floor.json");
        if !is_current {
            legacy_count += 1;
            if legacy_count > 1024 {
                return Err(UpdateError::TooLarge);
            }
        }
        let mut data = Vec::new();
        File::open(&path)?
            .take(MAX_RECORD_BYTES as u64 + 1)
            .read_to_end(&mut data)?;
        if data.len() > MAX_RECORD_BYTES {
            return Err(UpdateError::TooLarge);
        }
        let (feed, signature): (Vec<u8>, Vec<u8>) =
            serde_json::from_slice(&data).map_err(|_| UpdateError::InvalidFeed)?;
        let saved = verify_feed(offer.channel(), &feed, &signature)?;
        if offer.version() < saved.version() {
            return Err(UpdateError::Replay);
        }
        if offer.version() == saved.version() {
            if !offer.same_package(&saved) {
                return Err(UpdateError::InvalidFeed);
            }
            current |= is_current;
        }
    }
    cancelled(cancel)?;
    Ok(current)
}

pub fn verify_and_record(
    root: &Path,
    channel: Channel,
    feed: &[u8],
    signature: &[u8],
    cancel: &Cancellation,
) -> Result<VerifiedRelease, UpdateError> {
    cancelled(cancel)?;
    let offer = verify_feed(channel, feed, signature)?;
    let directory = root.join(channel.id());
    let _lock = lock_channel(&directory)?;
    if !check_records(&directory, &offer, cancel)? {
        let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
        serde_json::to_writer(&mut temporary, &(feed, signature)).map_err(std::io::Error::other)?;
        temporary.as_file().sync_all()?;
        cancelled(cancel)?;
        temporary
            .persist(directory.join("floor.json"))
            .map_err(|e| e.error)?;
    }
    Ok(offer)
}

/// Check a previously verified, frozen offer again immediately before use.
pub fn recheck(
    root: &Path,
    offer: &VerifiedRelease,
    cancel: &Cancellation,
) -> Result<(), UpdateError> {
    cancelled(cancel)?;
    let directory = root.join(offer.channel().id());
    let _lock = lock_channel(&directory)?;
    if check_records(&directory, offer, cancel)? {
        Ok(())
    } else {
        Err(UpdateError::InvalidFeed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const OLD: (&[u8], &[u8]) = (
        include_bytes!("../../tests/fixtures/beta-0.50.3.json"),
        include_bytes!("../../tests/fixtures/beta-0.50.3.json.sig"),
    );
    const NEW: (&[u8], &[u8]) = (
        include_bytes!("../../tests/fixtures/beta-0.50.14.json"),
        include_bytes!("../../tests/fixtures/beta-0.50.14.json.sig"),
    );

    #[test]
    fn advances_legacy_floor_and_refuses_older_cached_offers_across_profiles() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(Channel::Beta.id());
        fs::create_dir(&directory).unwrap();
        let legacy = directory.join("0.50.3.json");
        fs::write(&legacy, serde_json::to_vec(&OLD).unwrap()).unwrap();
        let cancel = Cancellation::default();
        let old = verify_and_record(root.path(), Channel::Beta, OLD.0, OLD.1, &cancel).unwrap();
        recheck(root.path(), &old, &cancel).unwrap();
        let new = verify_and_record(root.path(), Channel::Beta, NEW.0, NEW.1, &cancel).unwrap();
        let saved = fs::read(directory.join("floor.json")).unwrap();
        assert!(matches!(
            recheck(root.path(), &old, &cancel),
            Err(UpdateError::Replay)
        ));
        assert!(matches!(
            verify_and_record(root.path(), Channel::Beta, OLD.0, OLD.1, &cancel),
            Err(UpdateError::Replay)
        ));
        verify_and_record(root.path(), Channel::Beta, NEW.0, NEW.1, &cancel).unwrap();
        recheck(root.path(), &new, &cancel).unwrap();
        assert_eq!(fs::read(directory.join("floor.json")).unwrap(), saved);
        assert_eq!(fs::read(legacy).unwrap(), serde_json::to_vec(&OLD).unwrap());
        assert!(!directory.join("0.50.14.json").exists());
        assert!(verify_and_record(root.path(), Channel::Stable, NEW.0, NEW.1, &cancel).is_err());
        assert!(!root.path().join(Channel::Stable.id()).exists());
    }

    #[test]
    fn locked_damaged_and_cancelled_checks_preserve_the_trusted_record() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(Channel::Beta.id());
        let cancel = Cancellation::default();
        let old = verify_and_record(root.path(), Channel::Beta, OLD.0, OLD.1, &cancel).unwrap();
        let record = directory.join("floor.json");
        let before = fs::read(&record).unwrap();
        let held = lock_channel(&directory).unwrap();
        assert!(
            matches!(verify_and_record(root.path(), Channel::Beta, NEW.0, NEW.1, &cancel),
            Err(UpdateError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock)
        );
        drop(held);
        let stopped = Cancellation::default();
        stopped.cancel();
        assert!(
            matches!(verify_and_record(root.path(), Channel::Beta, NEW.0, NEW.1, &stopped),
            Err(UpdateError::Io(e)) if e.kind() == std::io::ErrorKind::Interrupted)
        );
        assert_eq!(fs::read(&record).unwrap(), before);
        let bogus = serde_json::to_vec(&(NEW.0, OLD.1)).unwrap();
        fs::write(&record, &bogus).unwrap();
        assert!(matches!(
            recheck(root.path(), &old, &cancel),
            Err(UpdateError::Signature)
        ));
        assert!(matches!(
            verify_and_record(root.path(), Channel::Beta, NEW.0, NEW.1, &cancel),
            Err(UpdateError::Signature)
        ));
        assert_eq!(fs::read(&record).unwrap(), bogus);
    }
}
