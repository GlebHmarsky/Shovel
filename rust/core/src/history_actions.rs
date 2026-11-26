//! History mutations use immutable commit IDs and native Git protections.
use crate::{
    branches::{Context, Progress, Stage},
    git::{Commit, Git, Snapshot, log_options, parse_commits},
    operation::{Completed, Error, Transcript},
    process::{self, Cancellation, Lifetime, Outcome},
    working::MutationGate,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reset {
    Soft,
    Mixed,
    Hard,
}
#[derive(Clone, Debug)]
pub enum Action {
    Reset {
        target: String,
        mode: Reset,
    },
    Revert {
        target: String,
        mainline: usize,
    },
    CherryPick {
        target: String,
        mainline: usize,
        commit: bool,
        append_origin: bool,
    },
    Merge {
        target: String,
        fast_forward: bool,
    },
    Rebase {
        target: String,
    },
}
impl Action {
    pub fn target(&self) -> &str {
        match self {
            Self::Reset { target, .. }
            | Self::Revert { target, .. }
            | Self::CherryPick { target, .. }
            | Self::Merge { target, .. }
            | Self::Rebase { target } => target,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CherryPickPreview {
    Clean,
    NoChanges,
    Conflicts,
    Unavailable,
    Stale,
}

fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn same_tip(current: &Snapshot, expected: &Snapshot) -> bool {
    current.identity == expected.identity
        && current.head == expected.head
        && current.branch == expected.branch
        && expected.operations.is_empty()
        && current.operations.is_empty()
}

impl Git {
    pub fn history_commit(
        &self,
        snapshot: &Snapshot,
        hash: &str,
        cancel: &Cancellation,
    ) -> Option<Commit> {
        if !oid(hash) {
            return None;
        }
        let mut args = vec!["log".into(), "-1".into()];
        args.extend(log_options());
        args.extend([hash.into(), "--".into()]);
        let result = self
            .read(&snapshot.worktree, args, cancel, 1024 * 1024)
            .ok()?;
        parse_commits(&result.stdout, 1)
            .ok()?
            .into_iter()
            .find(|commit| commit.hash == hash)
    }

    pub fn prepare_undo_last_commit(
        &self,
        expected: &Snapshot,
        cancel: &Cancellation,
    ) -> Result<String, Error> {
        let Some(head) = expected.head.as_deref().filter(|head| oid(head)) else {
            return Err(Error::Code("HistoryResetNeedsBranch"));
        };
        if expected.branch.is_none() {
            return Err(Error::Code("HistoryResetNeedsBranch"));
        }
        let context = self.prepare_local(expected, false, cancel)?;
        let parent = self
            .local_read(
                &expected.worktree,
                &["rev-parse", "--verify", &format!("{head}^1^{{commit}}")],
                cancel,
            )
            .map_err(|_| Error::Code("HistoryUndoNoParent"))?;
        let parent = parent.trim();
        if !oid(parent) {
            return Err(Error::Code("HistoryUndoNoParent"));
        }
        self.validate_local(&context, false, cancel)?;
        Ok(parent.to_owned())
    }

    pub fn preview_cherry_pick(
        &self,
        expected: &Snapshot,
        source: &Commit,
        mainline: usize,
        cancel: &Cancellation,
    ) -> CherryPickPreview {
        use CherryPickPreview::*;
        let preview = || {
            let root = &expected.worktree;
            let head = expected.head.as_deref().filter(|head| oid(head))?;
            if !oid(&source.hash) || !source.parents.iter().all(|parent| oid(parent)) {
                return None;
            }
            let before = self.working_copy(root, cancel).ok()?;
            if !same_tip(&before.repository, expected) {
                return Some(Stale);
            }
            if !before.entries.is_empty() {
                return None;
            }
            // Custom drivers or renormalization can run configured external helpers.
            let config = self.request(
                root,
                [
                    "config",
                    "--name-only",
                    "--get-regexp",
                    "^merge\\.(.*\\.driver|default|renormalize)$",
                ],
                Lifetime::Read,
                65536,
            );
            let config = process::run(&config, cancel, None).ok()?;
            if config.outcome != Outcome::Exited || config.exit_code != 1 {
                return None;
            }
            let objects = self
                .local_read(
                    root,
                    &[
                        "rev-parse",
                        "--path-format=absolute",
                        "--git-path",
                        "objects",
                    ],
                    cancel,
                )
                .ok()?;
            let objects = objects.trim_end_matches(['\r', '\n']);
            if !std::path::Path::new(objects).is_absolute() {
                return None;
            }
            // merge-tree never touches the worktree/index. Any objects it creates
            // belong to the temporary store; the repository is a read-only alternate.
            let scratch = tempfile::tempdir().ok()?;
            let mut isolated = self.clone();
            isolated.environment.insert(
                "GIT_OBJECT_DIRECTORY".into(),
                Some(scratch.path().as_os_str().into()),
            );
            let alternate = format!("\"{}\"", objects.replace('\\', "\\\\").replace('"', "\\\""));
            isolated.environment.insert(
                "GIT_ALTERNATE_OBJECT_DIRECTORIES".into(),
                Some(alternate.into()),
            );
            let base = match (source.parents.as_slice(), mainline) {
                ([], 0) => isolated
                    .local_read(
                        root,
                        &["hash-object", "-w", "-t", "tree", "--stdin"],
                        cancel,
                    )
                    .ok()?
                    .trim()
                    .to_owned(),
                ([parent], 0) => parent.clone(),
                (parents, mainline) if parents.len() > 1 && mainline > 0 => {
                    parents.get(mainline - 1)?.clone()
                }
                _ => return None,
            };
            let request = isolated.request(
                root,
                [
                    "merge-tree",
                    "--write-tree",
                    "--no-messages",
                    "--merge-base",
                    &base,
                    head,
                    &source.hash,
                ],
                Lifetime::Read,
                65536,
            );
            let simulated = process::run(&request, cancel, None).ok()?;
            let predicted = match (simulated.outcome, simulated.exit_code) {
                (Outcome::Exited, 0) => {
                    let output = std::str::from_utf8(&simulated.stdout).ok()?.trim();
                    if !oid(output) {
                        return None;
                    }
                    let head_tree = self
                        .local_read(
                            root,
                            &["rev-parse", "--verify", &format!("{head}^{{tree}}")],
                            cancel,
                        )
                        .ok()?;
                    if output == head_tree.trim() {
                        NoChanges
                    } else {
                        Clean
                    }
                }
                (Outcome::Exited, 1) => Conflicts,
                _ => return None,
            };
            let after = self.working_copy(root, cancel).ok()?;
            if !same_tip(&after.repository, expected) {
                return Some(Stale);
            }
            (after.entries.is_empty()
                && before.config_stamp == after.config_stamp
                && before.index_stamp == after.index_stamp)
                .then_some(predicted)
        };
        preview().unwrap_or(Unavailable)
    }

    pub fn execute_history(
        &self,
        context: &Context,
        action: &Action,
        progress: Option<Progress>,
    ) -> Result<Completed, Error> {
        let cancel = Cancellation::default();
        let stage = |s| {
            if let Some(callback) = &progress {
                callback(s);
            }
        };
        let _gate = MutationGate::acquire(&context.repository.common_directory, false)?;
        stage(Stage::Validating);
        let fresh = self.validate_local(context, true, &cancel)?;
        let root = &fresh.worktree;
        let target = action.target();
        if !oid(target) {
            return Err(Error::Code("HistoryTargetUnavailable"));
        }
        // Original object parents, never graph-projected stash parents or ref expressions.
        let metadata = self
            .local_read(root, &["cat-file", "commit", target], &cancel)
            .map_err(|_| Error::Code("HistoryTargetUnavailable"))?;
        let parents = metadata
            .split("\n\n")
            .next()
            .unwrap_or_default()
            .lines()
            .filter(|line| line.starts_with("parent "))
            .count();
        let mut args = vec!["-c".to_owned(), "submodule.recurse=false".into()];
        match action {
            Action::Reset { mode, .. } => {
                if fresh.head.is_none() {
                    return Err(Error::Code("HistoryResetNeedsBranch"));
                }
                let branch = fresh
                    .branch
                    .as_ref()
                    .ok_or(Error::Code("HistoryResetNeedsBranch"))?;
                let worktrees =
                    self.local_read(root, &["worktree", "list", "--porcelain", "-z"], &cancel)?;
                if !worktrees.ends_with('\0') {
                    return Err(Error::Code("GitErrorInvalidOutput"));
                }
                if worktrees
                    .split('\0')
                    .filter(|s| *s == format!("branch refs/heads/{branch}"))
                    .count()
                    > 1
                {
                    return Err(Error::Code("BranchManageOccupied"));
                }
                let mode = match mode {
                    Reset::Soft => "--soft",
                    Reset::Mixed => "--mixed",
                    Reset::Hard => "--hard",
                };
                args.extend(
                    ["reset", mode, "--no-recurse-submodules", target, "--"].map(str::to_owned),
                );
            }
            Action::Revert { mainline, .. } | Action::CherryPick { mainline, .. } => {
                if if parents > 1 {
                    *mainline == 0 || *mainline > parents
                } else {
                    *mainline != 0
                } {
                    return Err(Error::Code(if matches!(action, Action::Revert { .. }) {
                        "HistoryRevertMainlineRequired"
                    } else {
                        "HistoryMainlineRequired"
                    }));
                }
                let verb = if matches!(action, Action::Revert { .. }) {
                    "revert"
                } else {
                    "cherry-pick"
                };
                args.extend([verb.into(), "--no-edit".into()]);
                if *mainline != 0 {
                    args.extend(["--mainline".into(), mainline.to_string()]);
                }
                if let Action::CherryPick {
                    commit,
                    append_origin,
                    ..
                } = action
                {
                    if !commit {
                        args.push("--no-commit".into());
                    } else if *append_origin {
                        args.push("-x".into());
                    }
                }
                args.push(target.into());
            }
            Action::Merge { fast_forward, .. } => {
                if fresh.branch.is_none() || fresh.head.is_none() {
                    return Err(Error::Code("HistoryIntegrationNeedsBranch"));
                }
                args.extend(["merge", "--no-edit", "--no-autostash"].map(str::to_owned));
                if *fast_forward {
                    args.push("--ff-only".into());
                }
                args.extend(["--".into(), target.into()]);
            }
            Action::Rebase { .. } => {
                if fresh.branch.is_none() || fresh.head.is_none() {
                    return Err(Error::Code("HistoryIntegrationNeedsBranch"));
                }
                args.extend(
                    [
                        "rebase",
                        "--no-autostash",
                        "--no-update-refs",
                        "--no-rebase-merges",
                        "--",
                        target,
                    ]
                    .map(str::to_owned),
                );
            }
        }
        self.validate_local(context, true, &cancel)?;
        stage(match action {
            Action::Reset { .. } => Stage::Resetting,
            Action::Revert { .. } => Stage::Reverting,
            Action::CherryPick { .. } | Action::Merge { .. } | Action::Rebase { .. } => {
                Stage::RepositoryCommand
            }
        });
        let mut log = Transcript::default();
        let result = log.write(self, root, args).map(|_| None);
        stage(Stage::Reconciling);
        log.finish(self, root, result)
    }
}
