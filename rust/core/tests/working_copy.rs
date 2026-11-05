#![cfg(windows)]
use shovel_core::{
    git::Git,
    process::{self, Cancellation, Lifetime},
    working::{Action, Error, Group, MutationGate},
};
use std::{fs, path::Path};
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
    String::from_utf8(result.stdout).unwrap()
}
fn fixture() -> (tempfile::TempDir, Git) {
    let temp = tempfile::tempdir().unwrap();
    let mut git = Git::discover(None, &Cancellation::default()).unwrap();
    let config = temp.path().join("isolated.gitconfig");
    fs::write(
        &config,
        "[user]\nname = Fixture\nemail = fixture@example.invalid\n[commit]\ngpgsign = false\n",
    )
    .unwrap();
    git.environment
        .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
    git.environment
        .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
    command(&git, temp.path(), &["init", "--initial-branch=main"]);
    (temp, git)
}
#[test]
fn initial_stage_unstage_commit_and_partial_staging_preserve_exact_content() {
    let (temp, git) = fixture();
    let root = temp.path();
    let cancel = Cancellation::default();
    let file = "-שלום [literal].txt";
    fs::write(root.join(file), "first\n").unwrap();
    let review = git.working_copy(root, &cancel).unwrap();
    let chosen = review
        .entries
        .iter()
        .find(|e| e.path == file)
        .unwrap()
        .clone();
    let staged = git
        .execute_working(&review, &Action::Stage(vec![chosen]), None)
        .unwrap();
    assert!(staged.command.success());
    let staged = staged.updated.unwrap();
    let row = staged
        .entries
        .iter()
        .find(|e| e.path == file && e.group == Group::Staged)
        .unwrap()
        .clone();
    let unstage = git
        .execute_working(&staged, &Action::Unstage(vec![row]), None)
        .unwrap();
    assert!(unstage.command.success());
    assert_eq!(fs::read_to_string(root.join(file)).unwrap(), "first\n");
    let review = unstage.updated.unwrap();
    let selected = review
        .entries
        .iter()
        .filter(|e| e.path == file)
        .cloned()
        .collect();
    let stage = git
        .execute_working(&review, &Action::Stage(selected), None)
        .unwrap();
    assert!(stage.command.success());
    let review = stage.updated.unwrap();
    let result = git
        .execute_working(
            &review,
            &Action::Commit {
                message: "Initial שלום\n\nbody\n".into(),
                amend: false,
            },
            None,
        )
        .unwrap();
    assert!(result.command.success());
    assert!(result.commit.is_some());
    assert_eq!(
        command(&git, root, &["show", &format!("HEAD:{file}")]),
        "first\n"
    );
    fs::write(root.join(file), "staged\n").unwrap();
    let review = git.working_copy(root, &cancel).unwrap();
    let selected = review
        .entries
        .iter()
        .filter(|e| e.path == file)
        .cloned()
        .collect();
    let result = git
        .execute_working(&review, &Action::Stage(selected), None)
        .unwrap();
    assert!(result.command.success());
    fs::write(root.join(file), "working\n").unwrap();
    let review = git.working_copy(root, &cancel).unwrap();
    assert_eq!(review.entries.iter().filter(|e| e.path == file).count(), 2);
    assert!(
        git.working_diff(
            &review,
            review
                .entries
                .iter()
                .find(|e| e.path == file && e.group == Group::Staged)
                .unwrap(),
            &cancel
        )
        .unwrap()
        .contains("+staged")
    );
    let chosen = review
        .entries
        .iter()
        .find(|e| e.path == file && e.group == Group::Staged)
        .unwrap()
        .clone();
    let result = git
        .execute_working(&review, &Action::Unstage(vec![chosen]), None)
        .unwrap();
    assert!(result.command.success());
    assert_eq!(fs::read_to_string(root.join(file)).unwrap(), "working\n");
    assert_eq!(
        command(&git, root, &["show", &format!(":{file}")]),
        "first\n"
    );
}
#[test]
fn commit_rejects_changed_index_but_accepts_stat_refresh() {
    let (temp, git) = fixture();
    let root = temp.path();
    let cancel = Cancellation::default();
    fs::write(root.join("a"), "one\n").unwrap();
    command(&git, root, &["add", "--", "a"]);
    let review = git.working_copy(root, &cancel).unwrap();
    command(&git, root, &["update-index", "--refresh"]);
    assert_eq!(
        review.index_stamp,
        git.semantic_index(&review.repository, &cancel).unwrap()
    );
    let result = git
        .execute_working(
            &review,
            &Action::Commit {
                message: "one".into(),
                amend: false,
            },
            None,
        )
        .unwrap();
    assert!(result.command.success());
    fs::write(root.join("a"), "two\n").unwrap();
    command(&git, root, &["add", "--", "a"]);
    let review = git.working_copy(root, &cancel).unwrap();
    fs::write(root.join("a"), "three\n").unwrap();
    command(&git, root, &["add", "--", "a"]);
    assert!(matches!(
        git.execute_working(
            &review,
            &Action::Commit {
                message: "must not commit".into(),
                amend: false
            },
            None
        ),
        Err(Error::Stale)
    ));
    assert_eq!(command(&git, root, &["show", ":a"]), "three\n");
    assert_eq!(
        command(&git, root, &["log", "-1", "--format=%s"]).trim(),
        "one"
    );
}
#[test]
fn failed_native_hook_retains_index_and_reports_failure() {
    let (temp, git) = fixture();
    let root = temp.path();
    let cancel = Cancellation::default();
    fs::write(root.join("file"), "payload\n").unwrap();
    command(&git, root, &["add", "--", "file"]);
    fs::write(
        root.join(".git/hooks/pre-commit"),
        "#!/bin/sh\nprintf 'blocked by fixture\\n' >&2\nexit 1\n",
    )
    .unwrap();
    let review = git.working_copy(root, &cancel).unwrap();
    let result = git
        .execute_working(
            &review,
            &Action::Commit {
                message: "retained message".into(),
                amend: false,
            },
            None,
        )
        .unwrap();
    assert!(!result.command.success());
    assert!(
        String::from_utf8(result.command.stderr)
            .unwrap()
            .contains("blocked by fixture")
    );
    assert_eq!(review.index_stamp, result.updated.unwrap().index_stamp);
    assert!(result.commit.is_none());
    assert_eq!(command(&git, root, &["show", ":file"]), "payload\n");
}
#[test]
fn linked_worktree_gate_conflicts_on_another_thread() {
    let (temp, git) = fixture();
    let root = temp.path();
    let snapshot = git.inspect(root, &Cancellation::default()).unwrap();
    let common = snapshot.common_directory;
    let guard = MutationGate::acquire(&common, false).unwrap();
    let other = std::thread::spawn(move || {
        matches!(
            MutationGate::acquire(&common, false),
            Err(Error::OtherInstance)
        )
    });
    assert!(other.join().unwrap());
    drop(guard);
}

#[test]
fn standard_lfs_status_hashes_hydrated_content_without_running_helpers() {
    use sha2::{Digest, Sha256};
    let (temp, git) = fixture();
    let root = temp.path();
    let cancel = Cancellation::default();
    let asset = b"hydrated asset content\n";
    let pointer = format!(
        "version https://git-lfs.github.com/spec/v1\noid sha256:{:x}\nsize {}\n",
        Sha256::digest(asset),
        asset.len()
    );
    fs::write(root.join("art.asset"), pointer).unwrap();
    fs::write(root.join(".gitattributes"), "*.asset filter=lfs\n").unwrap();
    command(&git, root, &["add", "--all"]);
    command(&git, root, &["commit", "-m", "pointer"]);
    command(
        &git,
        root,
        &["config", "filter.lfs.clean", "git-lfs clean -- %f"],
    );
    command(
        &git,
        root,
        &["config", "filter.lfs.process", "git-lfs filter-process"],
    );
    command(&git, root, &["config", "filter.lfs.required", "true"]);
    fs::write(root.join("art.asset"), asset).unwrap();
    let review = git.working_copy(root, &cancel).unwrap();
    assert!(!review.entries.iter().any(|e| e.path == "art.asset"));
    let mut changed = asset.to_vec();
    changed[0] = b'X';
    fs::write(root.join("art.asset"), changed).unwrap();
    let review = git.working_copy(root, &cancel).unwrap();
    assert!(
        review
            .entries
            .iter()
            .any(|e| e.path == "art.asset" && e.group == Group::Unstaged)
    );
    // A custom clean transform must not be mistaken for standard LFS semantics.
    command(
        &git,
        root,
        &[
            "config",
            "filter.lfs.clean",
            "sh -c 'touch helper-ran; exit 1'",
        ],
    );
    fs::write(root.join("art.asset"), asset).unwrap();
    let review = git.working_copy(root, &cancel).unwrap();
    assert!(review.entries.iter().any(|e| e.path == "art.asset"));
    assert!(!root.join("helper-ran").exists());
}
