#![cfg(windows)]
use shovel_core::{
    cloning::Options,
    git::Git,
    process::{self, Cancellation, Lifetime},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
struct Fixture {
    temp: tempfile::TempDir,
    git: Git,
    source: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("isolated.config");
        fs::write(&config, "[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n[init]\ndefaultBranch=main\n").unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        for key in ["GIT_AUTHOR_DATE", "GIT_COMMITTER_DATE"] {
            git.environment
                .insert(key.into(), Some("2026-01-01T00:00:00+00:00".into()));
        }
        let source = std::env::var_os("SHOVEL_CLONE_PARITY_SOURCE").map(PathBuf::from);
        let f = Self {
            source: source
                .clone()
                .unwrap_or_else(|| temp.path().join("-source 日本語")),
            temp,
            git,
        };
        if source.is_none() {
            fs::create_dir(&f.source).unwrap();
            f.command(&f.source, &["init", "--initial-branch=main"]);
            fs::write(f.source.join("file.txt"), b"base\n").unwrap();
            fs::write(f.source.join("asset.dat"), [0, 255, 1, 2]).unwrap();
            fs::write(f.source.join(".gitattributes"), b"*.dat filter=fixture\n").unwrap();
            f.command(&f.source, &["add", "."]);
            f.command(&f.source, &["commit", "-m", "base"]);
            f.command(&f.source, &["tag", "v1"]);
            f.command(&f.source, &["checkout", "-b", "feature/topic"]);
            fs::write(f.source.join("file.txt"), b"feature\n").unwrap();
            f.command(&f.source, &["commit", "-am", "feature"]);
            f.command(&f.source, &["checkout", "main"]);
            fs::write(f.source.join("file.txt"), b"main\n").unwrap();
            f.command(&f.source, &["commit", "-am", "main"]);
        }
        f
    }
    fn command(&self, root: &Path, args: &[&str]) -> String {
        let result = process::run(
            &self
                .git
                .request(root, args.iter().copied(), Lifetime::AcceptedCommand, 65536),
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
    fn configure(&self, key: &str, value: &str) {
        self.command(
            self.temp.path(),
            &[
                "config",
                "--file",
                self.temp.path().join("isolated.config").to_str().unwrap(),
                key,
                value,
            ],
        );
    }
    fn options(&self, name: &str, branch: &str) -> Options {
        Options {
            source: self.source.to_string_lossy().into_owned(),
            directory: self.temp.path().join(name),
            branch: branch.into(),
        }
    }
    fn export(&self, root: &Path, case: &str, message: &str) {
        let Some(output) = std::env::var_os("SHOVEL_CLONE_PARITY_OUTPUT") else {
            return;
        };
        let hex = |b: &[u8]| b.iter().map(|b| format!("{b:02X}")).collect::<String>();
        let files = [".gitattributes", "asset.dat", "file.txt", "hook-ran"]
            .into_iter()
            .filter(|f| root.join(f).exists())
            .map(|f| (f, hex(&fs::read(root.join(f)).unwrap())))
            .collect::<std::collections::BTreeMap<_, _>>();
        let state = serde_json::json!({
            "message": message,
            "headFile": hex(&fs::read(root.join(".git/HEAD")).unwrap()),
            "head": self.command(root, &["rev-parse", "HEAD"]),
            "index": self.command(root, &["ls-files", "--stage", "-z"]),
            "status": self.command(root, &["status", "--porcelain", "-z"]),
            "config": hex(&fs::read(root.join(".git/config")).unwrap()),
            "worktree": files,
        });
        let output = PathBuf::from(output);
        fs::create_dir_all(&output).unwrap();
        fs::write(
            output.join(format!("{case}.json")),
            serde_json::to_vec_pretty(&state).unwrap(),
        )
        .unwrap();
    }
}
#[test]
fn native_clone_preserves_default_branch_explicit_branch_and_detached_tag() {
    let f = Fixture::new();
    for (case, branch, wanted, bytes) in [
        ("default", "", Some("main"), b"main\n".as_slice()),
        (
            "branch",
            "feature/topic",
            Some("feature/topic"),
            b"feature\n".as_slice(),
        ),
        ("tag", "v1", None, b"base\n".as_slice()),
    ] {
        let options = f.options(case, branch);
        let output = Arc::new(Mutex::new(Vec::new()));
        let captured = output.clone();
        let result = f
            .git
            .clone_repository(
                &options,
                None,
                Some(Arc::new(move |_, bytes| {
                    captured.lock().unwrap().extend_from_slice(bytes)
                })),
            )
            .unwrap();
        assert!(result.succeeded, "{}", result.output);
        assert_eq!(result.message, "CloneSucceeded");
        let repo = result.repository.unwrap();
        assert_eq!(repo.worktree, options.directory);
        assert_eq!(repo.branch.as_deref(), wanted);
        assert_eq!(fs::read(options.directory.join("file.txt")).unwrap(), bytes);
        assert_eq!(
            fs::read(options.directory.join("asset.dat")).unwrap(),
            [0, 255, 1, 2]
        );
        assert!(
            !output.lock().unwrap().is_empty(),
            "native progress is delivered"
        );
        assert_eq!(
            f.command(&options.directory, &["remote", "get-url", "origin"])
                .replace('/', "\\"),
            f.source.to_string_lossy().replace('/', "\\")
        );
        assert!(
            f.command(&options.directory, &["status", "--porcelain"])
                .is_empty()
        );
        f.export(&options.directory, case, result.message);
    }
}
#[test]
fn failed_native_checkout_hook_retains_repository_worktree_and_output() {
    let f = Fixture::new();
    let hooks = f.temp.path().join("hooks");
    fs::create_dir(&hooks).unwrap();
    fs::write(
        hooks.join("post-checkout"),
        b"#!/bin/sh\nprintf 'ran\\n' > hook-ran\nexit 1\n",
    )
    .unwrap();
    f.configure("core.hooksPath", hooks.to_str().unwrap());
    let options = f.options("hook-failure", "");
    let result = f.git.clone_repository(&options, None, None).unwrap();
    assert!(!result.succeeded);
    assert_eq!(result.message, "CloneIncomplete");
    assert!(result.repository.is_some());
    assert!(!result.output.is_empty());
    assert_eq!(
        fs::read(options.directory.join("hook-ran")).unwrap(),
        b"ran\n"
    );
    assert_eq!(
        fs::read(options.directory.join("file.txt")).unwrap(),
        b"main\n"
    );
    f.export(&options.directory, "hook-failure", result.message);
}
#[test]
fn native_required_filter_failure_preserves_clone_without_retry_or_cleanup() {
    let f = Fixture::new();
    f.configure("filter.fixture.smudge", "exit 7");
    f.configure("filter.fixture.required", "true");
    let options = f.options("filter-failure", "");
    let result = f.git.clone_repository(&options, None, None).unwrap();
    assert!(!result.succeeded);
    assert!(result.repository.is_some());
    assert_eq!(result.message, "CloneIncomplete");
    assert!(options.directory.join(".git/HEAD").is_file());
    assert_eq!(
        f.command(&options.directory, &["rev-parse", "HEAD"]),
        f.command(&f.source, &["rev-parse", "HEAD"])
    );
    assert!(result.output.contains("smudge"));
}
#[test]
fn existing_contents_and_invalid_sources_are_not_changed_and_empty_target_works() {
    let f = Fixture::new();
    let options = f.options("populated", "");
    fs::create_dir(&options.directory).unwrap();
    fs::write(options.directory.join("keep"), b"untouched").unwrap();
    assert_eq!(
        f.git
            .clone_repository(&options, None, None)
            .err()
            .unwrap()
            .code(),
        "CloneNotEmpty"
    );
    assert_eq!(
        fs::read(options.directory.join("keep")).unwrap(),
        b"untouched"
    );
    assert!(!options.directory.join(".git").exists());
    let mut invalid = f.options("invalid", "");
    invalid.source = "--upload-pack=bad".into();
    assert_eq!(
        f.git
            .clone_repository(&invalid, None, None)
            .err()
            .unwrap()
            .code(),
        "CloneInvalid"
    );
    assert!(!invalid.directory.exists());
    let options = f.options("empty", "");
    fs::create_dir(&options.directory).unwrap();
    assert!(
        f.git
            .clone_repository(&options, None, None)
            .unwrap()
            .succeeded
    );
    let missing = f.options("missing-branch", "no-such-branch");
    let failed = f.git.clone_repository(&missing, None, None).unwrap();
    assert!(!failed.succeeded);
    assert!(!failed.output.is_empty());
    assert!(failed.repository.is_none());
}

#[test]
fn clone_output_limit_keeps_the_accepted_hook_running_and_hides_partial_credentials() {
    let f = Fixture::new();
    let hooks = f.temp.path().join("hooks");
    fs::create_dir(&hooks).unwrap();
    fs::write(hooks.join("post-checkout"),
        b"#!/bin/sh\nprintf '%1048566s' ''\nprintf 'https://fixture-user:never-log-this@example.invalid/repo'\nprintf 'finished\\n' > hook-finished\n").unwrap();
    f.configure("core.hooksPath", hooks.to_str().unwrap());
    let options = f.options("output-limit", "");
    let result = f.git.clone_repository(&options, None, None).unwrap();
    assert!(!result.succeeded);
    assert_eq!(result.message, "CloneOutputLimit");
    assert!(result.repository.is_some());
    assert_eq!(
        fs::read(options.directory.join("hook-finished")).unwrap(),
        b"finished\n"
    );
    assert!(!result.output.contains("fixture-user"));
    assert!(!result.output.contains("never-log-this"));
    assert_eq!(
        fs::read(options.directory.join("file.txt")).unwrap(),
        b"main\n"
    );
}
