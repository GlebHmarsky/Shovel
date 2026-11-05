//! Revisioned per-worktree draft persistence, separate from the C# profile.
use crate::{
    identity::{self, Identity},
    process::Cancellation,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Text {
    pub subject: String,
    pub description: String,
}
impl Text {
    pub fn message(&self) -> String {
        if self.description.is_empty() {
            self.subject.clone()
        } else {
            format!("{}\n\n{}", self.subject, self.description)
        }
    }
    pub fn is_empty(&self) -> bool {
        self.subject.is_empty() && self.description.is_empty()
    }
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub revision: Option<String>,
    pub text: Text,
    pub has_recovery: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Document {
    schema_version: u32,
    key: String,
    revision: String,
    subject: String,
    description: String,
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("CommitDraftInvalid")]
    Invalid,
    #[error("CommitDraftNewerSchema")]
    Newer,
    #[error("CommitDraftChanged")]
    Changed,
    #[error("CommitDraftIdentityChanged")]
    Identity,
    #[error("CommitDraftTooLarge")]
    TooLarge,
    #[error("CommitDraftUnavailable")]
    Io(#[from] std::io::Error),
    #[error("CommitDraftUnavailable")]
    Cancelled,
}
#[derive(Clone)]
pub struct Store {
    pub directory: PathBuf,
    pub legacy_directory: Option<PathBuf>,
}
impl Store {
    pub fn key(identity: &Identity) -> String {
        format!(
            "{:X}",
            Sha256::digest(identity.fingerprint.to_uppercase().as_bytes())
        )
    }
    pub fn load(&self, identity: &Identity, cancel: &Cancellation) -> Result<Snapshot, Error> {
        retry(cancel, || {
            validate(identity, cancel)?;
            let key = Self::key(identity);
            let destination = self.directory.join(format!("{key}.json"));
            let mut snapshot = read(&destination, &key)?;
            if snapshot.revision.is_none()
                && let Some(legacy) = &self.legacy_directory
            {
                let imported = read(&legacy.join(format!("{key}.json")), &key)?;
                // The first Rust save owns a fresh revision; the C# bytes are never changed.
                snapshot.text = imported.text;
            }
            snapshot.has_recovery = has_recovery(&self.directory, &key)?;
            validate(identity, cancel)?;
            Ok(snapshot)
        })
    }
    pub fn save(
        &self,
        identity: &Identity,
        expected: Option<&str>,
        text: &Text,
        cancel: &Cancellation,
    ) -> Result<Snapshot, Error> {
        validate_text(text)?;
        retry(cancel, || {
            validate(identity, cancel)?;
            let key = Self::key(identity);
            fs::create_dir_all(&self.directory)?;
            let _gate = acquire(&self.directory, &key, cancel)?;
            let destination = self.directory.join(format!("{key}.json"));
            let current = read(&destination, &key)?;
            if current.revision.as_deref() != expected {
                return Err(Error::Changed);
            }
            let revision = uuid::Uuid::new_v4().simple().to_string();
            self.publish(identity, &key, &destination, &revision, text, cancel)?;
            Ok(Snapshot {
                revision: Some(revision),
                text: text.clone(),
                has_recovery: has_recovery(&self.directory, &key)?,
            })
        })
    }
    pub fn recovery(
        &self,
        identity: &Identity,
        copy: &str,
        text: &Text,
        cancel: &Cancellation,
    ) -> Result<(), Error> {
        validate_text(text)?;
        if copy.len() != 32 || !copy.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Invalid);
        }
        retry(cancel, || {
            validate(identity, cancel)?;
            let key = Self::key(identity);
            fs::create_dir_all(&self.directory)?;
            let _gate = acquire(&self.directory, &key, cancel)?;
            let destination = self.directory.join(format!("{key}.{copy}.recovery.json"));
            let _ = read(&destination, &key)?;
            if text.is_empty() {
                match fs::remove_file(destination) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(e) => Err(e.into()),
                }
            } else {
                self.publish(
                    identity,
                    &key,
                    &destination,
                    &uuid::Uuid::new_v4().simple().to_string(),
                    text,
                    cancel,
                )
            }
        })
    }
    fn publish(
        &self,
        identity: &Identity,
        key: &str,
        destination: &Path,
        revision: &str,
        text: &Text,
        cancel: &Cancellation,
    ) -> Result<(), Error> {
        let bytes = serde_json::to_vec(&Document {
            schema_version: 1,
            key: key.into(),
            revision: revision.into(),
            subject: text.subject.clone(),
            description: text.description.clone(),
        })
        .map_err(|_| Error::Invalid)?;
        if bytes.len() > 512 * 1024 {
            return Err(Error::TooLarge);
        }
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        validate(identity, cancel)?;
        if destination.exists() {
            let temporary = temporary.into_temp_path();
            let wide = |p: &Path| {
                p.as_os_str()
                    .encode_wide()
                    .chain(Some(0))
                    .collect::<Vec<_>>()
            };
            let dest = wide(destination);
            let source = wide(&temporary);
            if unsafe {
                windows_sys::Win32::Storage::FileSystem::ReplaceFileW(
                    dest.as_ptr(),
                    source.as_ptr(),
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
        } else {
            temporary
                .persist_noclobber(destination)
                .map_err(|e| Error::Io(e.error))?;
        }
        Ok(())
    }
}
fn validate_text(text: &Text) -> Result<(), Error> {
    if text.subject.encode_utf16().count() > 16384
        || text.description.encode_utf16().count() > 65536
    {
        Err(Error::TooLarge)
    } else {
        Ok(())
    }
}
fn validate(expected: &Identity, cancel: &Cancellation) -> Result<(), Error> {
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    if identity::read(&expected.root)?.fingerprint != expected.fingerprint {
        return Err(Error::Identity);
    }
    Ok(())
}
fn read(path: &Path, key: &str) -> Result<Snapshot, Error> {
    let file = match OpenOptions::new().read(true).share_mode(1 | 4).open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Snapshot::default()),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(512 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 512 * 1024 {
        return Err(Error::TooLarge);
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
    if value
        .get("SchemaVersion")
        .and_then(serde_json::Value::as_u64)
        .is_some_and(|v| v > 1)
    {
        return Err(Error::Newer);
    }
    let doc: Document = serde_json::from_value(value).map_err(|_| Error::Invalid)?;
    if doc.schema_version != 1
        || doc.key != key
        || doc.revision.len() != 32
        || !doc.revision.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(Error::Invalid);
    }
    let text = Text {
        subject: doc.subject,
        description: doc.description,
    };
    validate_text(&text)?;
    Ok(Snapshot {
        revision: Some(doc.revision),
        text,
        has_recovery: false,
    })
}
fn has_recovery(directory: &Path, key: &str) -> Result<bool, Error> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let name = entry?.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&format!("{key}.")) && name.ends_with(".recovery.json") {
            return Ok(true);
        }
    }
    Ok(false)
}
fn acquire(directory: &Path, key: &str, cancel: &Cancellation) -> Result<fs::File, Error> {
    let start = Instant::now();
    loop {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(directory.join(format!("{key}.lock")))
        {
            Ok(file) => return Ok(file),
            Err(e)
                if matches!(e.raw_os_error(), Some(32 | 33))
                    && start.elapsed() < Duration::from_secs(2) =>
            {
                thread::sleep(Duration::from_millis(50))
            }
            Err(_) => return Err(Error::Io(std::io::Error::other("draft writer unavailable"))),
        }
    }
}
fn retry<T>(
    cancel: &Cancellation,
    mut operation: impl FnMut() -> Result<T, Error>,
) -> Result<T, Error> {
    for attempt in 0..4 {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        match operation() {
            Err(Error::Io(e)) if matches!(e.raw_os_error(), Some(32 | 33)) && attempt < 3 => {
                thread::sleep(Duration::from_millis(50 * (attempt + 1)))
            }
            result => return result,
        }
    }
    unreachable!()
}
