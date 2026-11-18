#![cfg(windows)]
use serde::Deserialize;
use shovel_core::{
    identity,
    working::{Error, MutationGate},
};
use std::{
    fs,
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Expected {
    path: PathBuf,
    fingerprint: String,
    common: PathBuf,
    git: PathBuf,
}
struct Held {
    child: Child,
    release: PathBuf,
    _temp: tempfile::TempDir,
}
impl Held {
    fn new(path: &std::path::Path, category: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let ready = temp.path().join("ready");
        let release = temp.path().join("release");
        let child = Command::new("dotnet")
            .arg(std::env::var_os("SHOVEL_IDENTITY_BASELINE_DLL").unwrap())
            .arg("--hold-gate")
            .arg(path)
            .arg(category)
            .arg(&ready)
            .arg(&release)
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut held = Self {
            child,
            release,
            _temp: temp,
        };
        let until = Instant::now() + Duration::from_secs(10);
        while !ready.exists() {
            assert!(
                held.child.try_wait().unwrap().is_none(),
                "reference holder exited"
            );
            assert!(Instant::now() < until, "reference holder did not acquire");
            std::thread::sleep(Duration::from_millis(10));
        }
        held
    }
}
impl Drop for Held {
    fn drop(&mut self) {
        fs::write(&self.release, "release").unwrap();
        assert!(self.child.wait().unwrap().success());
    }
}

#[test]
fn native_identity_and_gate_match_pinned_csharp() {
    let Some(evidence) = std::env::var_os("SHOVEL_IDENTITY_PARITY") else {
        eprintln!("Optional paired check: run scripts/Test-RustIdentityParity.ps1");
        return;
    };
    let evidence = PathBuf::from(evidence);
    let rows: Vec<Expected> =
        serde_json::from_slice(&fs::read(evidence.join("identities.json")).unwrap()).unwrap();
    let mut mismatches = vec![];
    for row in &rows {
        let actual = identity::read(&row.path).unwrap();
        if actual.fingerprint != row.fingerprint {
            mismatches.push(row.path.clone());
        }
        assert_eq!(actual.root, row.path);
        assert_eq!(actual.git, row.git);
        assert_eq!(actual.common, row.common);
        assert_eq!(actual, identity::read(&row.path).unwrap());
    }
    // Fingerprints are Rust-profile persisted identifiers, not a cross-product
    // wire contract. Keep their published v1 encoding so existing draft keys
    // remain readable; native roots and shared mutex names must still match.
    eprintln!(
        "Rust v1 fingerprint encoding differs from C# for {} Unicode paths",
        mismatches.len()
    );
    let scalars: String = (0..=0x10ffff).filter_map(char::from_u32).collect();
    use sha2::{Digest, Sha256};
    let upper = shovel_core::case::invariant_upper(&scalars).unwrap();
    assert_eq!(
        format!("{:X}", Sha256::digest(upper.as_bytes())),
        fs::read_to_string(evidence.join("casing-sha256.txt")).unwrap()
    );
    for row in &rows {
        for fetch in [false, true] {
            let held = Held::new(&row.path, if fetch { "Fetch" } else { "GitAction" });
            assert!(
                matches!(
                    MutationGate::acquire(&row.common, fetch),
                    Err(Error::OtherInstance)
                ),
                "C# and Rust failed to share the native gate: {:?}, fetch={fetch}",
                row.path
            );
            if !fetch && row.path.file_name().unwrap() == "straße" {
                use shovel_core::{git::Git, process::Cancellation, working::Action};
                let mut git = Git::discover(None, &Cancellation::default()).unwrap();
                git.environment.insert(
                    "GIT_CONFIG_GLOBAL".into(),
                    Some(
                        row.path
                            .parent()
                            .unwrap()
                            .join("isolated.config")
                            .into_os_string(),
                    ),
                );
                git.environment
                    .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
                let file = row.path.join("gate-check.txt");
                fs::write(&file, b"preserved payload").unwrap();
                let context = git
                    .working_copy(&row.path, &Cancellation::default())
                    .unwrap();
                let selected = context
                    .entries
                    .iter()
                    .filter(|e| e.path == "gate-check.txt")
                    .cloned()
                    .collect();
                let before = fs::read(row.git.join("index")).ok();
                assert!(matches!(
                    git.execute_working(&context, &Action::Stage(selected), None),
                    Err(Error::OtherInstance)
                ));
                assert_eq!(fs::read(row.git.join("index")).ok(), before);
                assert_eq!(fs::read(file).unwrap(), b"preserved payload");
            }
            // The baseline intentionally allows Fetch alongside working-copy work.
            assert!(MutationGate::acquire(&row.common, !fetch).is_ok());
            drop(held);
            assert!(MutationGate::acquire(&row.common, fetch).is_ok());
        }
    }
    eprintln!(
        "PASS native roots, full Unicode casing digest and eighteen cross-process C#/Rust gates"
    );
}

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    git: shovel_core::git::Git,
}
impl Fixture {
    fn new() -> Self {
        use shovel_core::process::Cancellation;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        fs::create_dir(&root).unwrap();
        let config = temp.path().join("isolated.config");
        fs::write(&config, "[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n[core]\nautocrlf=false\n").unwrap();
        let mut git = shovel_core::git::Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        let fixture = Self {
            _temp: temp,
            root,
            git,
        };
        fixture.command(&fixture.root, &["init", "--initial-branch=main"]);
        fs::write(fixture.root.join("file"), b"base\n").unwrap();
        fixture.command(&fixture.root, &["add", "file"]);
        fixture.command(&fixture.root, &["commit", "-m", "base"]);
        fs::write(fixture.root.join("file"), b"working\n").unwrap();
        fixture
    }
    fn command(&self, root: &std::path::Path, args: &[&str]) {
        use shovel_core::process::{self, Cancellation, Lifetime};
        let output = process::run(
            &self
                .git
                .request(root, args.iter().copied(), Lifetime::AcceptedCommand, 65536),
            &Cancellation::default(),
            None,
        )
        .unwrap();
        assert!(output.success(), "{output:?}");
    }
    fn review(&self, root: &std::path::Path) -> shovel_core::working::Context {
        self.git
            .working_copy(root, &shovel_core::process::Cancellation::default())
            .unwrap()
    }
    fn reject_identical_replacement(&self, before: &shovel_core::working::Context) {
        use shovel_core::working::Action;
        let root = &before.repository.worktree;
        let fresh = self.review(root);
        assert_eq!(before.repository.head, fresh.repository.head);
        assert_eq!(before.repository.branch, fresh.repository.branch);
        assert_eq!(before.index_stamp, fresh.index_stamp);
        assert_eq!(before.config_stamp, fresh.config_stamp);
        assert_eq!(before.entries, fresh.entries);
        assert_ne!(before.repository.identity, fresh.repository.identity);
        let index = fs::read(fresh.repository.git_directory.join("index")).unwrap();
        let config = fs::read(fresh.repository.common_directory.join("config")).unwrap();
        let content = fs::read(root.join("file")).unwrap();
        let selected = before
            .entries
            .iter()
            .filter(|e| e.path == "file")
            .cloned()
            .collect();
        assert!(matches!(
            self.git
                .execute_working(before, &Action::Stage(selected), None),
            Err(Error::Stale)
        ));
        assert_eq!(
            fs::read(fresh.repository.git_directory.join("index")).unwrap(),
            index
        );
        assert_eq!(
            fs::read(fresh.repository.common_directory.join("config")).unwrap(),
            config
        );
        assert_eq!(fs::read(root.join("file")).unwrap(), content);
        assert!(!fresh.repository.git_directory.join("index.lock").exists());
    }
}

// Fixture-only copy creates new native file IDs while preserving all Git state.
fn copy_tree(source: &std::path::Path, target: &std::path::Path) {
    fs::create_dir(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            assert!(entry.file_type().unwrap().is_file());
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn differently_cased_worktree_opens_stages_and_still_rejects_replacement() {
    use shovel_core::working::{Action, Group};
    let fixture = Fixture::new();
    let alias = fixture.root.with_file_name("RePo");
    let reviewed = fixture.review(&alias);
    let normal = fixture.review(&fixture.root);
    assert_eq!(reviewed.repository.identity, normal.repository.identity);
    assert_eq!(reviewed.config_stamp, normal.config_stamp);
    let selected = reviewed.entries.iter().find(|e| e.path == "file").unwrap();
    fixture
        .git
        .execute_working(&reviewed, &Action::Stage(vec![selected.clone()]), None)
        .unwrap();
    let staged = fixture.review(&fixture.root);
    assert!(
        staged
            .entries
            .iter()
            .any(|e| e.path == "file" && e.group == Group::Staged)
    );

    fs::write(fixture.root.join("file"), b"later edit\n").unwrap();
    let before = fixture.review(&alias);
    let moved = fixture._temp.path().join("previous-root");
    fs::rename(&fixture.root, &moved).unwrap();
    copy_tree(&moved, &fixture.root);
    fixture.reject_identical_replacement(&before);
}

#[test]
fn dos_short_alias_shares_git_ownership_and_recovers_its_previous_draft() {
    use shovel_core::{
        drafts,
        process::Cancellation,
        working::{Action, Group},
    };
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;
    let fixture = Fixture::new();
    let root = fixture._temp.path().join("Repository with a long name");
    fs::rename(&fixture.root, &root).unwrap();
    let wide: Vec<_> = root.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut buffer = vec![0; 32768];
    let count =
        unsafe { GetShortPathNameW(wide.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) }
            as usize;
    assert!(count > 0 && count < buffer.len());
    let alias = PathBuf::from(std::ffi::OsString::from_wide(&buffer[..count]));
    if shovel_core::case::path_key(&root) == shovel_core::case::path_key(&alias) {
        eprintln!("This volume does not assign a distinct DOS short name.");
        return;
    }
    let reviewed = fixture.review(&alias);
    let normal = fixture.review(&root);
    assert_eq!(reviewed.repository.identity, normal.repository.identity);
    assert_eq!(
        reviewed.repository.common_directory,
        normal.repository.common_directory
    );
    let from_alias = identity::read(&alias).unwrap();
    let prior = from_alias
        .previous_fingerprint()
        .expect("old short-path fingerprint");
    assert!(from_alias.matches_recorded_fingerprint(prior));
    let mut old = from_alias.clone();
    old.fingerprint = prior.to_owned();
    let key = drafts::Store::key(&old);
    let store = drafts::Store {
        directory: fixture._temp.path().join("drafts"),
        legacy_directory: None,
    };
    fs::create_dir(&store.directory).unwrap();
    let old_file = store.directory.join(format!("{key}.json"));
    let original = serde_json::to_vec(&serde_json::json!({
        "SchemaVersion": 1, "Key": key, "Revision": "a".repeat(32),
        "Subject": "draft from a previous version", "Description": "retain both files"
    }))
    .unwrap();
    fs::write(&old_file, &original).unwrap();
    let cancel = Cancellation::default();
    let imported = store.load(&from_alias, &cancel).unwrap();
    assert_eq!(imported.text.subject, "draft from a previous version");
    assert!(imported.revision.is_none());
    store
        .save(&from_alias, None, &imported.text, &cancel)
        .unwrap();
    assert_eq!(
        store
            .load(&normal.repository.identity, &cancel)
            .unwrap()
            .text,
        imported.text
    );
    assert_eq!(fs::read(old_file).unwrap(), original);

    let selected = reviewed
        .entries
        .iter()
        .find(|entry| entry.path == "file")
        .unwrap()
        .clone();
    let gate = MutationGate::acquire(&normal.repository.common_directory, false).unwrap();
    let action = Action::Stage(vec![selected.clone()]);
    std::thread::scope(|threads| {
        assert!(
            threads
                .spawn(|| matches!(
                    fixture.git.execute_working(&reviewed, &action, None),
                    Err(Error::OtherInstance)
                ))
                .join()
                .unwrap()
        );
    });
    drop(gate);
    let staged = fixture
        .git
        .execute_working(&reviewed, &Action::Stage(vec![selected]), None)
        .unwrap();
    assert!(
        staged
            .updated
            .unwrap()
            .change(Group::Staged, "file")
            .is_some()
    );
    let before = fixture.review(&alias);
    let moved = fixture._temp.path().join("old-root");
    fs::rename(&root, &moved).unwrap();
    copy_tree(&moved, &root);
    fixture.reject_identical_replacement(&before);
    eprintln!(
        "Verified distinct DOS alias, shared mutation gate, legacy draft import and worktree replacement."
    );
}

#[test]
fn native_lock_holder_fixture() {
    use std::os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    };
    use windows_sys::Win32::{
        Foundation::WAIT_OBJECT_0,
        System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
    };
    let Some(name) = std::env::var_os("SHOVEL_TEST_NATIVE_MUTEX") else {
        return;
    };
    let name: Vec<_> = name.encode_wide().chain(Some(0)).collect();
    let raw = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    assert!(!raw.is_null(), "{}", std::io::Error::last_os_error());
    let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
    assert_eq!(
        unsafe { WaitForSingleObject(handle.as_raw_handle(), 0) },
        WAIT_OBJECT_0
    );
    fs::write(std::env::var_os("SHOVEL_TEST_MUTEX_READY").unwrap(), b"ready").unwrap();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    assert_ne!(unsafe { ReleaseMutex(handle.as_raw_handle()) }, 0);
}

#[test]
fn stage_in_linked_worktree_respects_both_native_mutex_namespaces() {
    use shovel_core::working::Action;
    use sha2::{Digest, Sha256};

    struct ChildLock(Child);
    impl Drop for ChildLock {
        fn drop(&mut self) {
            drop(self.0.stdin.take());
            let _ = self.0.wait();
        }
    }

    let fixture = Fixture::new();
    let linked = fixture._temp.path().join("linked");
    fixture.command(
        &fixture.root,
        &["worktree", "add", "--detach", linked.to_str().unwrap()],
    );
    fs::write(linked.join("file"), b"linked edit\n").unwrap();
    let review = fixture.review(&linked);
    let common = &review.repository.common_directory;
    assert_eq!(&identity::read(&fixture.root).unwrap().common, common);
    let key = Sha256::digest(
        shovel_core::case::invariant_upper(&common.to_string_lossy().replace('/', "\\"))
            .unwrap()
            .as_bytes(),
    );
    let index_path = review.repository.git_directory.join("index");
    let before = fs::read(&index_path).unwrap();
    let selected = Action::Stage(review.entries.clone());
    for scope in ["Global", "Local"] {
        let signal = fixture._temp.path().join(format!("{scope}-ready"));
        let mut owner = ChildLock(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "native_lock_holder_fixture"])
                .env(
                    "SHOVEL_TEST_NATIVE_MUTEX",
                    format!("{scope}\\Shovel.GitAction.{key:X}"),
                )
                .env("SHOVEL_TEST_MUTEX_READY", &signal)
                .creation_flags(0x08000000)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while !signal.exists() {
            assert!(owner.0.try_wait().unwrap().is_none(), "{scope} holder exited");
            assert!(Instant::now() < deadline, "{scope} holder did not acquire");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            matches!(
                fixture.git.execute_working(&review, &selected, None),
                Err(Error::OtherInstance)
            ),
            "{scope} mutex did not block Stage"
        );
        assert_eq!(fs::read(&index_path).unwrap(), before);
        // A failed acquisition must never strand the other lock or block Fetch's separate lane.
        drop(MutationGate::acquire(common, true).unwrap());
        drop(owner);
        drop(MutationGate::acquire(common, false).unwrap());
    }
    let staged = fixture
        .git
        .execute_working(&review, &selected, None)
        .unwrap();
    assert!(staged.command.success());
    assert_ne!(fs::read(index_path).unwrap(), before);
}

#[test]
fn replaced_root_and_git_directory_reject_stage_despite_identical_git_state() {
    let f = Fixture::new();
    let before = f.review(&f.root);
    let moved = f._temp.path().join("previous-root");
    fs::rename(&f.root, &moved).unwrap();
    copy_tree(&moved, &f.root);
    f.reject_identical_replacement(&before);
    let before = f.review(&f.root);
    let previous_git = f._temp.path().join("previous-git");
    fs::rename(f.root.join(".git"), &previous_git).unwrap();
    copy_tree(&previous_git, &f.root.join(".git"));
    f.reject_identical_replacement(&before);
}

#[test]
fn linked_pointer_edits_in_place_reject_stage_without_changing_git_state() {
    use std::{io::Write, os::windows::fs::MetadataExt};
    let f = Fixture::new();
    let linked = f._temp.path().join("linked");
    f.command(
        &f.root,
        &["worktree", "add", "-b", "linked", linked.to_str().unwrap()],
    );
    fs::write(linked.join("file"), b"linked working\n").unwrap();
    let before = f.review(&linked);
    assert_eq!(
        before.repository.common_directory,
        f.review(&f.root).repository.common_directory
    );
    for pointer in [
        linked.join(".git"),
        before.repository.git_directory.join("commondir"),
    ] {
        let review = f.review(&linked);
        let created = fs::metadata(&pointer).unwrap().creation_time();
        fs::OpenOptions::new()
            .append(true)
            .open(&pointer)
            .unwrap()
            .write_all(b"\n")
            .unwrap();
        assert_eq!(fs::metadata(&pointer).unwrap().creation_time(), created);
        // Target directories and Git state are identical. Only the reviewed
        // pointer content changed, without replacing its native file object.
        let identity = identity::read(&linked).unwrap();
        assert_eq!(identity.git, review.repository.identity.git);
        assert_eq!(identity.common, review.repository.identity.common);
        f.reject_identical_replacement(&review);
    }
}

#[test]
fn linked_common_directory_replacement_invalidates_both_worktrees() {
    let f = Fixture::new();
    let linked = f._temp.path().join("linked");
    f.command(
        &f.root,
        &["worktree", "add", "-b", "linked", linked.to_str().unwrap()],
    );
    fs::write(linked.join("file"), b"linked working\n").unwrap();
    let main = f.review(&f.root);
    let child = f.review(&linked);
    let previous = f._temp.path().join("previous-common");
    fs::rename(f.root.join(".git"), &previous).unwrap();
    copy_tree(&previous, &f.root.join(".git"));
    f.reject_identical_replacement(&main);
    f.reject_identical_replacement(&child);
}

#[test]
fn replaced_identity_rejects_mutation_families_before_native_writes() {
    use shovel_core::{
        branches, file_rules, history_actions, network, process::Cancellation, remotes, stashes,
    };
    let f = Fixture::new();
    fs::write(f.root.join("new.txt"), b"untracked\n").unwrap();
    let remote = f._temp.path().join("remote.git");
    f.command(&f.root, &["init", "--bare", remote.to_str().unwrap()]);
    f.command(
        &f.root,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    let working = f.review(&f.root);
    let snapshot = &working.repository;
    let local = f
        .git
        .prepare_local(snapshot, true, &Cancellation::default())
        .unwrap();
    let network = f
        .git
        .prepare_network(&f.root, &Cancellation::default())
        .unwrap();
    let start = snapshot
        .references
        .iter()
        .find(|r| r.name == "refs/heads/main")
        .unwrap()
        .clone();
    let tracked: Vec<_> = working
        .entries
        .iter()
        .filter(|e| e.path == "file")
        .cloned()
        .collect();
    let untracked: Vec<_> = working
        .entries
        .iter()
        .filter(|e| e.path == "new.txt")
        .cloned()
        .collect();
    let moved = f._temp.path().join("previous-root");
    fs::rename(&f.root, &moved).unwrap();
    copy_tree(&moved, &f.root);

    let before_index = fs::read(f.root.join(".git/index")).unwrap();
    let before_config = fs::read(f.root.join(".git/config")).unwrap();
    let patch = f._temp.path().join("export.patch");
    fs::write(&patch, b"previous export").unwrap();
    let rejection = |result: Result<(), shovel_core::operation::Error>| {
        result.expect_err("replaced context must fail").code()
    };
    let results = [
        (
            "branch",
            rejection(
                f.git
                    .execute_branch(
                        &local,
                        &branches::Action::Create {
                            name: "must-not-exist".into(),
                            start,
                        },
                        None,
                    )
                    .map(|_| ()),
            ),
            "ActionIdentityChanged",
        ),
        (
            "history",
            rejection(
                f.git
                    .execute_history(
                        &local,
                        &history_actions::Action::Reset {
                            target: snapshot.head.clone().unwrap(),
                            mode: history_actions::Reset::Hard,
                        },
                        None,
                    )
                    .map(|_| ()),
            ),
            "ActionIdentityChanged",
        ),
        (
            "network",
            rejection(
                f.git
                    .execute_network(
                        &network,
                        &network::Action::Fetch {
                            remote: Some("origin".into()),
                            all: false,
                        },
                        None,
                        None,
                    )
                    .map(|_| ()),
            ),
            "ActionIdentityChanged",
        ),
        (
            "remote",
            rejection(
                f.git
                    .execute_remote(
                        snapshot,
                        &remotes::Action::Add {
                            name: "must-not-exist".into(),
                            location: "../remote.git".into(),
                        },
                        None,
                    )
                    .map(|_| ()),
            ),
            "ActionIdentityChanged",
        ),
        (
            "stash",
            rejection(
                f.git
                    .execute_stash(
                        snapshot,
                        &stashes::Action::Create {
                            message: "must-not-exist".into(),
                            include_untracked: true,
                        },
                        None,
                    )
                    .map(|_| ()),
            ),
            "ActionIdentityChanged",
        ),
        (
            "selected stash",
            rejection(
                f.git
                    .stash_files(&working, &tracked, "must-not-exist")
                    .map(|_| ()),
            ),
            "CommitStale",
        ),
        (
            "discard",
            rejection(f.git.discard_files(&working, &tracked).map(|_| ())),
            "CommitStale",
        ),
        (
            "ignore",
            rejection(
                f.git
                    .edit_file_rules(&working, &untracked, file_rules::Action::IgnorePaths)
                    .map(|_| ()),
            ),
            "CommitStale",
        ),
        (
            "external diff",
            rejection(f.git.external_file_diff(&working, &tracked).map(|_| ())),
            "CommitStale",
        ),
        (
            "patch export",
            rejection(
                f.git
                    .export_selected_patch(&working, &tracked, &patch)
                    .map(|_| ()),
            ),
            "CommitStale",
        ),
    ];
    for (name, actual, expected) in results {
        assert_eq!(actual, expected, "{name}");
    }
    assert_eq!(fs::read(f.root.join(".git/index")).unwrap(), before_index);
    assert_eq!(fs::read(f.root.join(".git/config")).unwrap(), before_config);
    assert_eq!(fs::read(f.root.join("file")).unwrap(), b"working\n");
    assert_eq!(fs::read(f.root.join("new.txt")).unwrap(), b"untracked\n");
    assert_eq!(fs::read(patch).unwrap(), b"previous export");
    for path in [
        ".git/refs/heads/must-not-exist",
        ".git/refs/stash",
        ".git/FETCH_HEAD",
        ".git/index.lock",
        ".gitignore",
    ] {
        assert!(!f.root.join(path).exists(), "{path} must not be written");
    }
}
