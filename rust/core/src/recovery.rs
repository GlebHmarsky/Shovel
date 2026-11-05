//! Bounded native sequence state. Resolving/staging files never invalidates the
//! displayed recovery target, but a changed HEAD or sequence does.
use crate::{
    branches::{Progress, Stage},
    git::{self, Git, Snapshot},
    identity,
    operation::{Completed, Error, Transcript},
    process::{self, Cancellation, Lifetime},
    working::MutationGate,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs::File, io::Read, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Merge,
    Rebase,
    CherryPick,
    Revert,
}
impl Kind {
    pub fn command(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
            Self::CherryPick => "cherry-pick",
            Self::Revert => "revert",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub kind: Kind,
    pub stamp: String,
    pub target: String,
    pub current: String,
    pub branch: String,
}
impl State {
    pub fn can_skip(&self) -> bool {
        self.kind != Kind::Merge && !self.current.is_empty()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Continue,
    Skip,
    Abort,
}
impl Action {
    fn argument(self) -> &'static str {
        match self {
            Self::Continue => "--continue",
            Self::Skip => "--skip",
            Self::Abort => "--abort",
        }
    }
}
fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

pub(crate) fn read(directory: &Path, operations: &[String]) -> std::io::Result<Option<State>> {
    if operations.is_empty() || directory.join("rebase-apply/applying").exists() {
        return Ok(None);
    }
    let names = [
        "MERGE_HEAD",
        "MERGE_MSG",
        "MERGE_AUTOSTASH",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "ORIG_HEAD",
        "rebase-merge/head-name",
        "rebase-merge/orig-head",
        "rebase-merge/onto",
        "rebase-merge/stopped-sha",
        "rebase-merge/git-rebase-todo",
        "rebase-merge/done",
        "rebase-merge/message",
        "rebase-merge/autostash",
        "rebase-apply/head-name",
        "rebase-apply/orig-head",
        "rebase-apply/onto",
        "rebase-apply/next",
        "rebase-apply/last",
        "rebase-apply/original-commit",
        "rebase-apply/autostash",
        "sequencer/head",
        "sequencer/todo",
        "sequencer/opts",
        "sequencer/abort-safety",
    ];
    let mut digest = Sha256::new();
    let mut values = BTreeMap::new();
    for name in names {
        let file = match File::open(directory.join(name)) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let mut bytes = Vec::new();
        file.take(256 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 256 * 1024 {
            return Ok(None);
        }
        digest.update(name.as_bytes());
        digest.update([0]);
        digest.update(&bytes);
        digest.update([0]);
        values.insert(name, String::from_utf8_lossy(&bytes).trim().to_owned());
    }
    let has = |name: &str| operations.iter().any(|s| s == name);
    let kind = if has("rebase") || has("rebase-apply") {
        Kind::Rebase
    } else if operations == ["merge"] {
        Kind::Merge
    } else if has("cherry-pick") && !has("revert") {
        Kind::CherryPick
    } else if has("revert") && !has("cherry-pick") {
        Kind::Revert
    } else if operations == ["sequencer"] {
        let todo = values
            .get("sequencer/todo")
            .map(String::as_str)
            .unwrap_or_default();
        let first = todo
            .lines()
            .map(str::trim)
            .find(|s| !s.is_empty() && !s.starts_with('#'))
            .unwrap_or_default();
        if first.starts_with("pick ") {
            Kind::CherryPick
        } else if first.starts_with("revert ") {
            Kind::Revert
        } else {
            return Ok(None);
        }
    } else {
        return Ok(None);
    };
    let value = |name: &str| values.get(name).map(String::as_str).unwrap_or_default();
    let either = |first: &str, next: &str| {
        values
            .get(first)
            .or_else(|| values.get(next))
            .map(String::as_str)
            .unwrap_or_default()
    };
    let hash = |s: &str| if oid(s) { s.to_owned() } else { String::new() };
    let target = match kind {
        Kind::Rebase => either("rebase-merge/orig-head", "rebase-apply/orig-head"),
        Kind::Merge => value("ORIG_HEAD"),
        _ => value("sequencer/head"),
    };
    let current = match kind {
        Kind::Rebase => either("rebase-merge/stopped-sha", "rebase-apply/original-commit"),
        Kind::CherryPick => value("CHERRY_PICK_HEAD"),
        Kind::Revert => value("REVERT_HEAD"),
        Kind::Merge => "",
    };
    let branch = either("rebase-merge/head-name", "rebase-apply/head-name");
    let branch = branch
        .strip_prefix("refs/heads/")
        .filter(|s| !s.chars().any(char::is_control))
        .unwrap_or_default()
        .to_owned();
    Ok(Some(State {
        kind,
        stamp: format!("{:X}", digest.finalize()),
        target: hash(target),
        current: hash(current),
        branch,
    }))
}
impl Git {
    fn validate_recovery(
        &self,
        expected: &Snapshot,
        cancel: &Cancellation,
    ) -> Result<State, Error> {
        if identity::read(&expected.worktree)? != expected.identity {
            return Err(Error::Code("ActionIdentityChanged"));
        }
        let state = expected
            .recovery
            .as_ref()
            .ok_or(Error::Code("RecoveryUnavailable"))?;
        let head = self.local_read(
            &expected.worktree,
            &["rev-parse", "--verify", "HEAD"],
            cancel,
        )?;
        let branch = process::run(
            &self.request(
                &expected.worktree,
                ["symbolic-ref", "--quiet", "--short", "HEAD"],
                Lifetime::Read,
                4096,
            ),
            cancel,
            None,
        )?;
        if branch.outcome != process::Outcome::Exited || branch.exit_code > 1 {
            return Err(Error::Code("ActionReadFailed"));
        }
        let branch = if branch.exit_code == 0 {
            Some(
                String::from_utf8(branch.stdout)
                    .map_err(|_| Error::Code("GitErrorInvalidOutput"))?
                    .trim()
                    .to_owned(),
            )
        } else {
            None
        };
        let operations = git::operation_markers(&expected.git_directory);
        if expected.head.as_deref() != Some(head.trim())
            || branch != expected.branch
            || operations != expected.operations
            || read(&expected.git_directory, &operations)?.as_ref() != Some(state)
        {
            return Err(Error::Code("RecoveryChanged"));
        }
        Ok(state.clone())
    }
    pub fn execute_recovery(
        &self,
        expected: &Snapshot,
        action: Action,
        progress: Option<Progress>,
    ) -> Result<Completed, Error> {
        let cancel = Cancellation::default();
        let stage = |s| {
            if let Some(callback) = &progress {
                callback(s);
            }
        };
        let _gate = MutationGate::acquire(&expected.common_directory, false)?;
        stage(Stage::Validating);
        let state = self.validate_recovery(expected, &cancel)?;
        if action == Action::Skip && !state.can_skip() {
            return Err(Error::Code("RecoveryUnavailable"));
        }
        let index = self.semantic_index(expected, &cancel)?;
        if action == Action::Continue
            && !self
                .local_read(
                    &expected.worktree,
                    &["ls-files", "--unmerged", "-z"],
                    &cancel,
                )?
                .is_empty()
        {
            return Err(Error::Code("CommitUnresolved"));
        }
        self.validate_recovery(expected, &cancel)?;
        if self.semantic_index(expected, &cancel)? != index {
            return Err(Error::Code("RecoveryChanged"));
        }
        stage(Stage::Recovering);
        let mut log = Transcript::default();
        // Preserve the existing commit message; native hooks, signing and existing
        // rebase exec commands remain Git-owned. No invisible editor or todo edit.
        let result = log
            .write_with_editor(
                self,
                &expected.worktree,
                [
                    "-c",
                    "submodule.recurse=false",
                    state.kind.command(),
                    action.argument(),
                ],
                "true",
            )
            .map(|_| None);
        stage(Stage::Reconciling);
        log.finish(self, &expected.worktree, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn unsupported_mailbox_ambiguous_and_oversized_sequences_have_no_recovery_target() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("rebase-apply")).unwrap();
        fs::write(root.join("rebase-apply/applying"), "").unwrap();
        assert!(read(root, &["rebase-apply".into()]).unwrap().is_none());
        fs::remove_file(root.join("rebase-apply/applying")).unwrap();
        assert!(
            read(root, &["cherry-pick".into(), "revert".into()])
                .unwrap()
                .is_none()
        );
        fs::write(root.join("MERGE_MSG"), vec![b'x'; 256 * 1024 + 1]).unwrap();
        assert!(read(root, &["merge".into()]).unwrap().is_none());
    }
    #[test]
    fn recovery_labels_only_expose_valid_hashes_and_branch_names() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir(root.join("rebase-merge")).unwrap();
        fs::write(
            root.join("rebase-merge/head-name"),
            "refs/heads/feature\nforged",
        )
        .unwrap();
        fs::write(root.join("rebase-merge/orig-head"), "arbitrary file text").unwrap();
        let hash = "a".repeat(40);
        fs::write(root.join("rebase-merge/stopped-sha"), &hash).unwrap();
        let state = read(root, &["rebase".into()]).unwrap().unwrap();
        assert!(state.target.is_empty());
        assert!(state.branch.is_empty());
        assert_eq!(state.current, hash);
        assert!(state.can_skip());
        fs::write(
            root.join("rebase-merge/head-name"),
            "refs/heads/feature/שלום",
        )
        .unwrap();
        let changed = read(root, &["rebase".into()]).unwrap().unwrap();
        assert_eq!(changed.branch, "feature/שלום");
        assert_ne!(state.stamp, changed.stamp);
    }
}
