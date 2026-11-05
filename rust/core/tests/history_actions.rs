#![cfg(windows)]
use shovel_core::{
    git::{Git, Snapshot},
    history_actions::{Action, Reset},
    operation::{Completed, Error},
    process::{self, Cancellation, Lifetime},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn command(git: &Git, root: &Path, args: &[&str]) -> String {
    let result = process::run(
        &git.request(
            root,
            args.iter().copied(),
            Lifetime::AcceptedCommand,
            1024 * 1024,
        ),
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
    base: String,
    tip: String,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("history");
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
        let base = command(&git, &root, &["rev-parse", "HEAD"]);
        fs::write(root.join("file"), "second\n").unwrap();
        command(&git, &root, &["commit", "-am", "second"]);
        let tip = command(&git, &root, &["rev-parse", "HEAD"]);
        Self {
            _temp: temp,
            root,
            git,
            base,
            tip,
        }
    }
    fn snapshot(&self) -> Snapshot {
        self.git
            .inspect(&self.root, &Cancellation::default())
            .unwrap()
    }
    fn run(&self, action: Action) -> Result<Completed, Error> {
        let context = self
            .git
            .prepare_local(&self.snapshot(), true, &Cancellation::default())?;
        self.git.execute_history(&context, &action, None)
    }
    fn cmd(&self, args: &[&str]) -> String {
        command(&self.git, &self.root, args)
    }
}
fn reset(mode: Reset) {
    let f = Fixture::new();
    fs::write(f.root.join("file"), "staged\n").unwrap();
    f.cmd(&["add", "file"]);
    fs::write(f.root.join("file"), "working\n").unwrap();
    fs::write(f.root.join("untracked"), "retained\n").unwrap();
    fs::write(f.root.join(".git/info/exclude"), "ignored\n").unwrap();
    fs::write(f.root.join("ignored"), "retained\n").unwrap();
    let result = f
        .run(Action::Reset {
            target: f.base.clone(),
            mode,
        })
        .unwrap();
    assert!(result.succeeded(), "{} {:?}", result.output, result.message);
    assert_eq!(f.cmd(&["rev-parse", "HEAD"]), f.base);
    assert_eq!(f.cmd(&["rev-parse", "ORIG_HEAD"]), f.tip);
    assert_eq!(
        f.cmd(&["show", ":file"]),
        if mode == Reset::Soft {
            "staged"
        } else {
            "base"
        }
    );
    assert_eq!(
        fs::read_to_string(f.root.join("file")).unwrap(),
        if mode == Reset::Hard {
            "base\n"
        } else {
            "working\n"
        }
    );
    assert_eq!(
        fs::read_to_string(f.root.join("untracked")).unwrap(),
        "retained\n"
    );
    assert_eq!(
        fs::read_to_string(f.root.join("ignored")).unwrap(),
        "retained\n"
    );
}
#[test]
fn soft_reset_keeps_index_and_working_files() {
    reset(Reset::Soft);
}
#[test]
fn mixed_reset_keeps_working_files_and_replaces_index() {
    reset(Reset::Mixed);
}
#[test]
fn hard_reset_replaces_tracked_files_and_retains_unrelated_files() {
    reset(Reset::Hard);
}

#[test]
fn revert_creates_native_commit_and_retains_hook_failure() {
    let f = Fixture::new();
    fs::write(
        f.root.join(".git/hooks/prepare-commit-msg"),
        "#!/bin/sh\necho native-hook >> hook-evidence\n",
    )
    .unwrap();
    let result = f
        .run(Action::Revert {
            target: f.tip.clone(),
            mainline: 0,
        })
        .unwrap();
    assert!(result.succeeded(), "{} {:?}", result.output, result.message);
    assert_eq!(f.cmd(&["rev-parse", "HEAD^"]), f.tip);
    assert_eq!(fs::read_to_string(f.root.join("file")).unwrap(), "base\n");
    assert_eq!(
        fs::read_to_string(f.root.join("hook-evidence")).unwrap(),
        "native-hook\n"
    );
    let revert = f.cmd(&["rev-parse", "HEAD"]);
    fs::write(
        f.root.join(".git/hooks/prepare-commit-msg"),
        "#!/bin/sh\necho native-refusal >&2\nexit 1\n",
    )
    .unwrap();
    let failed = f
        .run(Action::Revert {
            target: revert.clone(),
            mainline: 0,
        })
        .unwrap();
    assert!(!failed.succeeded());
    assert!(failed.output.contains("native-refusal"));
    assert_eq!(f.cmd(&["rev-parse", "HEAD"]), revert);
    assert_eq!(f.cmd(&["show", ":file"]), "second");
}

#[test]
fn merge_revert_requires_explicit_original_parent_and_preserves_other_parent_changes() {
    let f = Fixture::new();
    f.cmd(&["switch", "--create", "side", &f.base]);
    fs::write(f.root.join("side-file"), "side\n").unwrap();
    f.cmd(&["add", "side-file"]);
    f.cmd(&["commit", "-m", "side"]);
    f.cmd(&["switch", "main"]);
    f.cmd(&["merge", "--no-ff", "--no-edit", "side"]);
    let merge = f.cmd(&["rev-parse", "HEAD"]);
    assert_eq!(
        f.run(Action::Revert {
            target: merge.clone(),
            mainline: 0
        })
        .err()
        .unwrap()
        .code(),
        "HistoryRevertMainlineRequired"
    );
    let result = f
        .run(Action::Revert {
            target: merge.clone(),
            mainline: 1,
        })
        .unwrap();
    assert!(result.succeeded(), "{} {:?}", result.output, result.message);
    assert!(!f.root.join("side-file").exists());
    assert_eq!(fs::read_to_string(f.root.join("file")).unwrap(), "second\n");
    assert_eq!(f.cmd(&["rev-parse", "HEAD^"]), merge);
}

#[test]
fn conflicted_revert_keeps_sequence_index_and_original_head() {
    let f = Fixture::new();
    fs::write(f.root.join("file"), "later\n").unwrap();
    f.cmd(&["commit", "-am", "later"]);
    let head = f.cmd(&["rev-parse", "HEAD"]);
    let result = f
        .run(Action::Revert {
            target: f.tip.clone(),
            mainline: 0,
        })
        .unwrap();
    assert!(!result.succeeded());
    assert!(result.has_conflicts);
    assert!(
        result
            .updated
            .unwrap()
            .operations
            .contains(&"revert".into())
    );
    assert_eq!(f.cmd(&["rev-parse", "HEAD"]), head);
    assert_eq!(f.cmd(&["rev-parse", "REVERT_HEAD"]), f.tip);
    assert!(!f.cmd(&["ls-files", "--unmerged"]).is_empty());
}

#[test]
fn changed_head_refs_configuration_and_invalid_targets_do_not_mutate() {
    let f = Fixture::new();
    let context = f
        .git
        .prepare_local(&f.snapshot(), true, &Cancellation::default())
        .unwrap();
    f.cmd(&["config", "branch.main.description", "external"]);
    assert!(
        f.git
            .execute_history(
                &context,
                &Action::Reset {
                    target: f.base.clone(),
                    mode: Reset::Hard
                },
                None
            )
            .is_err()
    );
    let context = f
        .git
        .prepare_local(&f.snapshot(), true, &Cancellation::default())
        .unwrap();
    f.cmd(&["branch", "external"]);
    assert!(
        f.git
            .execute_history(
                &context,
                &Action::Reset {
                    target: f.base.clone(),
                    mode: Reset::Soft
                },
                None
            )
            .is_err()
    );
    assert_eq!(
        f.run(Action::Revert {
            target: "HEAD~1".into(),
            mainline: 0
        })
        .err()
        .unwrap()
        .code(),
        "HistoryTargetUnavailable"
    );
    let context = f
        .git
        .prepare_local(&f.snapshot(), true, &Cancellation::default())
        .unwrap();
    f.cmd(&["commit", "--allow-empty", "-m", "external"]);
    assert!(
        f.git
            .execute_history(
                &context,
                &Action::Reset {
                    target: f.base.clone(),
                    mode: Reset::Mixed
                },
                None
            )
            .is_err()
    );
    let current = f.cmd(&["rev-parse", "HEAD"]);
    f.cmd(&["switch", "--detach"]);
    assert_eq!(
        f.run(Action::Reset {
            target: f.base.clone(),
            mode: Reset::Hard
        })
        .err()
        .unwrap()
        .code(),
        "HistoryResetNeedsBranch"
    );
    assert_eq!(f.cmd(&["rev-parse", "HEAD"]), current);
    assert_eq!(fs::read_to_string(f.root.join("file")).unwrap(), "second\n");
}

fn conflict_revert(f: &Fixture) -> Snapshot {
    fs::write(f.root.join("file"), "later\n").unwrap();
    f.cmd(&["commit", "-am", "later"]);
    let result = f
        .run(Action::Revert {
            target: f.tip.clone(),
            mainline: 0,
        })
        .unwrap();
    assert!(result.has_conflicts);
    result.updated.unwrap()
}
#[test]
fn recovery_revert_continue_accepts_staged_resolution_and_keeps_native_message() {
    use shovel_core::recovery::Action as Recovery;
    let f = Fixture::new();
    let expected = conflict_revert(&f);
    assert_eq!(
        f.git
            .execute_recovery(&expected, Recovery::Continue, None)
            .err()
            .unwrap()
            .code(),
        "CommitUnresolved"
    );
    fs::write(f.root.join("file"), "resolved\n").unwrap();
    f.cmd(&["add", "file"]);
    let result = f
        .git
        .execute_recovery(&expected, Recovery::Continue, None)
        .unwrap();
    assert!(result.succeeded(), "{} {:?}", result.output, result.message);
    assert_eq!(f.cmd(&["rev-parse", "HEAD^"]), expected.head.unwrap());
    assert!(f.cmd(&["log", "-1", "--format=%B"]).contains(&f.tip));
    assert_eq!(
        fs::read_to_string(f.root.join("file")).unwrap(),
        "resolved\n"
    );
}
#[test]
fn recovery_revert_abort_and_skip_restore_native_preoperation_state() {
    use shovel_core::recovery::Action as Recovery;
    for action in [Recovery::Abort, Recovery::Skip] {
        let f = Fixture::new();
        let expected = conflict_revert(&f);
        assert!(expected.recovery.as_ref().unwrap().can_skip());
        fs::write(f.root.join("untracked"), "keep\n").unwrap();
        let result = f.git.execute_recovery(&expected, action, None).unwrap();
        assert!(result.succeeded(), "{} {:?}", result.output, result.message);
        assert_eq!(f.cmd(&["rev-parse", "HEAD"]), expected.head.unwrap());
        assert_eq!(fs::read_to_string(f.root.join("file")).unwrap(), "later\n");
        assert_eq!(
            fs::read_to_string(f.root.join("untracked")).unwrap(),
            "keep\n"
        );
    }
}
fn conflict_branch(f: &Fixture, command_name: &str) -> Snapshot {
    f.cmd(&["switch", "--create", "side", &f.base]);
    fs::write(f.root.join("file"), "side\n").unwrap();
    f.cmd(&["commit", "-am", "side"]);
    let args = match command_name {
        "merge" => vec!["merge", "--no-edit", "main"],
        "rebase" => vec!["rebase", "main"],
        "cherry-pick" => vec!["cherry-pick", "main"],
        _ => panic!(),
    };
    let result = process::run(
        &f.git
            .request(&f.root, args, Lifetime::AcceptedCommand, 65536),
        &Cancellation::default(),
        None,
    )
    .unwrap();
    assert_ne!(result.exit_code, 0);
    f.snapshot()
}
#[test]
fn recovery_merge_continue_preserves_parents_and_rejects_skip() {
    use shovel_core::recovery::{Action as Recovery, Kind};
    let f = Fixture::new();
    let expected = conflict_branch(&f, "merge");
    assert_eq!(expected.recovery.as_ref().unwrap().kind, Kind::Merge);
    assert!(!expected.recovery.as_ref().unwrap().can_skip());
    assert_eq!(
        f.git
            .execute_recovery(&expected, Recovery::Skip, None)
            .err()
            .unwrap()
            .code(),
        "RecoveryUnavailable"
    );
    fs::write(f.root.join("file"), "merged\n").unwrap();
    f.cmd(&["add", "file"]);
    let result = f
        .git
        .execute_recovery(&expected, Recovery::Continue, None)
        .unwrap();
    assert!(result.succeeded(), "{} {:?}", result.output, result.message);
    assert_eq!(f.cmd(&["rev-parse", "HEAD^1"]), expected.head.unwrap());
    assert_eq!(f.cmd(&["rev-parse", "HEAD^2"]), f.tip);
}
fn rebase_recovery(action: shovel_core::recovery::Action) {
    use shovel_core::recovery::{Action as Recovery, Kind};
    let f = Fixture::new();
    let expected = conflict_branch(&f, "rebase");
    let state = expected.recovery.as_ref().unwrap();
    assert_eq!(state.kind, Kind::Rebase);
    assert_eq!(state.branch, "side");
    assert!(state.can_skip());
    if action == Recovery::Continue {
        fs::write(f.root.join("file"), "rebased\n").unwrap();
        f.cmd(&["add", "file"]);
    }
    let result = f.git.execute_recovery(&expected, action, None).unwrap();
    assert!(result.succeeded(), "{} {:?}", result.output, result.message);
    assert_eq!(f.cmd(&["branch", "--show-current"]), "side");
    match action {
        Recovery::Abort => {
            assert_eq!(f.cmd(&["rev-parse", "HEAD"]), state.target);
            assert_eq!(fs::read_to_string(f.root.join("file")).unwrap(), "side\n");
        }
        Recovery::Skip => {
            assert_eq!(f.cmd(&["rev-parse", "HEAD"]), f.tip);
        }
        Recovery::Continue => {
            assert_eq!(f.cmd(&["rev-parse", "HEAD^"]), f.tip);
            assert_eq!(
                fs::read_to_string(f.root.join("file")).unwrap(),
                "rebased\n"
            );
        }
    }
}
#[test]
fn recovery_rebase_continue_preserves_branch_and_commits_resolution() {
    rebase_recovery(shovel_core::recovery::Action::Continue);
}
#[test]
fn recovery_rebase_skip_discards_current_resolution() {
    rebase_recovery(shovel_core::recovery::Action::Skip);
}
#[test]
fn recovery_rebase_abort_restores_original_tip() {
    rebase_recovery(shovel_core::recovery::Action::Abort);
}
#[test]
fn recovery_rejects_changed_state_before_native_mutation_and_aborts_cherry_pick() {
    use shovel_core::recovery::{Action as Recovery, Kind};
    let f = Fixture::new();
    let expected = conflict_branch(&f, "cherry-pick");
    assert_eq!(expected.recovery.as_ref().unwrap().kind, Kind::CherryPick);
    let message_path = f.root.join(".git/MERGE_MSG");
    let original = fs::read(&message_path).unwrap();
    fs::write(&message_path, "changed outside Shovel").unwrap();
    assert_eq!(
        f.git
            .execute_recovery(&expected, Recovery::Abort, None)
            .err()
            .unwrap()
            .code(),
        "RecoveryChanged"
    );
    assert!(f.root.join(".git/CHERRY_PICK_HEAD").exists());
    fs::write(&message_path, original).unwrap();
    let result = f
        .git
        .execute_recovery(&expected, Recovery::Abort, None)
        .unwrap();
    assert!(result.succeeded(), "{} {:?}", result.output, result.message);
    assert_eq!(f.cmd(&["rev-parse", "HEAD"]), expected.head.unwrap());
    assert_eq!(fs::read_to_string(f.root.join("file")).unwrap(), "side\n");
}
