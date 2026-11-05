//! Saved stashes are selected by immutable hash and reviewed reflog position.
//! Pop applies once, then removes only the unchanged reviewed saved entry.
use crate::{
    branches::{Progress, Stage},
    git::{self, Commit, Git, Snapshot},
    identity,
    operation::{Completed, Error, Transcript},
    process::Cancellation,
    working::MutationGate,
};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub hash: String,
    pub selector: String,
    pub message: String,
    pub date: DateTime<Utc>,
}
#[derive(Clone, Debug)]
pub struct List {
    pub entries: Vec<Entry>,
    pub stamp: String,
}
#[derive(Clone, Debug)]
pub struct Target {
    pub hash: String,
    pub selector: String,
    pub list_stamp: Option<String>,
}
impl Target {
    pub fn reviewed(list: &List, entry: &Entry) -> Self {
        Self {
            hash: entry.hash.clone(),
            selector: entry.selector.clone(),
            list_stamp: Some(list.stamp.clone()),
        }
    }
}
#[derive(Clone, Debug)]
pub enum Action {
    Create {
        message: String,
        include_untracked: bool,
    },
    Apply {
        target: Target,
        restore_index: bool,
        pop: bool,
    },
    Drop(Target),
}
#[derive(Clone, Debug)]
pub struct Inspection {
    pub worktree: Commit,
    pub index: Commit,
    pub untracked: Option<Commit>,
}
fn oid(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn matches(list: &List, target: &Target) -> bool {
    target
        .list_stamp
        .as_ref()
        .is_none_or(|stamp| *stamp == list.stamp)
        && list
            .entries
            .iter()
            .any(|e| e.hash == target.hash && e.selector == target.selector)
}
impl Git {
    pub fn stashes(&self, expected: &Snapshot, cancel: &Cancellation) -> Result<List, Error> {
        if identity::read(&expected.worktree)? != expected.identity {
            return Err(Error::Code("ActionIdentityChanged"));
        }
        let result = self.read(
            &expected.worktree,
            [
                "stash",
                "list",
                "-z",
                "--no-show-signature",
                "--no-notes",
                "--max-count=1001",
                "--format=%H%x00%gd%x00%gs%x00%ct",
            ],
            cancel,
            1024 * 1024,
        )?;
        let text = std::str::from_utf8(&result.stdout)
            .map_err(|_| Error::Code("GitErrorInvalidOutput"))?;
        let mut entries = vec![];
        if !text.is_empty() {
            if !text.ends_with('\0') {
                return Err(Error::Code("GitErrorInvalidOutput"));
            }
            let fields = text.split_terminator('\0').collect::<Vec<_>>();
            if fields.len() % 4 != 0 {
                return Err(Error::Code("GitErrorInvalidOutput"));
            }
            for fields in fields.as_chunks::<4>().0 {
                if entries.len() == 1000 {
                    return Err(Error::Code("StashListLimit"));
                }
                if !oid(fields[0]) || fields[1] != format!("stash@{{{}}}", entries.len()) {
                    return Err(Error::Code("GitErrorInvalidOutput"));
                }
                let date = fields[3]
                    .parse()
                    .ok()
                    .and_then(|seconds| DateTime::from_timestamp(seconds, 0))
                    .ok_or(Error::Code("GitErrorInvalidOutput"))?;
                entries.push(Entry {
                    hash: fields[0].into(),
                    selector: fields[1].into(),
                    message: fields[2].into(),
                    date,
                });
            }
        }
        if identity::read(&expected.worktree)? != expected.identity {
            return Err(Error::Code("ActionIdentityChanged"));
        }
        Ok(List {
            entries,
            stamp: format!("{:X}", Sha256::digest(&result.stdout)),
        })
    }
    pub fn inspect_stash(
        &self,
        expected: &Snapshot,
        hash: &str,
        cancel: &Cancellation,
    ) -> Result<Inspection, Error> {
        if !oid(hash) {
            return Err(Error::Code("StashInvalid"));
        }
        if identity::read(&expected.worktree)? != expected.identity {
            return Err(Error::Code("ActionIdentityChanged"));
        }
        let commit = |hash: &str| -> Result<Commit, Error> {
            let mut args = vec!["log".into(), "--no-walk=unsorted".into()];
            args.extend(git::log_options());
            args.extend([hash.into(), "--".into()]);
            let raw = self.read(&expected.worktree, args, cancel, 1024 * 1024)?;
            let mut rows = git::parse_commits(&raw.stdout, 1)?;
            if rows.len() != 1 || rows[0].hash != hash {
                return Err(Error::Code("StashInvalid"));
            }
            Ok(rows.remove(0))
        };
        let worktree = commit(hash)?;
        if !(2..=3).contains(&worktree.parents.len()) {
            return Err(Error::Code("StashInvalid"));
        }
        let index = commit(&worktree.parents[1])?;
        let untracked = worktree.parents.get(2).map(|id| commit(id)).transpose()?;
        if identity::read(&expected.worktree)? != expected.identity {
            return Err(Error::Code("ActionIdentityChanged"));
        }
        Ok(Inspection {
            worktree,
            index,
            untracked,
        })
    }
    pub fn execute_stash(
        &self,
        expected: &Snapshot,
        action: &Action,
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
        let current = self.stashes(expected, &cancel)?;
        let fresh = self.inspect(&expected.worktree, &cancel)?;
        if fresh.identity != expected.identity
            || fresh.head != expected.head
            || fresh.branch != expected.branch
        {
            return Err(Error::Code("ActionChanged"));
        }
        let root = &fresh.worktree;
        if !matches!(action, Action::Drop(_)) {
            if !fresh.operations.is_empty() {
                return Err(Error::Code("ActionSequence"));
            }
            if matches!(action, Action::Create { .. }) && fresh.head.is_none() {
                return Err(Error::Code("StashNeedsCommit"));
            }
            let index = self.semantic_index(&fresh, &cancel)?;
            if !self
                .local_read(root, &["ls-files", "--unmerged", "-z"], &cancel)?
                .is_empty()
            {
                return Err(Error::Code("CommitUnresolved"));
            }
            if index != self.semantic_index(&fresh, &cancel)? {
                return Err(Error::Code("CommitStale"));
            }
        }
        let mut args = vec![
            "-c".to_owned(),
            "submodule.recurse=false".into(),
            "stash".into(),
        ];
        match action {
            Action::Create {
                message,
                include_untracked,
            } => {
                if message.contains('\0') || message.len() > 4096 {
                    return Err(Error::Code("StashMessageLimit"));
                }
                args.extend(["push".into(), "--no-keep-index".into()]);
                if *include_untracked {
                    args.push("--include-untracked".into());
                }
                args.push(format!("--message={message}"));
            }
            Action::Apply {
                target,
                restore_index,
                ..
            } => {
                if !matches(&current, target) {
                    return Err(Error::Code("StashChanged"));
                }
                args.extend([
                    "apply".into(),
                    if *restore_index {
                        "--index"
                    } else {
                        "--no-index"
                    }
                    .into(),
                    target.hash.clone(),
                ]);
            }
            Action::Drop(target) => {
                if !matches(&current, target)
                    || self.stashes(expected, &cancel)?.stamp != current.stamp
                {
                    return Err(Error::Code("StashChanged"));
                }
                args.extend(["drop".into(), target.selector.clone()]);
            }
        }
        stage(match action {
            Action::Create { .. } => Stage::Stashing,
            Action::Apply { .. } => Stage::Reapplying,
            Action::Drop(_) => Stage::DroppingStash,
        });
        if matches!(action, Action::Drop(_))
            && self.stashes(expected, &cancel)?.stamp != current.stamp
        {
            return Err(Error::Code("StashChanged"));
        }
        let mut log = Transcript::default();
        let result = (|| -> Result<Option<&'static str>, Error> {
            let succeeded = log.write(self, root, args)?;
            if let Action::Apply { target, pop, .. } = action {
                let unmerged =
                    match self.local_read(root, &["ls-files", "--unmerged", "-z"], &cancel) {
                        Ok(unmerged) => unmerged,
                        Err(_) => return Err(Error::Code("StashInspectFailed")),
                    };
                if !succeeded || !unmerged.is_empty() {
                    return Ok(Some("StashApplyFailed"));
                }
                if *pop {
                    // Once applied, never retry. Any list drift retains all saved data.
                    if !self
                        .stashes(expected, &cancel)
                        .is_ok_and(|after| after.stamp == current.stamp)
                    {
                        return Ok(Some("StashAppliedRetained"));
                    }
                    stage(Stage::DroppingStash);
                    if !self
                        .stashes(expected, &cancel)
                        .is_ok_and(|after| after.stamp == current.stamp)
                    {
                        return Ok(Some("StashAppliedRetained"));
                    }
                    match log.write(
                        self,
                        root,
                        [
                            "-c",
                            "submodule.recurse=false",
                            "stash",
                            "drop",
                            &target.selector,
                        ],
                    ) {
                        Ok(true) => {}
                        Ok(false) | Err(_) => return Ok(Some("StashAppliedRetained")),
                    }
                }
            }
            Ok(None)
        })();
        stage(Stage::Reconciling);
        let mut completed = log.finish(self, root, result)?;
        completed.preserves_worktree = matches!(action, Action::Drop(_));
        Ok(completed)
    }
}
