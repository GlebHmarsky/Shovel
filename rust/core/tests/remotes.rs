#![cfg(windows)]
use shovel_core::{
    branches::Stage,
    git::{Git, Snapshot},
    process::{self, Cancellation, Lifetime},
    remotes::{Action, Configuration, valid},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
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
    temp: tempfile::TempDir,
    root: PathBuf,
    git: Git,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repository");
        fs::create_dir(&root).unwrap();
        let config = temp.path().join("isolated.config");
        fs::write(&config, "[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n").unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        for name in ["GIT_AUTHOR_DATE", "GIT_COMMITTER_DATE"] {
            git.environment
                .insert(name.into(), Some("2026-01-01T00:00:00+00:00".into()));
        }
        command(&git, &root, &["init", "--initial-branch=main"]);
        fs::write(root.join("file"), "base\n").unwrap();
        command(&git, &root, &["add", "file"]);
        command(&git, &root, &["commit", "-m", "base"]);
        fs::write(root.join("file"), "staged\n").unwrap();
        command(&git, &root, &["add", "file"]);
        fs::write(root.join("file"), "working\n").unwrap();
        Self { temp, root, git }
    }
    fn snapshot(&self) -> Snapshot {
        self.git
            .inspect(&self.root, &Cancellation::default())
            .unwrap()
    }
    fn config(&self) -> Configuration {
        self.git
            .remote_configuration(&self.snapshot(), &Cancellation::default())
            .unwrap()
    }
    fn add(&self, name: &str, location: &str) -> shovel_core::operation::Completed {
        self.git
            .execute_remote(
                &self.snapshot(),
                &Action::Add {
                    name: name.into(),
                    location: location.into(),
                },
                None,
            )
            .unwrap()
    }
    fn change(&self, location: &str, push: &str, remove: bool) -> Action {
        Action::Change {
            reviewed: self.config(),
            name: "origin".into(),
            location: location.into(),
            push_location: push.into(),
            remove,
        }
    }
    fn preserved(&self) -> (String, Vec<u8>, Vec<u8>) {
        (
            command(
                &self.git,
                &self.root,
                &[
                    "for-each-ref",
                    "--format=%(refname) %(objectname)",
                    "refs/heads/",
                ],
            ),
            fs::read(self.root.join(".git/index")).unwrap(),
            fs::read(self.root.join("file")).unwrap(),
        )
    }
    fn record(&self, case: &str, success: bool, commands: usize) -> serde_json::Value {
        let status = command(&self.git, &self.root, &["status", "--porcelain=v1"]);
        let cfg = raw(
            &self.git,
            &self.root,
            &[
                "config",
                "--local",
                "--get-regexp",
                r"^(remote\.|branch\.main\.)",
            ],
        );
        serde_json::json!({"case":case,"success":success,"commands":commands,
            "head":command(&self.git,&self.root,&["rev-parse","HEAD"]),
            "index":command(&self.git,&self.root,&["ls-files","--stage"]),
            "worktree":fs::read_to_string(self.root.join("file")).unwrap(),"status":status,
            "config":String::from_utf8(cfg.stdout).unwrap().trim_end().replace('\r',""),
            "refs":command(&self.git,&self.root,&["for-each-ref","--format=%(refname) %(objectname)"])})
    }
}
#[test]
fn validation_matches_native_location_contract() {
    for value in [
        "../relative.git",
        "some folder",
        "git@host:path",
        "https://host/repo",
        "https://user@host/repo",
        "no-contact::repository",
        r"C:\local repo",
    ] {
        assert!(valid("origin", value), "{value}");
    }
    for value in [
        "",
        "https://user:secret@host/repo",
        "https://host/repo?token=secret",
        "line\nbreak",
    ] {
        assert!(!valid("origin", value), "{value}");
    }
    for name in ["", "-option", "two words", "line\nbreak"] {
        assert!(!valid(name, "../relative"));
    }
    assert!(!valid(&"😀".repeat(128), "repo"));
    assert!(valid(&"x".repeat(255), "repo"));
}
#[test]
fn effective_config_does_not_treat_included_remotes_as_locally_editable() {
    let f = Fixture::new();
    assert!(f.add("Primary.Team", "../direct.git").succeeded());
    assert!(f.add("other", "../other.git").succeeded());
    let included = f.temp.path().join("included.config");
    fs::write(
        &included,
        "[remote \"Primary.Team\"]\n pushurl = ../included-push.git\n[remote \"included-only\"]\n url = ../shared.git\n",
    )
    .unwrap();
    command(
        &f.git,
        &f.root,
        &["config", "--local", "include.path", included.to_str().unwrap()],
    );
    let config = f.config();
    assert_eq!(config.remotes.len(), 2);
    let direct = &config.remotes[0];
    assert_eq!(direct.name, "Primary.Team");
    assert_eq!(direct.location, "../direct.git");
    assert!(direct.push_location.is_empty() && !direct.can_edit);
    assert_eq!(config.remotes[1].name, "other");
    assert!(config.remotes[1].can_edit);
    assert_eq!(
        config.stamp,
        f.git.config_stamp(&f.root, &Cancellation::default()).unwrap()
    );
}

#[test]
fn native_remote_outcomes_preserve_index_worktree_and_local_tips() {
    let f = Fixture::new();
    let before = f.preserved();
    let mut records = vec![];
    let result = f.add("origin", "no-contact::initial");
    assert!(result.succeeded());
    assert_eq!(before, f.preserved());
    records.push(f.record("add", result.succeeded(), result.commands.len()));
    let result = f
        .git
        .execute_remote(
            &f.snapshot(),
            &f.change("../fetch two", "../push two", false),
            None,
        )
        .unwrap();
    assert!(result.succeeded());
    assert_eq!(before, f.preserved());
    records.push(f.record("edit", true, result.commands.len()));
    assert_eq!(
        command(&f.git, &f.root, &["remote", "get-url", "--push", "origin"]),
        "../push two"
    );
    let result = f
        .git
        .execute_remote(&f.snapshot(), &f.change("../fetch two", "", false), None)
        .unwrap();
    assert!(result.succeeded());
    assert_eq!(before, f.preserved());
    records.push(f.record("fallback", true, result.commands.len()));
    assert_eq!(
        command(&f.git, &f.root, &["remote", "get-url", "--push", "origin"]),
        "../fetch two"
    );
    let result = f
        .git
        .execute_remote(&f.snapshot(), &f.change("../fetch two", "", false), None)
        .unwrap();
    assert!(result.succeeded());
    assert!(result.commands.is_empty());
    records.push(f.record("unchanged", true, 0));
    let stale = f.change("../should-not-write", "", false);
    command(
        &f.git,
        &f.root,
        &["config", "remote.origin.url", "../external"],
    );
    assert_eq!(
        f.git
            .execute_remote(&f.snapshot(), &stale, None)
            .err()
            .unwrap()
            .code(),
        "RemoteManageChanged"
    );
    records.push(f.record("stale", false, 0));
    let head = f.snapshot().head.unwrap();
    command(
        &f.git,
        &f.root,
        &["update-ref", "refs/remotes/origin/main", &head],
    );
    command(&f.git, &f.root, &["config", "branch.main.remote", "origin"]);
    command(
        &f.git,
        &f.root,
        &["config", "branch.main.merge", "refs/heads/main"],
    );
    let result = f
        .git
        .execute_remote(&f.snapshot(), &f.change("", "", true), None)
        .unwrap();
    assert!(result.succeeded());
    assert_eq!(before, f.preserved());
    records.push(f.record("remove", true, result.commands.len()));
    if let Some(path) = std::env::var_os("SHOVEL_REMOTE_PARITY_OUTPUT") {
        fs::write(path, serde_json::to_vec_pretty(&records).unwrap()).unwrap();
    }
}
#[test]
fn unsupported_configuration_is_visible_redacted_and_not_flattened() {
    let f = Fixture::new();
    assert!(f.add("origin", "../one").succeeded());
    command(
        &f.git,
        &f.root,
        &["config", "--add", "remote.origin.url", "../two"],
    );
    command(
        &f.git,
        &f.root,
        &[
            "config",
            "remote.secret.url",
            "https://user:secret@host/repo?token=private",
        ],
    );
    command(
        &f.git,
        &f.root,
        &["config", "remote.inherited.url", "../local"],
    );
    command(
        &f.git,
        &f.root,
        &["config", "--global", "remote.inherited.url", "../global"],
    );
    command(
        &f.git,
        &f.root,
        &[
            "config",
            "--global",
            "remote.global-only.url",
            "../global-only",
        ],
    );
    let config = f.config();
    assert_eq!(config.remotes.len(), 3);
    assert!(config.remotes.iter().all(|r| !r.can_edit));
    assert!(!format!("{config:?}").contains("secret@"));
    assert!(!format!("{config:?}").contains("private"));
    let before = fs::read(f.root.join(".git/config")).unwrap();
    assert_eq!(
        f.git
            .execute_remote(&f.snapshot(), &f.change("../flatten", "", false), None)
            .err()
            .unwrap()
            .code(),
        "RemoteManageUnsupported"
    );
    assert_eq!(before, fs::read(f.root.join(".git/config")).unwrap());
    assert!(
        f.git
            .execute_remote(&f.snapshot(), &f.change("", "", true), None)
            .unwrap()
            .succeeded()
    );
    let cancelled = Cancellation::default();
    cancelled.cancel();
    assert!(
        f.git
            .remote_configuration(&f.snapshot(), &cancelled)
            .is_err()
    );
    for i in 0..33 {
        command(
            &f.git,
            &f.root,
            &["config", &format!("remote.r{i}.url"), "../local"],
        );
    }
    assert_eq!(
        f.git
            .remote_configuration(&f.snapshot(), &Cancellation::default())
            .unwrap_err()
            .code(),
        "ActionUnsupportedRemote"
    );
}
#[test]
fn native_lock_duplicate_and_partial_edit_keep_actual_outcomes() {
    let f = Fixture::new();
    assert!(f.add("origin", "../initial").succeeded());
    assert!(!f.add("origin", "../duplicate").succeeded());
    let snapshot = f.snapshot();
    let action = f.change("../changed-fetch", "../changed-push", false);
    let before = f.preserved();
    let lock = f.root.join(".git/config.lock");
    fs::write(&lock, "external owner").unwrap();
    let result = f.git.execute_remote(&snapshot, &action, None).unwrap();
    assert!(!result.succeeded());
    assert_eq!(result.commands.len(), 1);
    assert_eq!(f.config().remotes[0].location, "../initial");
    fs::remove_file(&lock).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let count2 = count.clone();
    let lock2 = lock.clone();
    let progress = Arc::new(move |stage| {
        if stage == Stage::UpdatingRemote && count2.fetch_add(1, Ordering::SeqCst) == 1 {
            fs::write(&lock2, "external owner").unwrap();
        }
    });
    let result = f
        .git
        .execute_remote(&snapshot, &action, Some(progress))
        .unwrap();
    assert!(!result.succeeded());
    assert_eq!(result.commands.len(), 2);
    assert!(result.commands[0].succeeded());
    assert!(!result.commands[1].succeeded());
    assert!(!result.output.is_empty());
    assert!(result.updated.is_ok());
    fs::remove_file(&lock).unwrap();
    let current = f.config();
    assert_eq!(current.remotes[0].location, "../changed-fetch");
    assert!(current.remotes[0].push_location.is_empty());
    assert_eq!(before, f.preserved());
}
#[test]
fn remote_edits_allow_head_movement_and_sequences_but_reject_replaced_identity() {
    let f = Fixture::new();
    assert!(f.add("origin", "../one").succeeded());
    let snapshot = f.snapshot();
    let action = f.change("../two", "", false);
    command(&f.git, &f.root, &["commit", "-m", "advanced"]);
    fs::write(
        f.root.join(".git/MERGE_HEAD"),
        format!("{}\n", snapshot.head.as_ref().unwrap()),
    )
    .unwrap();
    let result = f.git.execute_remote(&snapshot, &action, None).unwrap();
    assert!(result.succeeded());
    assert!(!result.updated.unwrap().operations.is_empty());
    fs::remove_file(f.root.join(".git/MERGE_HEAD")).unwrap();
    fs::rename(f.root.join(".git"), f.temp.path().join("old.git")).unwrap();
    command(&f.git, &f.root, &["init", "--initial-branch=main"]);
    assert_eq!(
        f.git
            .execute_remote(&snapshot, &action, None)
            .err()
            .unwrap()
            .code(),
        "ActionIdentityChanged"
    );
}
