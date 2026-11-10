//! Preparation-free initialization options; all validation and native work is
//! performed on submission. Existing repositories are inspected, never reinitialized.
use crate::{
    git::{self, Git, Snapshot},
    network::redact,
    operation::Error,
    process::{self, Cancellation, Lifetime},
    working::MutationGate,
};
use std::{
    ffi::OsString,
    fs,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

#[derive(Clone, Debug)]
pub struct Options {
    pub directory: PathBuf,
    pub initial_branch: String,
}
pub struct Completed {
    pub repository: Option<Snapshot>,
    pub succeeded: bool,
    pub message: &'static str,
    pub output: String,
}
pub fn destination(path: &Path) -> Result<PathBuf, Error> {
    if !path.is_absolute() || path.as_os_str().is_empty() {
        return Err(Error::Code("InitInvalidDirectory"));
    }
    let text = path.to_string_lossy();
    if text
        .chars()
        .any(|c| c == '\0' || c == '"' || c == '<' || c == '>' || c == '|')
    {
        return Err(Error::Code("InitInvalidDirectory"));
    }
    std::path::absolute(path)
        .map(|p| p.components().collect())
        .map_err(|_| Error::Code("InitInvalidDirectory"))
}
fn validate(target: &Path) -> Result<PathBuf, Error> {
    let mut nearest = None;
    for part in target.ancestors() {
        if part
            .file_name()
            .is_some_and(|v| v.eq_ignore_ascii_case(".git"))
        {
            return Err(Error::Code("InitMetadataDirectory"));
        }
        match fs::symlink_metadata(part) {
            Ok(metadata) => {
                if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    return Err(Error::Code("InitLinkedDirectory"));
                }
                if !metadata.is_dir() {
                    return Err(Error::Code("InitInvalidDirectory"));
                }
                if nearest.is_none() {
                    nearest = Some(part.to_path_buf());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    nearest.ok_or(Error::Code("InitInvalidDirectory"))
}
fn exists(path: &Path) -> Result<bool, Error> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
fn metadata_ancestor(path: &Path) -> Result<bool, Error> {
    for part in path.ancestors() {
        if exists(&part.join(".git"))?
            || exists(&part.join("HEAD"))?
                && (exists(&part.join("objects"))? || exists(&part.join("reftable"))?)
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn same(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .eq_ignore_ascii_case(
            b.to_string_lossy()
                .replace('/', "\\")
                .trim_end_matches('\\'),
        )
}
fn read_failure(result: process::Result) -> Error {
    git::Error::Command {
        outcome: result.outcome,
        code: result.exit_code,
        output: redact(&String::from_utf8_lossy(&result.stderr)),
    }
    .into()
}
impl Git {
    pub fn initialize_repository(&self, options: &Options) -> Result<Completed, Error> {
        let target = destination(&options.directory)?;
        let _gate = MutationGate::acquire_named(&target, "Init")?;
        let parent = validate(&target)?;
        let cancel = Cancellation::default();
        let probe = process::run(
            &self.request(
                &parent,
                ["rev-parse", "--is-bare-repository"],
                Lifetime::Read,
                1024 * 1024,
            ),
            &cancel,
            None,
        )?;
        if probe.outcome != process::Outcome::Exited {
            return Err(read_failure(probe));
        }
        if probe.exit_code == 0 {
            if String::from_utf8_lossy(&probe.stdout).trim() != "false" {
                return Err(Error::Code("InitBareRepository"));
            }
            let repository = self.inspect(&parent, &cancel)?;
            if !same(&repository.worktree, &target) {
                return Err(Error::Code("InitNestedRepository"));
            }
            return Ok(Completed {
                repository: Some(repository),
                succeeded: true,
                message: "InitAlreadyExists",
                output: String::new(),
            });
        }
        if probe.exit_code != 128 || metadata_ancestor(&parent)? {
            return Err(read_failure(probe));
        }
        let branch = &options.initial_branch;
        if !branch.trim().is_empty() {
            if branch.starts_with('-') || branch.chars().any(char::is_control) {
                return Err(Error::Code("InitInvalidBranch"));
            }
            let check = process::run(
                &self.request(
                    &parent,
                    ["check-ref-format", "--branch", branch],
                    Lifetime::Read,
                    1024 * 1024,
                ),
                &cancel,
                None,
            )?;
            if check.outcome != process::Outcome::Exited {
                return Err(read_failure(check));
            }
            if check.exit_code != 0 {
                return Err(Error::Code("InitInvalidBranch"));
            }
        }
        if !same(&parent, &validate(&target)?) || metadata_ancestor(&parent)? {
            return Err(Error::Code("InitDestinationChanged"));
        }
        let mut args: Vec<OsString> =
            vec!["-c".into(), "core.fsmonitor=false".into(), "init".into()];
        if !branch.trim().is_empty() {
            args.push(format!("--initial-branch={branch}").into());
        }
        args.push("--".into());
        args.push(target.as_os_str().to_owned());
        let result = process::run(
            &self.request(&parent, args, Lifetime::AcceptedCommand, 1024 * 1024),
            &cancel,
            None,
        )?;
        let succeeded = result.success();
        let output = redact(&format!(
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        ));
        let mut completed = Completed {
            repository: None,
            succeeded,
            message: if succeeded {
                "InitSucceeded"
            } else {
                "InitIncomplete"
            },
            output,
        };
        match validate(&target).and_then(|_| self.inspect(&target, &cancel).map_err(Error::from)) {
            Ok(repository) if same(&repository.worktree, &target) => {
                completed.repository = Some(repository)
            }
            Ok(_) => completed.message = "InitUnexpectedLayout",
            Err(_) if succeeded => completed.message = "InitCreatedUnverified",
            Err(_) => {}
        }
        Ok(completed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn destination_gate_matches_baseline_and_only_blocks_the_same_target() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("created");
        let lock_target = target.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let owner = std::thread::spawn(move || {
            let _gate = MutationGate::acquire_named(&lock_target, "Init").unwrap();
            ready_tx.send(()).unwrap();
            let _ = release_rx.recv();
        });
        ready_rx.recv().unwrap();
        let git = Git::discover(None, &Cancellation::default()).unwrap();
        let error = git
            .initialize_repository(&Options {
                directory: temp.path().join("unused/../created"),
                initial_branch: String::new(),
            })
            .err()
            .expect("canonical target must share the gate");
        assert_eq!(error.code(), "ActionOtherInstance");
        assert!(!target.exists());
        // A different target and the GitAction/Fetch lanes are independent.
        assert!(MutationGate::acquire_named(&temp.path().join("other"), "Init").is_ok());
        assert!(MutationGate::acquire(&target, false).is_ok());
        assert!(MutationGate::acquire(&target, true).is_ok());
        drop(release_tx);
        owner.join().unwrap();
        assert!(MutationGate::acquire_named(&target, "Init").is_ok());
    }
}
