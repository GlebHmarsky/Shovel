#![cfg(windows)]
use shovel_core::{
    branches::{Action, Changes, Checkout, Manage, Stage},
    git::{Git, Reference, Snapshot},
    operation::Error,
    process::{self, Cancellation, Lifetime},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
fn raw(git: &Git, root: &Path, args: &[&str]) -> process::Result {
    process::run(
        &git.request(
            root,
            args.iter().copied(),
            Lifetime::AcceptedCommand,
            1024 * 1024,
        ),
        &Cancellation::default(),
        None,
    )
    .unwrap()
}
fn command(git: &Git, root: &Path, args: &[&str]) -> String {
    let result = raw(git, root, args);
    assert!(result.success(), "{result:?}");
    String::from_utf8(result.stdout)
        .unwrap()
        .trim_end()
        .to_owned()
}
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    git: Git,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        fs::create_dir(&root).unwrap();
        let config = temp.path().join("isolated.config");
        fs::write(&config, "[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n[branch]\nautoSetupMerge=always\n").unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        command(&git, &root, &["init", "--initial-branch=main"]);
        fs::write(root.join("file"), "base\n").unwrap();
        command(&git, &root, &["add", "file"]);
        command(&git, &root, &["commit", "-m", "base"]);
        command(&git, &root, &["branch", "target"]);
        Self {
            _temp: temp,
            root,
            git,
        }
    }
    fn snapshot(&self) -> Snapshot {
        self.git
            .inspect(&self.root, &Cancellation::default())
            .unwrap()
    }
    fn reference(&self, name: &str) -> Reference {
        self.snapshot()
            .references
            .into_iter()
            .find(|r| r.name == name)
            .unwrap()
    }
    fn checkout(&self, target: Checkout, changes: Changes) -> shovel_core::operation::Completed {
        let review = self
            .git
            .working_copy(&self.root, &Cancellation::default())
            .unwrap();
        let context = self
            .git
            .prepare_local(&review.repository, false, &Cancellation::default())
            .unwrap();
        self.git
            .execute_branch(
                &context,
                &Action::Checkout {
                    target,
                    review: Arc::new(review),
                    changes,
                    stash_message: "fixture checkout".into(),
                },
                None,
            )
            .unwrap()
    }
    fn manage(
        &self,
        branch: Reference,
        action: Manage,
    ) -> Result<shovel_core::operation::Completed, Error> {
        let context = self
            .git
            .prepare_local(&self.snapshot(), true, &Cancellation::default())
            .unwrap();
        self.git
            .execute_branch(&context, &Action::Manage { branch, action }, None)
    }
}
#[test]
fn create_switch_and_detach_run_native_hooks_without_inheriting_upstream() {
    let f = Fixture::new();
    let cancel = Cancellation::default();
    let snapshot = f.snapshot();
    let start = snapshot
        .references
        .iter()
        .find(|r| r.name == "HEAD")
        .unwrap()
        .clone();
    let context = f.git.prepare_local(&snapshot, true, &cancel).unwrap();
    let created = f
        .git
        .execute_branch(
            &context,
            &Action::Create {
                name: "feature/שלום".into(),
                start: start.clone(),
            },
            None,
        )
        .unwrap();
    assert!(
        created.succeeded(),
        "{} {:?}",
        created.output,
        created.message
    );
    assert_eq!(
        raw(
            &f.git,
            &f.root,
            &["config", "--get", "branch.feature/שלום.remote"]
        )
        .exit_code,
        1
    );
    assert_eq!(f.snapshot().branch.as_deref(), Some("main"));
    fs::write(
        f.root.join(".git/hooks/post-checkout"),
        "#!/bin/sh\nprintf '%s %s %s\\n' \"$1\" \"$2\" \"$3\" >> .git/checkout-ran\n",
    )
    .unwrap();
    let switched = f.checkout(
        Checkout::Branch(f.reference("refs/heads/feature/שלום")),
        Changes::Keep,
    );
    assert!(
        switched.succeeded(),
        "{} {:?}",
        switched.output,
        switched.message
    );
    assert_eq!(f.snapshot().branch.as_deref(), Some("feature/שלום"));
    let detached = f.checkout(Checkout::Detached(start.hash.clone()), Changes::Keep);
    assert!(
        detached.succeeded(),
        "{} {:?}",
        detached.output,
        detached.message
    );
    assert!(f.snapshot().branch.is_none());
    assert_eq!(f.snapshot().head.as_deref(), Some(start.hash.as_str()));
    assert_eq!(
        fs::read_to_string(f.root.join(".git/checkout-ran"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}
#[test]
fn checkout_stash_restores_exact_index_worktree_untracked_and_drops_only_its_entry() {
    let f = Fixture::new();
    fs::write(f.root.join("file"), "staged\n").unwrap();
    command(&f.git, &f.root, &["add", "file"]);
    fs::write(f.root.join("file"), "unstaged\n").unwrap();
    fs::write(f.root.join("untracked"), "keep\n").unwrap();
    let index = command(&f.git, &f.root, &["show", ":file"]);
    let result = f.checkout(
        Checkout::Branch(f.reference("refs/heads/target")),
        Changes::StashAndReapply,
    );
    assert!(result.succeeded(), "{} {:?}", result.output, result.message);
    assert_eq!(command(&f.git, &f.root, &["show", ":file"]), index);
    assert_eq!(
        fs::read_to_string(f.root.join("file")).unwrap(),
        "unstaged\n"
    );
    assert_eq!(
        fs::read_to_string(f.root.join("untracked")).unwrap(),
        "keep\n"
    );
    assert!(command(&f.git, &f.root, &["stash", "list"]).is_empty());
}
#[test]
fn keep_rejects_collision_and_discard_preserves_untracked_ignored_files() {
    let f = Fixture::new();
    command(&f.git, &f.root, &["switch", "target"]);
    fs::write(f.root.join("file"), "target\n").unwrap();
    command(&f.git, &f.root, &["commit", "-am", "target"]);
    command(&f.git, &f.root, &["switch", "main"]);
    fs::write(f.root.join("file"), "local\n").unwrap();
    fs::write(f.root.join("untracked"), "untracked\n").unwrap();
    fs::write(f.root.join(".git/info/exclude"), "ignored\n").unwrap();
    fs::write(f.root.join("ignored"), "ignored\n").unwrap();
    let target = f.reference("refs/heads/target");
    let kept = f.checkout(Checkout::Branch(target.clone()), Changes::Keep);
    assert!(!kept.succeeded());
    assert_eq!(f.snapshot().branch.as_deref(), Some("main"));
    assert_eq!(fs::read_to_string(f.root.join("file")).unwrap(), "local\n");
    let discarded = f.checkout(Checkout::Branch(target), Changes::DiscardTracked);
    assert!(
        discarded.succeeded(),
        "{} {:?}",
        discarded.output,
        discarded.message
    );
    assert_eq!(fs::read_to_string(f.root.join("file")).unwrap(), "target\n");
    assert_eq!(
        fs::read_to_string(f.root.join("untracked")).unwrap(),
        "untracked\n"
    );
    assert_eq!(
        fs::read_to_string(f.root.join("ignored")).unwrap(),
        "ignored\n"
    );
}
#[test]
fn failed_checkout_hook_retains_stash_and_actual_changed_head() {
    let f = Fixture::new();
    fs::write(f.root.join("file"), "dirty\n").unwrap();
    fs::write(
        f.root.join(".git/hooks/post-checkout"),
        "#!/bin/sh\nprintf 'fixture hook failed\\n'\nexit 1\n",
    )
    .unwrap();
    let result = f.checkout(
        Checkout::Branch(f.reference("refs/heads/target")),
        Changes::StashAndReapply,
    );
    assert!(!result.succeeded());
    assert_eq!(result.warning.as_deref(), Some("CheckoutStashRetained"));
    assert_eq!(f.snapshot().branch.as_deref(), Some("target"));
    assert!(result.output.contains("fixture hook failed"));
    assert!(command(&f.git, &f.root, &["stash", "list"]).contains("fixture checkout"));
    assert_eq!(
        command(&f.git, &f.root, &["show", "stash@{0}:file"]),
        "dirty"
    );
}
#[test]
fn checkout_rejects_new_staged_work_but_accepts_unrelated_fetched_refs() {
    let f = Fixture::new();
    let cancel = Cancellation::default();
    let review = f.git.working_copy(&f.root, &cancel).unwrap();
    let target = f.reference("refs/heads/target");
    let context = f
        .git
        .prepare_local(&review.repository, false, &cancel)
        .unwrap();
    fs::write(f.root.join("file"), "new staged\n").unwrap();
    command(&f.git, &f.root, &["add", "file"]);
    let action = Action::Checkout {
        target: Checkout::Branch(target.clone()),
        review: Arc::new(review),
        changes: Changes::DiscardTracked,
        stash_message: String::new(),
    };
    assert!(matches!(
        f.git.execute_branch(&context, &action, None),
        Err(Error::Code("CheckoutChanged"))
    ));
    assert_eq!(command(&f.git, &f.root, &["show", ":file"]), "new staged");
    let review = f.git.working_copy(&f.root, &cancel).unwrap();
    let context = f
        .git
        .prepare_local(&review.repository, false, &cancel)
        .unwrap();
    command(
        &f.git,
        &f.root,
        &["update-ref", "refs/remotes/fetched/unrelated", "HEAD"],
    );
    let action = Action::Checkout {
        target: Checkout::Branch(target),
        review: Arc::new(review),
        changes: Changes::Keep,
        stash_message: String::new(),
    };
    let result = f.git.execute_branch(&context, &action, None).unwrap();
    assert!(result.succeeded(), "{} {:?}", result.output, result.message);
    assert_eq!(command(&f.git, &f.root, &["show", ":file"]), "new staged");
}
#[test]
fn branch_management_checks_upstream_and_current_linked_unmerged_guards() {
    let f = Fixture::new();
    command(
        &f.git,
        &f.root,
        &["remote", "add", "origin", "https://example.invalid/fixture"],
    );
    command(
        &f.git,
        &f.root,
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    let target = f.reference("refs/heads/target");
    let upstream = f.reference("refs/remotes/origin/main");
    assert!(
        f.manage(target.clone(), Manage::SetUpstream(upstream))
            .unwrap()
            .succeeded()
    );
    assert_eq!(
        command(
            &f.git,
            &f.root,
            &["config", "--get", "branch.target.remote"]
        ),
        "origin"
    );
    assert!(
        f.manage(f.reference("refs/heads/target"), Manage::UnsetUpstream)
            .unwrap()
            .succeeded()
    );
    assert_eq!(
        raw(
            &f.git,
            &f.root,
            &["config", "--get", "branch.target.remote"]
        )
        .exit_code,
        1
    );
    assert!(matches!(
        f.manage(f.reference("refs/heads/main"), Manage::Delete),
        Err(Error::Code("BranchManageCurrent"))
    ));
    let linked = f.root.parent().unwrap().join("linked");
    command(
        &f.git,
        &f.root,
        &["worktree", "add", linked.to_str().unwrap(), "target"],
    );
    assert!(matches!(
        f.manage(f.reference("refs/heads/target"), Manage::Delete),
        Err(Error::Code("BranchManageOccupied"))
    ));
    command(
        &f.git,
        &f.root,
        &["worktree", "remove", linked.to_str().unwrap()],
    );
    assert!(
        f.manage(f.reference("refs/heads/target"), Manage::Delete)
            .unwrap()
            .succeeded()
    );
    assert!(
        !f.snapshot()
            .references
            .iter()
            .any(|r| r.name == "refs/heads/target")
    );
    command(&f.git, &f.root, &["switch", "-c", "unmerged"]);
    command(
        &f.git,
        &f.root,
        &["commit", "--allow-empty", "-m", "unmerged"],
    );
    command(&f.git, &f.root, &["switch", "main"]);
    let unmerged = f.reference("refs/heads/unmerged");
    let refused = f.manage(unmerged.clone(), Manage::Delete).unwrap();
    assert!(!refused.succeeded());
    assert_eq!(f.reference("refs/heads/unmerged").hash, unmerged.hash);
}
#[test]
fn concurrent_stash_creation_never_causes_another_entry_to_be_dropped() {
    let f = Fixture::new();
    let cancel = Cancellation::default();
    fs::write(f.root.join("file"), "keep\n").unwrap();
    let review = f.git.working_copy(&f.root, &cancel).unwrap();
    let context = f
        .git
        .prepare_local(&review.repository, false, &cancel)
        .unwrap();
    let target = f.reference("refs/heads/target");
    let git = f.git.clone();
    let root = f.root.clone();
    let progress = Arc::new(move |stage| {
        if stage == Stage::Reapplying {
            fs::write(root.join("newer"), "other data\n").unwrap();
            command(
                &git,
                &root,
                &[
                    "stash",
                    "push",
                    "--include-untracked",
                    "--message",
                    "concurrent entry",
                ],
            );
        }
    });
    let result = f
        .git
        .execute_branch(
            &context,
            &Action::Checkout {
                target: Checkout::Branch(target),
                review: Arc::new(review),
                changes: Changes::StashAndReapply,
                stash_message: "reviewed checkout".into(),
            },
            Some(progress),
        )
        .unwrap();
    assert_eq!(result.warning.as_deref(), Some("CheckoutStashRetained"));
    let list = command(&f.git, &f.root, &["stash", "list"]);
    assert!(result.succeeded(), "{} {:?}", result.output, result.message);
    assert!(list.contains("concurrent entry"));
    assert!(list.contains("reviewed checkout"));
    assert_eq!(fs::read_to_string(f.root.join("file")).unwrap(), "keep\n");
}
