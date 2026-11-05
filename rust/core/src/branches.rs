//! Local branch and reviewed checkout actions. Native Git owns worktree,
//! merged-branch, hook and filter behavior; no force or recovery rollback.
use crate::{
    git::{Git, Reference, Snapshot},
    identity,
    operation::{Completed, Error, Transcript},
    process::{self, Cancellation, Lifetime},
    working::{self, MutationGate},
};
use std::{path::Path, sync::Arc};

const LIMIT: usize = 1024 * 1024;
#[derive(Clone, Debug)]
pub struct Context {
    pub repository: Snapshot,
    pub config_stamp: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Changes {
    Keep,
    StashAndReapply,
    DiscardTracked,
}
#[derive(Clone, Debug)]
pub enum Checkout {
    Branch(Reference),
    New { name: String, start: Reference },
    Detached(String),
}
#[derive(Clone, Debug)]
pub enum Manage {
    Delete,
    SetUpstream(Reference),
    UnsetUpstream,
}
#[derive(Clone, Debug)]
pub enum Action {
    Create {
        name: String,
        start: Reference,
    },
    Checkout {
        target: Checkout,
        review: Arc<working::Context>,
        changes: Changes,
        stash_message: String,
    },
    Manage {
        branch: Reference,
        action: Manage,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Validating,
    Stashing,
    Discarding,
    Switching,
    Reapplying,
    DroppingStash,
    UpdatingBranch,
    Resetting,
    Reverting,
    Recovering,
    Reconciling,
}
pub type Progress = Arc<dyn Fn(Stage) + Send + Sync>;
fn text(bytes: &[u8]) -> Result<String, Error> {
    String::from_utf8(bytes.to_vec()).map_err(|_| Error::Code("GitErrorInvalidOutput"))
}
fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn same_ref(a: &Reference, b: &Reference) -> bool {
    a.name == b.name && a.hash == b.hash
}
fn has_ref(snapshot: &Snapshot, target: &Reference) -> bool {
    snapshot.references.iter().any(|r| same_ref(r, target))
}
impl Git {
    pub(crate) fn local_read(
        &self,
        root: &Path,
        args: &[&str],
        cancel: &Cancellation,
    ) -> Result<String, Error> {
        text(&self.read(root, args.iter().copied(), cancel, LIMIT)?.stdout)
    }
    pub fn prepare_local(
        &self,
        expected: &Snapshot,
        unchanged_references: bool,
        cancel: &Cancellation,
    ) -> Result<Context, Error> {
        let root = &expected.worktree;
        if identity::read(root)? != expected.identity {
            return Err(Error::Code("ActionIdentityChanged"));
        }
        let stamp = self.config_stamp(root, cancel)?;
        let fresh = self.inspect(root, cancel)?;
        if !fresh.operations.is_empty() {
            return Err(Error::Code("ActionSequence"));
        }
        if fresh.identity != expected.identity
            || fresh.branch != expected.branch
            || fresh.head != expected.head
            || unchanged_references && fresh.fingerprint != expected.fingerprint
            || self.config_stamp(root, cancel)? != stamp
        {
            return Err(Error::Code("BranchManageChanged"));
        }
        Ok(Context {
            repository: fresh,
            config_stamp: stamp,
        })
    }
    pub(crate) fn validate_local(
        &self,
        context: &Context,
        refs: bool,
        cancel: &Cancellation,
    ) -> Result<Snapshot, Error> {
        let fresh = self.prepare_local(&context.repository, refs, cancel)?;
        if fresh.config_stamp != context.config_stamp {
            return Err(Error::Code("BranchManageChanged"));
        }
        Ok(fresh.repository)
    }
    fn branch_name(&self, root: &Path, name: &str, cancel: &Cancellation) -> Result<(), Error> {
        if name.trim().is_empty()
            || name.starts_with('-')
            || name == "HEAD"
            || name.contains('@')
            || name.chars().any(char::is_control)
        {
            return Err(Error::Code("ActionBranchName"));
        }
        self.local_read(
            root,
            &["check-ref-format", &format!("refs/heads/{name}")],
            cancel,
        )
        .map_err(|_| Error::Code("ActionBranchName"))?;
        Ok(())
    }
    fn direct_ref(
        &self,
        root: &Path,
        target: &Reference,
        cancel: &Cancellation,
    ) -> Result<(), Error> {
        let data = self.local_read(
            root,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)%00%(symref)%00",
                "--",
                &target.name,
            ],
            cancel,
        )?;
        let mut matching = data
            .lines()
            .map(|line| line.split('\0').collect::<Vec<_>>())
            .filter(|fields| fields.first().copied() == Some(target.name.as_str()));
        let fields = matching.next().ok_or(Error::Code("BranchManageChanged"))?;
        if matching.next().is_some()
            || fields.len() != 4
            || fields[1] != target.hash
            || !fields[3].is_empty()
        {
            return Err(Error::Code("BranchManageChanged"));
        }
        if !fields[2].is_empty() {
            return Err(Error::Code("BranchManageSymbolicRef"));
        }
        Ok(())
    }
    fn branch_occupied(
        &self,
        root: &Path,
        target: &Reference,
        cancel: &Cancellation,
    ) -> Result<(), Error> {
        let data = self.local_read(root, &["worktree", "list", "--porcelain", "-z"], cancel)?;
        if !data.ends_with('\0') {
            return Err(Error::Code("GitErrorInvalidOutput"));
        }
        if data
            .split('\0')
            .any(|line| line == format!("branch {}", target.name))
        {
            return Err(Error::Code("BranchManageOccupied"));
        }
        Ok(())
    }
    fn checkout_command(
        &self,
        fresh: &Snapshot,
        target: &Checkout,
        cancel: &Cancellation,
    ) -> Result<Vec<String>, Error> {
        let root = &fresh.worktree;
        if fresh.git_directory.join("info/sparse-checkout").is_file() {
            return Err(Error::Code("ActionWorktreeUnsupported"));
        }
        let mut args = [
            "-c",
            "submodule.recurse=false",
            "switch",
            "--no-guess",
            "--no-overwrite-ignore",
            "--no-recurse-submodules",
        ]
        .map(str::to_owned)
        .to_vec();
        match target {
            Checkout::Branch(reference) => {
                let branch = reference
                    .name
                    .strip_prefix("refs/heads/")
                    .ok_or(Error::Code("ActionBranchName"))?;
                self.branch_name(root, branch, cancel)?;
                if !has_ref(fresh, reference) {
                    return Err(Error::Code("CheckoutChanged"));
                }
                args.extend(["--".into(), branch.into()]);
            }
            Checkout::New { name, start } => {
                self.branch_name(root, name, cancel)?;
                if !has_ref(fresh, start) {
                    return Err(Error::Code("ActionNoStart"));
                }
                args.extend([
                    "--no-track".into(),
                    "--create".into(),
                    name.clone(),
                    start.hash.clone(),
                ]);
            }
            Checkout::Detached(commit) => {
                if !oid(commit) {
                    return Err(Error::Code("HistoryTargetUnavailable"));
                }
                self.local_read(root, &["cat-file", "commit", commit], cancel)?;
                args.extend(["--detach".into(), commit.clone()]);
            }
        }
        Ok(args)
    }
    fn stash_tip(&self, root: &Path, cancel: &Cancellation) -> Result<Option<String>, Error> {
        let result = process::run(
            &self.request(
                root,
                ["rev-parse", "--verify", "--quiet", "refs/stash"],
                Lifetime::Read,
                4096,
            ),
            cancel,
            None,
        )?;
        if result.outcome != process::Outcome::Exited || result.exit_code > 1 {
            return Err(Error::Code("CheckoutStashUnavailable"));
        }
        if result.exit_code == 1 {
            return Ok(None);
        }
        let value = text(&result.stdout)?.trim().to_owned();
        if !oid(&value) {
            return Err(Error::Code("CheckoutStashUnavailable"));
        }
        Ok(Some(value))
    }
    pub fn execute_branch(
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
        let checkout = matches!(action, Action::Checkout { .. });
        let fresh = self.validate_local(context, !checkout, &cancel)?;
        let root = &fresh.worktree;
        let mut log = Transcript::default();
        let result = (|| -> Result<Option<&'static str>, Error> {
            match action {
                Action::Create { name, start } => {
                    self.branch_name(root, name, &cancel)?;
                    if !has_ref(&fresh, start) {
                        return Err(Error::Code("ActionNoStart"));
                    }
                    stage(Stage::UpdatingBranch);
                    log.write(
                        self,
                        root,
                        ["branch", "--no-track", "--", name, &start.hash],
                    )?;
                }
                Action::Manage { branch, action } => {
                    let name = branch
                        .name
                        .strip_prefix("refs/heads/")
                        .ok_or(Error::Code("BranchManageChanged"))?;
                    self.branch_name(root, name, &cancel)?;
                    if !has_ref(&context.repository, branch) {
                        return Err(Error::Code("BranchManageChanged"));
                    }
                    self.direct_ref(root, branch, &cancel)?;
                    let args = match action {
                        Manage::Delete => {
                            if fresh.branch.as_deref() == Some(name) {
                                return Err(Error::Code("BranchManageCurrent"));
                            }
                            self.branch_occupied(root, branch, &cancel)?;
                            vec!["branch".into(), "--delete".into(), "--".into(), name.into()]
                        }
                        Manage::SetUpstream(upstream) => {
                            if !upstream.name.starts_with("refs/remotes/")
                                || !has_ref(&context.repository, upstream)
                            {
                                return Err(Error::Code("BranchManageNoRemoteRef"));
                            }
                            self.direct_ref(root, upstream, &cancel)?;
                            vec![
                                "branch".into(),
                                format!("--set-upstream-to={}", upstream.name),
                                "--".into(),
                                name.into(),
                            ]
                        }
                        Manage::UnsetUpstream => vec![
                            "branch".into(),
                            "--unset-upstream".into(),
                            "--".into(),
                            name.into(),
                        ],
                    };
                    self.validate_local(context, true, &cancel)?;
                    self.direct_ref(root, branch, &cancel)?;
                    if let Manage::SetUpstream(upstream) = action {
                        self.direct_ref(root, upstream, &cancel)?;
                    }
                    if matches!(action, Manage::Delete) {
                        self.branch_occupied(root, branch, &cancel)?;
                    }
                    stage(Stage::UpdatingBranch);
                    log.write(self, root, args)?;
                }
                Action::Checkout {
                    target,
                    review,
                    changes,
                    stash_message,
                } => {
                    let command = self.checkout_command(&fresh, target, &cancel)?;
                    let current = self.working_copy(root, &cancel)?;
                    if current.repository.identity != review.repository.identity
                        || current.repository.head != review.repository.head
                        || current.repository.branch != review.repository.branch
                        || current.index_stamp != review.index_stamp
                        || current.config_stamp != review.config_stamp
                        || current.entries != review.entries
                    {
                        return Err(Error::Code("CheckoutChanged"));
                    }
                    // Unrelated fetched refs do not invalidate this review, but the
                    // exact named target, HEAD and configuration are checked again.
                    let latest = self.validate_local(context, false, &cancel)?;
                    self.checkout_command(&latest, target, &cancel)?;
                    let mut stash = None;
                    if *changes == Changes::StashAndReapply && !review.entries.is_empty() {
                        let prior = self.stash_tip(root, &cancel)?;
                        stage(Stage::Stashing);
                        let mut args = vec!["stash", "push", "--include-untracked"];
                        if !stash_message.is_empty() {
                            args.extend(["--message", stash_message]);
                        }
                        if !log.write(self, root, args)? {
                            return Ok(None);
                        }
                        stash = self.stash_tip(root, &cancel)?;
                        if stash.is_none() || stash == prior {
                            return Err(Error::Code("CheckoutStashUnavailable"));
                        }
                    } else if *changes == Changes::DiscardTracked {
                        stage(Stage::Discarding);
                        if !log.write(
                            self,
                            root,
                            [
                                "-c",
                                "submodule.recurse=false",
                                "restore",
                                "--source=HEAD",
                                "--staged",
                                "--worktree",
                                "--",
                                ".",
                            ],
                        )? {
                            return Ok(None);
                        }
                    }
                    stage(Stage::Switching);
                    if !log.write(self, root, command)? {
                        return Ok(stash.as_ref().map(|_| "CheckoutStashRetained"));
                    }
                    if let Some(stash) = stash {
                        stage(Stage::Reapplying);
                        let applied =
                            log.write(self, root, ["stash", "apply", "--index", &stash])?;
                        let unmerged =
                            self.local_read(root, &["ls-files", "--unmerged", "-z"], &cancel)?;
                        if !applied
                            || !unmerged.is_empty()
                            || self.stash_tip(root, &cancel)?.as_deref() != Some(&stash)
                        {
                            return Ok(Some("CheckoutStashRetained"));
                        }
                        stage(Stage::DroppingStash);
                        if !log.write(self, root, ["stash", "drop", "stash@{0}"])? {
                            return Ok(Some("CheckoutStashRetained"));
                        }
                    }
                }
            }
            Ok(None)
        })();
        stage(Stage::Reconciling);
        log.finish(self, root, result)
    }
}
