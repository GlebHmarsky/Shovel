#![cfg(windows)]
use shovel_core::{
    git::Git,
    initialization::Options,
    process::{self, Cancellation, Lifetime},
    working::{Action, Group},
};
use std::{fs, path::Path};
fn git(git: &Git, root: &Path, args: &[&str]) -> String {
    let result = process::run(
        &git.request(root, args.iter().copied(), Lifetime::AcceptedCommand, 65536),
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
fn engine(root: &Path) -> Git {
    let config = root.join("isolated.config");
    fs::write(&config, "[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n[init]\ndefaultBranch=team-default\n").unwrap();
    let mut result = Git::discover(None, &Cancellation::default()).unwrap();
    result
        .environment
        .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
    result
        .environment
        .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
    for key in ["GIT_AUTHOR_DATE", "GIT_COMMITTER_DATE"] {
        result
            .environment
            .insert(key.into(), Some("2026-01-01T00:00:00+00:00".into()));
    }
    result
}
fn init(git: &Git, root: &Path, branch: &str) -> shovel_core::initialization::Completed {
    git.initialize_repository(&Options {
        directory: root.into(),
        initial_branch: branch.into(),
    })
    .unwrap()
}
#[test]
fn configured_branch_templates_preserve_files_and_new_repository_accepts_commit() {
    let temp = tempfile::tempdir().unwrap();
    let engine = engine(temp.path());
    let templates = temp.path().join("templates");
    fs::create_dir(&templates).unwrap();
    fs::write(templates.join("fixture-marker"), b"copied template").unwrap();
    git(
        &engine,
        temp.path(),
        &[
            "config",
            "--file",
            temp.path().join("isolated.config").to_str().unwrap(),
            "init.templateDir",
            templates.to_str().unwrap(),
        ],
    );
    let root = temp.path().join("-populated 日本語 folder");
    fs::create_dir(&root).unwrap();
    let bytes = [0u8, 255, 1, 2];
    fs::write(root.join("keep.bin"), bytes).unwrap();
    let result = init(&engine, &root, "");
    assert!(result.succeeded);
    assert_eq!(result.message, "InitSucceeded");
    export(&engine, &root, "configured", result.message, &["keep.bin"]);
    let snapshot = result.repository.unwrap();
    assert_eq!(snapshot.branch.as_deref(), Some("team-default"));
    assert!(snapshot.head.is_none());
    assert_eq!(fs::read(root.join("keep.bin")).unwrap(), bytes);
    assert_eq!(
        fs::read(root.join(".git/fixture-marker")).unwrap(),
        b"copied template"
    );
    assert!(git(&engine, &root, &["ls-files"]).is_empty());
    assert!(git(&engine, &root, &["remote"]).is_empty());
    let context = engine
        .working_copy(&root, &Cancellation::default())
        .unwrap();
    assert_eq!(context.entries.len(), 1);
    assert_eq!(context.entries[0].group, Group::Untracked);
    let staged = engine
        .execute_working(&context, &Action::Stage(context.entries.clone()), None)
        .unwrap();
    let committed = engine
        .execute_working(
            &staged.updated.unwrap(),
            &Action::Commit {
                message: "first".into(),
                amend: false,
            },
            None,
        )
        .unwrap();
    assert!(committed.succeeded());
    assert_eq!(git(&engine, &root, &["log", "-1", "--format=%s"]), "first");
}
#[test]
fn explicit_missing_destination_and_existing_linked_worktree_are_not_reinitialized() {
    let temp = tempfile::tempdir().unwrap();
    let engine = engine(temp.path());
    let root = temp.path().join("-new 日本語/child");
    assert_eq!(
        init(&engine, &root, "work/start")
            .repository
            .unwrap()
            .branch
            .as_deref(),
        Some("work/start")
    );
    git(&engine, &root, &["commit", "--allow-empty", "-m", "base"]);
    let head = git(&engine, &root, &["rev-parse", "HEAD"]);
    let config = fs::read(root.join(".git/config")).unwrap();
    fs::write(root.join("kept"), b"untouched").unwrap();
    assert_eq!(
        init(&engine, &root, "--ignored-invalid").message,
        "InitAlreadyExists"
    );
    assert_eq!(git(&engine, &root, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(root.join(".git/config")).unwrap(), config);
    assert_eq!(fs::read(root.join("kept")).unwrap(), b"untouched");
    export(&engine, &root, "existing", "InitAlreadyExists", &["kept"]);
    let linked = temp.path().join("linked");
    git(
        &engine,
        &root,
        &["worktree", "add", "-b", "linked", linked.to_str().unwrap()],
    );
    let result = init(&engine, &linked, "");
    assert_eq!(result.message, "InitAlreadyExists");
    assert_eq!(result.repository.unwrap().branch.as_deref(), Some("linked"));
}
#[test]
fn invalid_nested_bare_and_malformed_metadata_destinations_never_reinitialize() {
    let temp = tempfile::tempdir().unwrap();
    let engine = engine(temp.path());
    let reject = |path: &Path, branch: &str, code: &str| {
        let result = engine.initialize_repository(&Options {
            directory: path.into(),
            initial_branch: branch.into(),
        });
        assert_eq!(result.err().expect("must reject").code(), code);
    };
    reject(Path::new("relative"), "", "InitInvalidDirectory");
    let file = temp.path().join("file");
    fs::write(&file, b"keep").unwrap();
    reject(&file, "", "InitInvalidDirectory");
    assert_eq!(fs::read(&file).unwrap(), b"keep");
    let bad = temp.path().join("invalid");
    for branch in ["bad..branch", "--help", "with\nnewline"] {
        reject(&bad, branch, "InitInvalidBranch");
    }
    assert!(!bad.exists());
    let existing = temp.path().join("existing");
    init(&engine, &existing, "main");
    reject(&existing.join("nested"), "", "InitNestedRepository");
    reject(&existing.join(".git/inside"), "", "InitMetadataDirectory");
    assert!(!existing.join("nested").exists());
    let bare = temp.path().join("bare");
    git(
        &engine,
        temp.path(),
        &["init", "--bare", bare.to_str().unwrap()],
    );
    reject(&bare, "", "InitBareRepository");
    let broken = temp.path().join("broken");
    fs::create_dir(&broken).unwrap();
    fs::write(broken.join(".git"), b"gitdir: missing\n").unwrap();
    assert!(
        engine
            .initialize_repository(&Options {
                directory: broken.clone(),
                initial_branch: "main".into()
            })
            .is_err()
    );
    assert_eq!(fs::read(broken.join(".git")).unwrap(), b"gitdir: missing\n");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}
fn export(git_engine: &Git, root: &Path, case: &str, message: &str, files: &[&str]) {
    let Some(output) = std::env::var_os("SHOVEL_INIT_PARITY_OUTPUT") else {
        return;
    };
    let mut worktree = std::collections::BTreeMap::new();
    for name in files {
        worktree.insert(*name, hex(&fs::read(root.join(name)).unwrap()));
    }
    let state = serde_json::json!({
        "message": message,
        "repositoryObserved": true,
        "headFile": hex(&fs::read(root.join(".git/HEAD")).unwrap_or_default()),
        "indexExists": root.join(".git/index").exists(),
        "index": if case == "partial" { String::new() } else { git(git_engine, root, &["ls-files", "--stage", "-z"]) },
        "status": if case == "partial" { String::new() } else { git(git_engine, root, &["status", "--porcelain", "-z"]) },
        "tip": if case == "existing" { git(git_engine, root, &["rev-parse", "HEAD"]) } else { String::new() },
        "config": hex(&fs::read(root.join(".git/config")).unwrap()),
        "templateMarker": hex(&fs::read(root.join(".git/fixture-marker")).unwrap_or_default()),
        "worktree": worktree,
    });
    let output = std::path::PathBuf::from(output);
    fs::create_dir_all(&output).unwrap();
    fs::write(
        output.join(format!("{case}.json")),
        serde_json::to_vec_pretty(&state).unwrap(),
    )
    .unwrap();
}
#[test]
fn failed_native_template_preserves_partial_metadata_and_original_files() {
    let temp = tempfile::tempdir().unwrap();
    let engine = engine(temp.path());
    let templates = temp.path().join("templates");
    fs::create_dir(&templates).unwrap();
    fs::write(
        templates.join("objects"),
        b"blocked native object directory",
    )
    .unwrap();
    fs::write(templates.join("fixture-marker"), b"copied template").unwrap();
    git(
        &engine,
        temp.path(),
        &[
            "config",
            "--file",
            temp.path().join("isolated.config").to_str().unwrap(),
            "init.templateDir",
            templates.to_str().unwrap(),
        ],
    );
    let root = temp.path().join("partial");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("keep.bin"), [0, 255, 1, 2]).unwrap();
    let result = init(&engine, &root, "main");
    assert!(!result.succeeded);
    assert!(
        result.repository.is_some(),
        "inspectable partial repositories remain available"
    );
    assert_eq!(result.message, "InitIncomplete");
    assert!(!result.output.is_empty());
    assert_eq!(fs::read(root.join("keep.bin")).unwrap(), [0, 255, 1, 2]);
    assert_eq!(
        fs::read(root.join(".git/objects")).unwrap(),
        b"blocked native object directory"
    );
    assert_eq!(
        fs::read(root.join(".git/fixture-marker")).unwrap(),
        b"copied template"
    );
    assert!(!root.join(".git/index").exists());
    export(&engine, &root, "partial", result.message, &["keep.bin"]);
}
