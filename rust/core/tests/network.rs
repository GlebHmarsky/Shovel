#![cfg(windows)]
use shovel_core::{
    git::Git,
    network::{Action, Error, PushTarget, Stage, Strategy, Target},
    process::{self, Cancellation, Lifetime},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
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
    String::from_utf8(result.stdout)
        .unwrap()
        .trim_end()
        .to_owned()
}
struct Fixture {
    _temp: tempfile::TempDir,
    git: Git,
    root: PathBuf,
    remote: PathBuf,
    other: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("local");
        let remote = temp.path().join("remote.git");
        let other = temp.path().join("other");
        fs::create_dir(&root).unwrap();
        let config = temp.path().join("isolated.gitconfig");
        fs::write(&config, "[user]\nname = Fixture\nemail = fixture@example.invalid\n[commit]\ngpgsign = false\n[init]\ndefaultBranch = main\n[core]\nautocrlf = false\n").unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        command(
            &git,
            temp.path(),
            &["init", "--bare", remote.to_str().unwrap()],
        );
        command(&git, &root, &["init"]);
        fs::write(root.join("a"), "base\n").unwrap();
        command(&git, &root, &["add", "a"]);
        command(&git, &root, &["commit", "-m", "base"]);
        command(
            &git,
            &root,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        command(&git, &root, &["push", "-u", "origin", "main"]);
        command(
            &git,
            temp.path(),
            &["clone", remote.to_str().unwrap(), other.to_str().unwrap()],
        );
        Self {
            _temp: temp,
            git,
            root,
            remote,
            other,
        }
    }
    fn incoming(&self, name: &str, contents: &str) -> String {
        fs::write(self.other.join(name), contents).unwrap();
        command(&self.git, &self.other, &["add", "--", name]);
        command(&self.git, &self.other, &["commit", "-m", "incoming"]);
        command(&self.git, &self.other, &["push"]);
        command(&self.git, &self.other, &["rev-parse", "HEAD"])
    }
    fn run(&self, action: Action) -> shovel_core::network::Completed {
        let review = self
            .git
            .prepare_network(&self.root, &Cancellation::default())
            .unwrap();
        self.git
            .execute_network(&review, &action, None, None)
            .unwrap()
    }
}

#[test]
fn configured_fetch_pull_and_push_preserve_native_hooks_and_targets() {
    let f = Fixture::new();
    let cancel = Cancellation::default();
    let tip = f.incoming("incoming", "remote\n");
    let before = command(&f.git, &f.root, &["rev-parse", "HEAD"]);
    let fetched = f.run(Action::Fetch {
        remote: None,
        all: false,
    });
    assert!(
        fetched.succeeded(),
        "{} {:?}",
        fetched.output,
        fetched.message
    );
    assert_eq!(command(&f.git, &f.root, &["rev-parse", "HEAD"]), before);
    assert_eq!(command(&f.git, &f.root, &["rev-parse", "origin/main"]), tip);
    let pulled = f.run(Action::Pull {
        target: None,
        strategy: Strategy::Rebase,
        auto_stash: false,
    });
    assert!(pulled.succeeded(), "{} {:?}", pulled.output, pulled.message);
    assert_eq!(command(&f.git, &f.root, &["rev-parse", "HEAD"]), tip);
    fs::write(f.root.join("local"), "local\n").unwrap();
    command(&f.git, &f.root, &["add", "local"]);
    command(&f.git, &f.root, &["commit", "-m", "local"]);
    let local = command(&f.git, &f.root, &["rev-parse", "HEAD"]);
    fs::write(
        f.root.join(".git/hooks/pre-push"),
        "#!/bin/sh\nprintf 'hook-result\\n'\nexit 1\n",
    )
    .unwrap();
    let failed = f.run(Action::Push {
        target: None,
        all_tags: false,
        set_upstream: false,
    });
    assert!(!failed.succeeded());
    assert!(failed.output.contains("hook-result"));
    assert_eq!(command(&f.git, &f.remote, &["rev-parse", "main"]), tip);
    fs::remove_file(f.root.join(".git/hooks/pre-push")).unwrap();
    let pushed = f.run(Action::Push {
        target: None,
        all_tags: false,
        set_upstream: false,
    });
    assert!(pushed.succeeded(), "{} {:?}", pushed.output, pushed.message);
    assert_eq!(command(&f.git, &f.remote, &["rev-parse", "main"]), local);
    let review = f.git.prepare_network(&f.root, &cancel).unwrap();
    command(
        &f.git,
        &f.root,
        &["config", "branch.main.remote", "changed"],
    );
    assert!(matches!(
        f.git.execute_network(
            &review,
            &Action::Fetch {
                remote: Some("origin".into()),
                all: false
            },
            None,
            None
        ),
        Err(Error::Code("ActionChanged"))
    ));
}

#[test]
fn push_defaults_refuse_ambiguous_scope_and_follow_push_remote_priority() {
    let f = Fixture::new();
    let cancel = Cancellation::default();
    command(
        &f.git,
        &f.root,
        &["remote", "add", "publish", f.remote.to_str().unwrap()],
    );
    command(
        &f.git,
        &f.root,
        &["config", "branch.main.pushRemote", "publish"],
    );
    let review = f.git.prepare_network(&f.root, &cancel).unwrap();
    assert_eq!(
        f.git.default_push(&review, &cancel).unwrap(),
        PushTarget {
            source: "main".into(),
            destination: Target {
                remote: "publish".into(),
                branch: "main".into()
            }
        }
    );
    command(
        &f.git,
        &f.root,
        &[
            "config",
            "remote.publish.push",
            "+refs/heads/*:refs/heads/*",
        ],
    );
    let review = f.git.prepare_network(&f.root, &cancel).unwrap();
    assert!(matches!(
        f.git.default_push(&review, &cancel),
        Err(Error::Code("DefaultPushNeedsTarget"))
    ));
    command(
        &f.git,
        &f.root,
        &["config", "remote.publish.mirror", "true"],
    );
    command(&f.git, &f.root, &["branch", "unpublished"]);
    let result = f.run(Action::Push {
        target: Some(PushTarget {
            source: "main".into(),
            destination: Target {
                remote: "publish".into(),
                branch: "explicit".into(),
            },
        }),
        all_tags: false,
        set_upstream: false,
    });
    assert!(result.succeeded(), "{} {:?}", result.output, result.message);
    assert_eq!(
        command(&f.git, &f.remote, &["for-each-ref", "--format=%(refname)"]),
        "refs/heads/explicit\nrefs/heads/main"
    );
}

#[test]
fn pull_preserves_ignored_files_and_detects_changed_head_after_fetch() {
    let f = Fixture::new();
    fs::write(f.root.join(".git/info/exclude"), "generated\n").unwrap();
    fs::write(f.root.join("generated"), "keep ignored\n").unwrap();
    f.incoming("generated", "incoming tracked\n");
    let head = command(&f.git, &f.root, &["rev-parse", "HEAD"]);
    let result = f.run(Action::Pull {
        target: None,
        strategy: Strategy::Rebase,
        auto_stash: false,
    });
    assert!(!result.succeeded());
    assert_eq!(result.message.as_deref(), Some("ActionIgnoredCollision"));
    assert_eq!(result.commands.len(), 1);
    assert!(result.commands[0].succeeded());
    assert_eq!(
        fs::read_to_string(f.root.join("generated")).unwrap(),
        "keep ignored\n"
    );
    assert_eq!(command(&f.git, &f.root, &["rev-parse", "HEAD"]), head);
    let review = f
        .git
        .prepare_network(&f.root, &Cancellation::default())
        .unwrap();
    let git = f.git.clone();
    let root = f.root.clone();
    let progress = Arc::new(move |stage| {
        if stage == Stage::CheckingFetchedTip {
            command(&git, &root, &["switch", "-c", "external-change"]);
        }
    });
    let result = f
        .git
        .execute_network(
            &review,
            &Action::Pull {
                target: None,
                strategy: Strategy::Merge,
                auto_stash: false,
            },
            None,
            Some(progress),
        )
        .unwrap();
    assert!(!result.succeeded());
    assert_eq!(result.message.as_deref(), Some("ActionChangedAfterFetch"));
    assert_eq!(result.commands.len(), 1);
}

#[test]
fn merge_choice_overrides_branch_merge_options_and_autostash_conflicts_are_not_success() {
    let f = Fixture::new();
    f.incoming("a", "incoming\n");
    fs::write(f.root.join("a"), "dirty local\n").unwrap();
    command(
        &f.git,
        &f.root,
        &["config", "branch.main.mergeOptions", "--squash --no-commit"],
    );
    let result = f.run(Action::Pull {
        target: None,
        strategy: Strategy::Merge,
        auto_stash: true,
    });
    assert!(!result.succeeded());
    assert!(result.has_conflicts);
    assert!(result.commands.iter().all(|c| c.succeeded()));
    assert!(!command(&f.git, &f.root, &["stash", "list"]).is_empty());
    assert!(command(&f.git, &f.root, &["ls-files", "--unmerged"]).contains("a"));
    assert_eq!(
        command(&f.git, &f.root, &["rev-parse", "HEAD"]),
        command(&f.git, &f.other, &["rev-parse", "HEAD"])
    );
}

#[test]
fn automatic_fetch_updates_refs_without_touching_worktree_and_honors_cancellation() {
    let f = Fixture::new();
    let incoming = f.incoming("auto.txt", "incoming\n");
    let head = command(&f.git, &f.root, &["rev-parse", "HEAD"]);
    fs::write(f.root.join("a"), "local edit\n").unwrap();
    let review = f
        .git
        .prepare_network(&f.root, &Cancellation::default())
        .unwrap();
    let token = Cancellation::default();
    token.cancel();
    assert!(f.git.automatic_fetch(&review, &token).is_err());
    let result = f
        .git
        .automatic_fetch(&review, &Cancellation::default())
        .unwrap();
    assert!(result.succeeded(), "{}", result.output);
    assert_eq!(
        command(&f.git, &f.root, &["rev-parse", "refs/remotes/origin/main"]),
        incoming
    );
    assert_eq!(command(&f.git, &f.root, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        fs::read_to_string(f.root.join("a")).unwrap(),
        "local edit\n"
    );
    assert!(!f.root.join("auto.txt").exists());
}
