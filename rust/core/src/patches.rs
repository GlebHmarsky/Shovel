//! Selected binary patch export. The repository is read only; the destination
//! is replaced by one same-directory move only after complete validation.
use crate::{
    diff, files,
    git::Git,
    operation::Error,
    process::{self, Cancellation, Lifetime},
    working::{self, Change, Context, Group},
};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    io::Write,
    os::windows::ffi::OsStrExt,
    path::{Component, Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};
pub const MAX_BYTES: usize = 16 * 1024 * 1024;
const FLAGS: [&str; 10] = [
    "--binary",
    "--full-index",
    "--no-ext-diff",
    "--no-textconv",
    "--no-color",
    "--no-relative",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--submodule=short",
    "--ignore-submodules=dirty",
];
pub fn key(context: &Context, entries: &[Change], destination: &Path) -> String {
    let mut digest = Sha256::new();
    digest.update(files::discard_key(context, entries).as_bytes());
    for unit in destination.as_os_str().encode_wide() {
        digest.update(unit.to_le_bytes());
    }
    format!("save-patch:{:x}", digest.finalize())
}

fn spelling(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").to_uppercase()
}
fn destination(
    context: &Context,
    entries: &[Change],
    destination: &Path,
) -> Result<PathBuf, Error> {
    if !destination.is_absolute() || destination.as_os_str().encode_wide().any(|c| c == 0) {
        return Err(Error::Code("FilePatchDestination"));
    }
    let destination = std::path::absolute(destination)?;
    if destination
        .components()
        .any(|c| matches!(c, Component::Normal(n) if n.eq_ignore_ascii_case(".git")))
    {
        return Err(Error::Code("FilePatchDestination"));
    }
    let parent = destination
        .parent()
        .ok_or(Error::Code("FilePatchDestination"))?
        .canonicalize()?;
    let name = destination
        .file_name()
        .ok_or(Error::Code("FilePatchDestination"))?;
    let destination = parent.join(name);
    let normalized = spelling(&destination);
    for root in [
        &context.repository.git_directory,
        &context.repository.common_directory,
    ] {
        let root = spelling(&root.canonicalize()?);
        if normalized == root || normalized.starts_with(&(root + "\\")) {
            return Err(Error::Code("FilePatchDestination"));
        }
    }
    for source in files::paths(entries) {
        let file = working::path(&context.repository.worktree, &source)?;
        let file = match file.canonicalize() {
            Ok(p) => p,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                context.repository.worktree.canonicalize()?.join(source)
            }
            Err(e) => return Err(e.into()),
        };
        if spelling(&file) == normalized {
            return Err(Error::Code("FilePatchDestination"));
        }
    }
    Ok(destination)
}
impl Git {
    pub fn selected_patch(&self, context: &Context, entries: &[Change]) -> Result<Vec<u8>, Error> {
        if entries.is_empty() || entries.iter().any(|e| !context.entries.contains(e)) {
            return Err(Error::Code("CommitStale"));
        }
        if !files::supported(entries) {
            return Err(Error::Code("FileActionUnsupported"));
        }
        let staged = entries[0].group == Group::Staged;
        if entries.iter().any(|e| (e.group == Group::Staged) != staged) {
            return Err(Error::Code("CommitStale"));
        }
        let paths = files::paths(entries);
        if paths.len() > 128
            || paths
                .iter()
                .map(|p| p.encode_utf16().count() + 3)
                .sum::<usize>()
                > 16000
        {
            return Err(Error::Code("FileActionSelectionLimit"));
        }
        let cancel = Cancellation::default();
        self.validate_file_selection(context, entries, &cancel)?;
        let root = &context.repository.worktree;
        let stamps = || -> Result<Vec<String>, Error> {
            if staged {
                Ok(vec![])
            } else {
                paths
                    .iter()
                    .map(|p| diff::content_stamp(root, p, &cancel).map_err(Error::from))
                    .collect()
            }
        };
        let before = stamps()?;
        let mut bytes = vec![];
        let mut read =
            |scope: Vec<OsString>, selected: Vec<OsString>, allow_one: bool| -> Result<(), Error> {
                let mut args = vec![
                    OsString::from("--literal-pathspecs"),
                    "-c".into(),
                    "core.fsmonitor=false".into(),
                ];
                args.extend(context.filters.clone());
                args.push("diff".into());
                args.extend(scope);
                args.extend(FLAGS.map(Into::into));
                args.push("--".into());
                args.extend(selected);
                let mut request = self.request(root, args, Lifetime::AcceptedCommand, MAX_BYTES);
                request
                    .environment
                    .insert("GIT_OPTIONAL_LOCKS".into(), Some("0".into()));
                let r = process::run(&request, &cancel, None)?;
                if r.outcome == process::Outcome::OutputLimit
                    || bytes.len() + r.stdout.len() > MAX_BYTES
                {
                    return Err(Error::Code("GitErrorOutputLimit"));
                }
                if r.outcome != process::Outcome::Exited
                    || r.exit_code != 0 && !(allow_one && r.exit_code == 1)
                {
                    return Err(Error::Git(crate::git::Error::Command {
                        outcome: r.outcome,
                        code: r.exit_code,
                        output: crate::network::redact(&String::from_utf8_lossy(&r.stderr)),
                    }));
                }
                bytes.extend_from_slice(&r.stdout);
                Ok(())
            };
        let tracked = files::paths(
            &entries
                .iter()
                .filter(|e| e.group != Group::Untracked)
                .cloned()
                .collect::<Vec<_>>(),
        );
        if !tracked.is_empty() {
            let mut scope = vec![];
            if staged {
                scope.push("--cached".into());
                if let Some(head) = &context.repository.head {
                    scope.push(head.into());
                }
            }
            read(scope, tracked.into_iter().map(Into::into).collect(), false)?;
        }
        for entry in entries.iter().filter(|e| e.group == Group::Untracked) {
            read(
                vec!["--no-index".into()],
                vec!["/dev/null".into(), entry.path.clone().into()],
                true,
            )?;
        }
        if before != stamps()? {
            return Err(Error::Code("CommitStale"));
        }
        self.validate_file_selection(context, entries, &cancel)?;
        Ok(bytes)
    }
    pub fn export_selected_patch(
        &self,
        context: &Context,
        entries: &[Change],
        target: &Path,
    ) -> Result<usize, Error> {
        let target = destination(context, entries, target)?;
        let bytes = self.selected_patch(context, entries)?;
        self.write_patch(&target, &bytes)
            .map_err(|error| match error {
                Error::Io(_) => Error::Code("RustPatchWriteFailed"),
                other => other,
            })?;
        Ok(bytes.len())
    }
    fn write_patch(&self, target: &Path, bytes: &[u8]) -> Result<(), Error> {
        let parent = target.parent().ok_or(Error::Code("FilePatchDestination"))?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".shovel-patch-")
            .tempfile_in(parent)?;
        temporary.write_all(bytes)?;
        temporary.as_file().sync_all()?;
        let temporary = temporary.into_temp_path();
        let wide = |p: &Path| {
            p.as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        };
        let source = wide(&temporary);
        let dest = wide(target);
        // Both paths are absolute, owned, NUL-terminated UTF-16. The source is
        // our closed regular temporary file on the same volume as the target.
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                dest.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, fs::OpenOptions, os::windows::fs::OpenOptionsExt};
    #[test]
    fn patch_atomic_write_preserves_locked_readonly_destinations_and_cleans_own_temp() {
        let directory = tempfile::tempdir().unwrap();
        let git = Git::discover(None, &Cancellation::default()).unwrap();
        let target = directory.path().join("output.patch");
        fs::write(&target, b"existing").unwrap();
        let lock = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&target)
            .unwrap();
        assert!(git.write_patch(&target, b"replacement").is_err());
        drop(lock);
        assert_eq!(fs::read(&target).unwrap(), b"existing");
        let original_permissions = fs::metadata(&target).unwrap().permissions();
        let mut permissions = original_permissions.clone();
        permissions.set_readonly(true);
        fs::set_permissions(&target, permissions).unwrap();
        let result = git.write_patch(&target, b"replacement");
        fs::set_permissions(&target, original_permissions).unwrap();
        assert!(result.is_err());
        assert_eq!(fs::read(&target).unwrap(), b"existing");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        git.write_patch(&target, b"replacement").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"replacement");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
