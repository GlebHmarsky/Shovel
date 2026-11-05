#![cfg(windows)]
use shovel_core::{
    branches::Stage,
    git::{Git, Snapshot},
    process::{self, Cancellation, Lifetime},
    stashes::{Action, Target},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
fn command(git: &Git, root: &Path, args: &[&str]) -> String {
    let result = process::run(
        &git.request(root, args.iter().copied(), Lifetime::AcceptedCommand, 65536),
        &Cancellation::default(),
        None,
    )
    .unwrap();
    assert!(result.success(), "{result:?}");
    String::from_utf8(result.stdout).unwrap().trim_end().into()
}
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    git: Git,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("stash-repository");
        fs::create_dir(&root).unwrap();
        let config = temp.path().join("isolated.config");
        fs::write(&config, "[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n").unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        command(&git, &root, &["init", "--initial-branch=main"]);
        fs::write(root.join("file"), "base\n").unwrap();
        command(&git, &root, &["add", "file"]);
        command(&git, &root, &["commit", "-m", "base"]);
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
    fn create(&self, message: &str, include_untracked: bool) -> Target {
        let result = self
            .git
            .execute_stash(
                &self.snapshot(),
                &Action::Create {
                    message: message.into(),
                    include_untracked,
                },
                None,
            )
            .unwrap();
        assert!(result.succeeded(), "{} {:?}", result.output, result.message);
        let list = self
            .git
            .stashes(&self.snapshot(), &Cancellation::default())
            .unwrap();
        Target::reviewed(&list, &list.entries[0])
    }
}
#[test]
fn create_inspect_and_pop_preserve_exact_index_worktree_and_untracked_content() {
    let f = Fixture::new();
    let cancel = Cancellation::default();
    fs::write(f.root.join("file"), "staged\n").unwrap();
    command(&f.git, &f.root, &["add", "file"]);
    fs::write(f.root.join("file"), "working\n").unwrap();
    fs::write(f.root.join("untracked"), "saved\n").unwrap();
    fs::write(f.root.join(".git/info/exclude"), "ignored\n").unwrap();
    fs::write(f.root.join("ignored"), "kept\n").unwrap();
    let target = f.create("שלום {0} saved", true);
    assert_eq!(fs::read_to_string(f.root.join("file")).unwrap(), "base\n");
    assert!(!f.root.join("untracked").exists());
    let snapshot = f.snapshot();
    let inspection = f
        .git
        .inspect_stash(&snapshot, &target.hash, &cancel)
        .unwrap();
    assert!(inspection.worktree.message.contains("שלום {0} saved"));
    assert_eq!(inspection.worktree.parents[1], inspection.index.hash);
    for (commit, parent, path, old, new) in [
        (
            &inspection.worktree.hash,
            Some(inspection.worktree.parents[0].as_str()),
            "file",
            "base",
            "working",
        ),
        (
            &inspection.index.hash,
            Some(inspection.worktree.parents[0].as_str()),
            "file",
            "base",
            "staged",
        ),
        (
            &inspection.worktree.hash,
            Some(inspection.index.hash.as_str()),
            "file",
            "staged",
            "working",
        ),
        (
            &inspection.untracked.as_ref().unwrap().hash,
            None,
            "untracked",
            "",
            "saved",
        ),
    ] {
        let files = f
            .git
            .commit_files(&f.root, commit, parent, &cancel)
            .unwrap();
        let entry = files.entries.iter().find(|e| e.path == path).unwrap();
        let diff = f
            .git
            .commit_file_diff(&f.root, &files, entry, &cancel)
            .unwrap();
        let lines = format!("{diff:?}");
        assert!(lines.contains(new));
        if !old.is_empty() {
            assert!(lines.contains(old));
        }
    }
    let result = f
        .git
        .execute_stash(
            &snapshot,
            &Action::Apply {
                target,
                restore_index: true,
                pop: true,
            },
            None,
        )
        .unwrap();
    assert!(result.succeeded(), "{} {:?}", result.output, result.message);
    assert_eq!(command(&f.git, &f.root, &["show", ":file"]), "staged");
    assert_eq!(
        fs::read_to_string(f.root.join("file")).unwrap(),
        "working\n"
    );
    assert_eq!(
        fs::read_to_string(f.root.join("untracked")).unwrap(),
        "saved\n"
    );
    assert_eq!(
        fs::read_to_string(f.root.join("ignored")).unwrap(),
        "kept\n"
    );
    assert!(
        f.git
            .stashes(&f.snapshot(), &cancel)
            .unwrap()
            .entries
            .is_empty()
    );
}
#[test]
fn conflicted_pop_retains_stash_and_does_not_invent_a_sequence() {
    let f = Fixture::new();
    fs::write(f.root.join("file"), "stash\n").unwrap();
    let target = f.create("saved", false);
    fs::write(f.root.join("file"), "later\n").unwrap();
    command(&f.git, &f.root, &["commit", "-am", "later"]);
    let result = f
        .git
        .execute_stash(
            &f.snapshot(),
            &Action::Apply {
                target: target.clone(),
                restore_index: false,
                pop: true,
            },
            None,
        )
        .unwrap();
    assert!(!result.succeeded());
    assert!(result.has_conflicts);
    assert_eq!(result.warning.as_deref(), Some("StashApplyFailed"));
    let updated = result.updated.unwrap();
    assert!(updated.operations.is_empty());
    assert!(updated.recovery.is_none());
    assert_eq!(
        f.git
            .stashes(&updated, &Cancellation::default())
            .unwrap()
            .entries[0]
            .hash,
        target.hash
    );
    let before = command(&f.git, &f.root, &["ls-files", "--unmerged", "-z"]);
    let dropped = f
        .git
        .execute_stash(&updated, &Action::Drop(target), None)
        .unwrap();
    assert!(
        dropped.succeeded(),
        "Drop must report its own result despite pre-existing conflicts"
    );
    assert!(dropped.has_conflicts);
    assert_eq!(
        command(&f.git, &f.root, &["ls-files", "--unmerged", "-z"]),
        before
    );
}
#[test]
fn stale_ordinals_and_new_entries_during_pop_never_remove_another_stash() {
    let f = Fixture::new();
    fs::write(f.root.join("file"), "first\n").unwrap();
    let first = f.create("first", false);
    fs::write(f.root.join("file"), "second\n").unwrap();
    let second = f.create("second", false);
    let earlier_hash = first.hash.clone();
    assert_eq!(
        f.git
            .execute_stash(&f.snapshot(), &Action::Drop(first), None)
            .err()
            .unwrap()
            .code(),
        "StashChanged"
    );
    let git = f.git.clone();
    let root = f.root.clone();
    let hash = earlier_hash.clone();
    let result = f
        .git
        .execute_stash(
            &f.snapshot(),
            &Action::Apply {
                target: second.clone(),
                restore_index: false,
                pop: true,
            },
            Some(Arc::new(move |stage| {
                if stage == Stage::DroppingStash {
                    command(
                        &git,
                        &root,
                        &["stash", "store", "--message=newer external", &hash],
                    );
                }
            })),
        )
        .unwrap();
    assert!(result.succeeded(), "{} {:?}", result.output, result.message);
    assert_eq!(result.warning.as_deref(), Some("StashAppliedRetained"));
    let list = f
        .git
        .stashes(&f.snapshot(), &Cancellation::default())
        .unwrap();
    assert_eq!(list.entries.len(), 3);
    assert_eq!(list.entries[0].hash, earlier_hash);
    assert_eq!(list.entries[1].hash, second.hash);
    assert_eq!(fs::read_to_string(f.root.join("file")).unwrap(), "second\n");
}
#[test]
fn explicit_drop_removes_only_reviewed_saved_data_and_preserves_worktree() {
    let f = Fixture::new();
    fs::write(f.root.join("file"), "stash\n").unwrap();
    let target = f.create("drop", false);
    fs::write(f.root.join("file"), "unrelated\n").unwrap();
    fs::write(f.root.join("untracked"), "keep\n").unwrap();
    let result = f
        .git
        .execute_stash(&f.snapshot(), &Action::Drop(target), None)
        .unwrap();
    assert!(result.succeeded());
    assert!(
        f.git
            .stashes(&f.snapshot(), &Cancellation::default())
            .unwrap()
            .entries
            .is_empty()
    );
    assert_eq!(
        fs::read_to_string(f.root.join("file")).unwrap(),
        "unrelated\n"
    );
    assert_eq!(
        fs::read_to_string(f.root.join("untracked")).unwrap(),
        "keep\n"
    );
}

#[test]
fn failed_pop_removal_reports_applied_content_without_retrying_or_losing_saved_data() {
    let f = Fixture::new();
    fs::write(f.root.join("file"), "applied once\n").unwrap();
    let target = f.create("retained on failed removal", false);
    let lock = f.root.join(".git/refs/stash.lock");
    let result = f
        .git
        .execute_stash(
            &f.snapshot(),
            &Action::Apply {
                target: target.clone(),
                restore_index: false,
                pop: true,
            },
            Some(Arc::new(move |stage| {
                if stage == Stage::DroppingStash {
                    fs::write(&lock, "external lock").unwrap();
                }
            })),
        )
        .unwrap();
    assert!(!result.succeeded());
    assert_eq!(result.commands.len(), 2);
    assert!(result.commands[0].succeeded());
    assert!(!result.commands[1].succeeded());
    assert_eq!(result.warning.as_deref(), Some("StashAppliedRetained"));
    assert_eq!(
        fs::read_to_string(f.root.join("file")).unwrap(),
        "applied once\n"
    );
    assert_eq!(
        f.git
            .stashes(&f.snapshot(), &Cancellation::default())
            .unwrap()
            .entries[0]
            .hash,
        target.hash
    );
}
