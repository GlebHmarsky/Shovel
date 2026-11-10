#![cfg(windows)]
use shovel_core::{
    branches::Context,
    git::Git,
    operation::Error,
    process::{self, Cancellation, Lifetime},
    sidebar::Submodule,
    submodules::{Action, Operation},
    working::MutationGate,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};
fn command(git: &Git, path: &Path, args: &[&str]) -> String {
    let result = process::run(
        &git.request(
            path,
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
    temp: tempfile::TempDir,
    root: PathBuf,
    source: PathBuf,
    git: Git,
    first: String,
    second: String,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("parent");
        let source = temp.path().join("child-source");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&source).unwrap();
        let config = temp.path().join("gitconfig");
        fs::write(&config,"[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n[protocol \"file\"]\nallow=always\n").unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        for name in ["GIT_AUTHOR_DATE", "GIT_COMMITTER_DATE"] {
            git.environment
                .insert(name.into(), Some("2026-01-01T00:00:00+00:00".into()));
        }
        command(&git, &source, &["init", "--initial-branch=main"]);
        fs::write(source.join("file"), "one\n").unwrap();
        command(&git, &source, &["add", "file"]);
        command(&git, &source, &["commit", "-m", "one"]);
        let first = command(&git, &source, &["rev-parse", "HEAD"]);
        command(&git, &root, &["init", "--initial-branch=main"]);
        fs::write(root.join("parent-file"), "parent\n").unwrap();
        command(&git, &root, &["add", "parent-file"]);
        command(&git, &root, &["commit", "-m", "parent"]);
        command(
            &git,
            &root,
            &["submodule", "add", "--", "../child-source", "modules/child"],
        );
        command(&git, &root, &["commit", "-am", "add child"]);
        fs::write(source.join("file"), "two\n").unwrap();
        command(&git, &source, &["commit", "-am", "two"]);
        let second = command(&git, &source, &["rev-parse", "HEAD"]);
        Self {
            temp,
            root,
            source,
            git,
            first,
            second,
        }
    }
    fn context(&self) -> Context {
        let snapshot = self
            .git
            .inspect(&self.root, &Cancellation::default())
            .unwrap();
        self.git
            .prepare_local(&snapshot, true, &Cancellation::default())
            .unwrap()
    }
    fn target(&self) -> Submodule {
        let snapshot = self
            .git
            .inspect(&self.root, &Cancellation::default())
            .unwrap();
        self.git
            .sidebar(&snapshot, &Cancellation::default())
            .unwrap()
            .submodules[0]
            .clone()
    }
    fn stage(&self, hash: &str) {
        command(
            &self.git,
            &self.root,
            &[
                "update-index",
                "--cacheinfo",
                &format!("160000,{hash},modules/child"),
            ],
        );
    }
    fn execute(&self, operation: Operation) -> Result<shovel_core::operation::Completed, Error> {
        self.git.execute_submodule(
            &self.context(),
            &Action {
                target: self.target(),
                operation,
            },
            None,
            None,
        )
    }
    fn record(&self, name: &str, success: bool) -> serde_json::Value {
        let child = self.root.join("modules/child");
        let normalize = |s: String| {
            s.replace('\\', "/").replace(
                &self.temp.path().to_string_lossy().replace('\\', "/"),
                "<fixture>",
            )
        };
        let cfg = command(
            &self.git,
            &self.root,
            &["config", "--local", "--get-regexp", r"^submodule\."],
        );
        serde_json::json!({"case":name,"success":success,
            "head":command(&self.git,&self.root,&["rev-parse","HEAD"]),
            "index":command(&self.git,&self.root,&["ls-files","--stage"]),
            "modules":fs::read_to_string(self.root.join(".gitmodules")).unwrap(),
            "config":normalize(cfg),
            "childHead":command(&self.git,&child,&["rev-parse","HEAD"]),
            "childIndex":command(&self.git,&child,&["ls-files","--stage"]),
            "childStatus":command(&self.git,&child,&["status","--porcelain=v1"]),
            "parentStatus":command(&self.git,&self.root,&["status","--porcelain=v1"]),
            "childFile":fs::read_to_string(child.join("file")).unwrap(),
            "childOrigin":normalize(command(&self.git,&child,&["remote","get-url","origin"])),
            "parentFile":fs::read_to_string(self.root.join("parent-file")).unwrap()})
    }
}
#[test]
fn selected_submodule_updates_indexed_commit_syncs_url_and_opens_child() {
    let f = Fixture::new();
    let child = f.root.join("modules/child");
    let mut records = vec![];
    command(
        &f.git,
        &f.root,
        &["submodule", "deinit", "--force", "--", "modules/child"],
    );
    let cached = f.context().repository;
    assert_eq!(
        f.git
            .open_submodule(&cached, &f.target())
            .err()
            .unwrap()
            .code(),
        "SidebarSubmoduleUnavailable"
    );
    let head = command(&f.git, &f.root, &["rev-parse", "HEAD"]);
    let index = command(&f.git, &f.root, &["ls-files", "--stage"]);
    let result = f.execute(Operation::Update).unwrap();
    assert!(result.succeeded());
    assert_eq!(result.related.as_slice(), std::slice::from_ref(&child));
    assert_eq!(command(&f.git, &child, &["rev-parse", "HEAD"]), f.first);
    assert_eq!(command(&f.git, &f.root, &["rev-parse", "HEAD"]), head);
    assert_eq!(command(&f.git, &f.root, &["ls-files", "--stage"]), index);
    assert_eq!(f.git.open_submodule(&cached, &f.target()).unwrap(), child);
    records.push(f.record("initialize", true));
    f.stage(&f.second);
    fs::write(f.root.join("parent-file"), "parent dirty\n").unwrap();
    let index = command(&f.git, &f.root, &["ls-files", "--stage"]);
    let result = f.execute(Operation::Update).unwrap();
    assert!(result.succeeded());
    assert_eq!(command(&f.git, &child, &["rev-parse", "HEAD"]), f.second);
    assert_eq!(command(&f.git, &f.root, &["ls-files", "--stage"]), index);
    assert_eq!(command(&f.git, &f.root, &["rev-parse", "HEAD"]), head);
    records.push(f.record("indexed-update", true));
    f.stage(&f.first);
    fs::write(child.join("file"), "local child\n").unwrap();
    let failed = f.execute(Operation::Update).unwrap();
    assert!(!failed.succeeded());
    records.push(f.record("dirty-child", false));
    let stale = f.context();
    let old = f.target();
    f.stage(&f.second);
    assert_eq!(
        f.git
            .execute_submodule(
                &stale,
                &Action {
                    target: old,
                    operation: Operation::Update
                },
                None,
                None
            )
            .err()
            .unwrap()
            .code(),
        "ActionChanged"
    );
    records.push(f.record("stale-index", false));
    fs::write(child.join("file"), "two\n").unwrap();
    command(
        &f.git,
        &f.root,
        &[
            "config",
            "--file",
            ".gitmodules",
            "submodule.modules/child.url",
            "../child-source-new",
        ],
    );
    let result = f.execute(Operation::Sync).unwrap();
    assert!(result.succeeded());
    assert!(
        command(&f.git, &child, &["remote", "get-url", "origin"])
            .replace('\\', "/")
            .ends_with("/child-source-new")
    );
    records.push(f.record("sync", true));
    if let Some(output) = std::env::var_os("SHOVEL_SUBMODULE_PARITY_OUTPUT") {
        fs::write(output, serde_json::to_vec_pretty(&records).unwrap()).unwrap();
    }
}
#[test]
fn dirty_child_failure_and_stale_index_keep_working_and_staged_changes() {
    let f = Fixture::new();
    let child = f.root.join("modules/child");
    let context = f.context();
    let old = f.target();
    f.stage(&f.second);
    let err = f
        .git
        .execute_submodule(
            &context,
            &Action {
                target: old,
                operation: Operation::Update,
            },
            None,
            None,
        )
        .err()
        .unwrap();
    assert_eq!(err.code(), "ActionChanged");
    fs::write(child.join("file"), "must survive\n").unwrap();
    let before = command(&f.git, &f.root, &["ls-files", "--stage"]);
    let result = f.execute(Operation::Update).unwrap();
    assert!(!result.succeeded());
    assert!(!result.output.is_empty());
    assert!(result.updated.is_ok());
    assert_eq!(result.related.as_slice(), std::slice::from_ref(&child));
    assert_eq!(
        fs::read_to_string(child.join("file")).unwrap(),
        "must survive\n"
    );
    assert_eq!(command(&f.git, &child, &["rev-parse", "HEAD"]), f.first);
    assert_eq!(command(&f.git, &f.root, &["ls-files", "--stage"]), before);
}
#[test]
fn child_and_parent_ownership_and_child_sequence_refuse_overlapping_writes() {
    let f = Fixture::new();
    let child = f.root.join("modules/child");
    let context = f.context();
    let target = f.target();
    for common in [
        context.repository.common_directory.clone(),
        shovel_core::identity::read(&child).unwrap().common,
    ] {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let _gate = MutationGate::acquire(&common, false).unwrap();
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        ready_rx.recv().unwrap();
        let result = f.git.execute_submodule(
            &context,
            &Action {
                target: target.clone(),
                operation: Operation::Sync,
            },
            None,
            None,
        );
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        assert!(result.is_err());
    }
    let child_id = shovel_core::identity::read(&child).unwrap();
    fs::write(child_id.git.join("MERGE_HEAD"), format!("{}\n", f.first)).unwrap();
    assert_eq!(
        f.execute(Operation::Update).err().unwrap().code(),
        "ActionSequence"
    );
}
#[test]
fn update_preserves_nested_uninitialized_submodule_and_native_hooks() {
    let f = Fixture::new();
    let nested = f.temp.path().join("nested-source");
    fs::create_dir(&nested).unwrap();
    command(&f.git, &nested, &["init", "--initial-branch=main"]);
    command(
        &f.git,
        &nested,
        &["commit", "--allow-empty", "-m", "nested"],
    );
    command(
        &f.git,
        &f.source,
        &["submodule", "add", "--", "../nested-source", "nested"],
    );
    command(&f.git, &f.source, &["commit", "-am", "nested pointer"]);
    let target = command(&f.git, &f.source, &["rev-parse", "HEAD"]);
    f.stage(&target);
    let child = f.root.join("modules/child");
    let child_id = shovel_core::identity::read(&child).unwrap();
    fs::write(
        child_id.git.join("hooks/post-checkout"),
        "#!/bin/sh\nprintf 'hook ran\\n' > hook-result\n",
    )
    .unwrap();
    let result = f.execute(Operation::Update).unwrap();
    assert!(result.succeeded());
    assert_eq!(
        fs::read_to_string(child.join("hook-result")).unwrap(),
        "hook ran\n"
    );
    assert!(
        !child.join("nested/.git").exists(),
        "selected update must not recurse"
    );
    assert_eq!(command(&f.git, &child, &["rev-parse", "HEAD"]), target);
}
#[test]
fn submodule_paths_reject_escape_aliases_and_link_components() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    for value in [
        "",
        "../outside",
        "a/../b",
        "a//b",
        "a/./b",
        ".git/x",
        "a/.Git/x",
        "/rooted",
        "C:/rooted",
        "alias. /child",
    ] {
        assert!(
            shovel_core::submodules::path(&root, value).is_err(),
            "{value}"
        );
    }
    assert_eq!(
        shovel_core::submodules::path(&root, "modules/space name").unwrap(),
        root.join("modules/space name")
    );
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let junction = root.join("junction");
    let status = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&junction)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "disposable junction fixture failed"
    );
    assert_eq!(
        shovel_core::submodules::path(&root, "junction/child")
            .err()
            .unwrap()
            .code(),
        "SidebarSubmodulePath"
    );
    fs::remove_dir(&junction).unwrap();
    assert!(outside.is_dir());
}

#[test]
fn selected_path_is_literal_and_never_updates_a_matching_sibling() {
    let f = Fixture::new();
    for path in ["modules/[c]", "modules/c"] {
        command(
            &f.git,
            &f.root,
            &["submodule", "add", "--", "../child-source", path],
        );
        command(
            &f.git,
            &f.root,
            &[
                "update-index",
                "--cacheinfo",
                &format!("160000,{},{}", f.first, path),
            ],
        );
    }
    let context = f.context();
    let target = f
        .git
        .sidebar(&context.repository, &Cancellation::default())
        .unwrap()
        .submodules
        .into_iter()
        .find(|s| s.path == "modules/[c]")
        .unwrap();
    assert!(
        f.git
            .execute_submodule(
                &context,
                &Action {
                    target,
                    operation: Operation::Update
                },
                None,
                None
            )
            .unwrap()
            .succeeded()
    );
    assert_eq!(
        command(&f.git, &f.root.join("modules/[c]"), &["rev-parse", "HEAD"]),
        f.first
    );
    assert_eq!(
        command(&f.git, &f.root.join("modules/c"), &["rev-parse", "HEAD"]),
        f.second
    );
}

#[test]
fn first_initialization_owns_child_before_checkout_hook_finishes() {
    first_initialization_ownership(false);
}
#[test]
fn linked_parent_initialization_owns_child_before_checkout_hook_finishes() {
    first_initialization_ownership(true);
}
fn first_initialization_ownership(linked: bool) {
    use std::time::{Duration, Instant};
    let f = Fixture::new();
    let fresh = f.temp.path().join("fresh-parent");
    if linked {
        command(
            &f.git,
            &f.root,
            &[
                "worktree",
                "add",
                "--detach",
                &fresh.to_string_lossy(),
                "HEAD",
            ],
        );
    } else {
        command(
            &f.git,
            f.temp.path(),
            &[
                "clone",
                "--no-recurse-submodules",
                &f.root.to_string_lossy(),
                &fresh.to_string_lossy(),
            ],
        );
    }
    let hooks = f.temp.path().join("hooks");
    fs::create_dir(&hooks).unwrap();
    fs::write(hooks.join("post-checkout"),
        "#!/bin/sh\ntouch hook-started\nwhile test ! -f hook-release; do sleep .05; done\ntouch hook-finished\n").unwrap();
    command(
        &f.git,
        &f.root,
        &[
            "config",
            "--file",
            &f.temp.path().join("gitconfig").to_string_lossy(),
            "core.hooksPath",
            &hooks.to_string_lossy().replace('\\', "/"),
        ],
    );
    let snapshot = f.git.inspect(&fresh, &Cancellation::default()).unwrap();
    let context = f
        .git
        .prepare_local(&snapshot, true, &Cancellation::default())
        .unwrap();
    let target = f
        .git
        .sidebar(&snapshot, &Cancellation::default())
        .unwrap()
        .submodules[0]
        .clone();
    let child = fresh.join(&target.path);
    assert!(!child.join(".git").exists());
    assert!(!fresh.join(".git/modules").exists());
    struct Release(PathBuf);
    impl Drop for Release {
        fn drop(&mut self) {
            let _ = fs::write(&self.0, "");
        }
    }
    let release = Release(child.join("hook-release"));
    let git = f.git.clone();
    let worker = thread::spawn(move || {
        git.execute_submodule(
            &context,
            &Action {
                target,
                operation: Operation::Update,
            },
            None,
            None,
        )
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    while !child.join("hook-started").exists() {
        assert!(
            Instant::now() < deadline,
            "native post-checkout did not start"
        );
        thread::sleep(Duration::from_millis(10));
    }
    // The marker and HEAD already exist, so the child can be opened while
    // checkout still owns it. A real child branch action must refuse to write.
    let observed = f.git.inspect(&child, &Cancellation::default()).unwrap();
    let child_context = f
        .git
        .prepare_local(&observed, true, &Cancellation::default())
        .unwrap();
    let start = observed
        .references
        .iter()
        .find(|r| r.name == "refs/heads/main")
        .unwrap()
        .clone();
    let attempted = f.git.execute_branch(
        &child_context,
        &shovel_core::branches::Action::Create {
            name: "must-wait".into(),
            start,
        },
        None,
    );
    drop(release);
    let result = worker.join().unwrap().unwrap();
    assert!(result.succeeded());
    assert_eq!(attempted.err().unwrap().code(), "ActionOtherInstance");
    assert!(child.join("hook-finished").exists());
    assert!(
        command(
            &f.git,
            &child,
            &[
                "for-each-ref",
                "--format=%(refname)",
                "refs/heads/must-wait"
            ]
        )
        .is_empty()
    );
    let observed = f.git.inspect(&child, &Cancellation::default()).unwrap();
    let context = f
        .git
        .prepare_local(&observed, true, &Cancellation::default())
        .unwrap();
    let start = observed
        .references
        .iter()
        .find(|r| r.name == "refs/heads/main")
        .unwrap()
        .clone();
    assert!(
        f.git
            .execute_branch(
                &context,
                &shovel_core::branches::Action::Create {
                    name: "after-update".into(),
                    start
                },
                None
            )
            .unwrap()
            .succeeded()
    );
}
