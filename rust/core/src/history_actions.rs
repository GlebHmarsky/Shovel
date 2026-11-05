//! History mutations use immutable commit IDs and native Git protections.
use crate::{
    branches::{Context, Progress, Stage},
    git::Git,
    operation::{Completed, Error, Transcript},
    process::Cancellation,
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
    Reset { target: String, mode: Reset },
    Revert { target: String, mainline: usize },
}
impl Action {
    pub fn target(&self) -> &str {
        match self {
            Self::Reset { target, .. } | Self::Revert { target, .. } => target,
        }
    }
}
impl Git {
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
        if !matches!(target.len(), 40 | 64) || !target.bytes().all(|b| b.is_ascii_hexdigit()) {
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
            Action::Revert { mainline, .. } => {
                if if parents > 1 {
                    *mainline == 0 || *mainline > parents
                } else {
                    *mainline != 0
                } {
                    return Err(Error::Code("HistoryRevertMainlineRequired"));
                }
                args.extend(["revert".into(), "--no-edit".into()]);
                if *mainline != 0 {
                    args.extend(["--mainline".into(), mainline.to_string()]);
                }
                args.push(target.into());
            }
        }
        self.validate_local(context, true, &cancel)?;
        stage(match action {
            Action::Reset { .. } => Stage::Resetting,
            Action::Revert { .. } => Stage::Reverting,
        });
        let mut log = Transcript::default();
        let result = log.write(self, root, args).map(|_| None);
        stage(Stage::Reconciling);
        log.finish(self, root, result)
    }
}
