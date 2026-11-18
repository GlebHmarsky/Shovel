#![cfg(windows)]
use chrono::Utc;
use shovel_core::{
    git::{Commit, Entry, Snapshot},
    graph,
    history_cache::{MAX_BYTES, PREVIEW_SCHEMA, Preview, Store},
    identity::Identity,
};
use std::{fs, fs::OpenOptions, os::windows::fs::OpenOptionsExt};
fn preview(root: &std::path::Path) -> Preview {
    let identity = Identity {
        root: root.into(),
        git: root.join(".git"),
        common: root.join(".git"),
        fingerprint: "A".repeat(64),
    };
    let now = Utc::now();
    Preview {
        schema: PREVIEW_SCHEMA,
        directory: root.into(),
        identity: identity.fingerprint.clone(),
        repository: Snapshot {
            identity,
            worktree: root.into(),
            git_directory: root.join(".git"),
            common_directory: root.join(".git"),
            branch: Some("main".into()),
            head: Some("b".repeat(40)),
            upstream: None,
            upstream_target: None,
            remotes: vec![],
            operations: vec![],
            recovery: None,
            shallow: false,
            fingerprint: "c".repeat(64),
            references: vec![],
        },
        entries: vec![Entry {
            commit: Commit {
                hash: "b".repeat(40),
                parents: vec![],
                author: "Fixture".into(),
                email: "fixture@example.invalid".into(),
                authored: now.fixed_offset(),
                committer: "Fixture".into(),
                committer_email: "fixture@example.invalid".into(),
                committed: now.fixed_offset(),
                message: "Unicode שלום 日本語\ncomplete message".into(),
            },
            graph: graph::Row {
                node: 0,
                color: 0,
                width: 1,
                is_merge: false,
                is_stash: false,
                lines: vec![],
            },
            references: vec![],
        }],
        offset: 400,
        selected: Some("b".repeat(40)),
        vertical: 52.,
        horizontal: 27.,
        saved_at: now,
    }
}
#[test]
fn selected_cache_roundtrip_replaces_atomically_and_preserves_unrelated_profile_files() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        directory: temp.path().join("profile"),
    };
    let mut value = preview(temp.path());
    store.save(Some(&value));
    fs::write(store.directory.join("settings.json"), "untouched").unwrap();
    let first = store.read(temp.path(), &value.identity).unwrap();
    assert_eq!(first.offset, 400);
    assert_eq!(first.vertical, 52.);
    assert_eq!(first.horizontal, 27.);
    assert_eq!(first.selected, value.selected);
    assert_eq!(
        first.entries[0].commit.message,
        value.entries[0].commit.message
    );
    let original = fs::read(store.path()).unwrap();
    let held = OpenOptions::new()
        .read(true)
        .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
        .open(store.path())
        .unwrap();
    value.entries[0].commit.message = "replacement".into();
    store.save(Some(&value)); // Windows sharing failure cannot destroy the last complete cache.
    assert_eq!(fs::read(store.path()).unwrap(), original);
    assert_eq!(fs::read_dir(&store.directory).unwrap().count(), 2);
    drop(held);
    store.save(Some(&value));
    assert_eq!(
        store.read(temp.path(), &value.identity).unwrap().entries[0]
            .commit
            .message,
        "replacement"
    );
    store.save(None);
    assert!(!store.path().exists());
    assert_eq!(
        fs::read_to_string(store.directory.join("settings.json")).unwrap(),
        "untouched"
    );
}
#[test]
fn cache_validation_rejects_malformed_paths_identity_graph_counts_and_offsets() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        directory: temp.path().join("profile"),
    };
    let valid = preview(temp.path());
    store.save(Some(&valid));
    assert!(store.read(temp.path(), &"D".repeat(64)).is_none());
    assert!(
        store
            .read(&temp.path().join("different"), &valid.identity)
            .is_none()
    );
    for index in 0..10 {
        let mut bad = valid.clone();
        match index {
            0 => bad.schema = 1,
            1 => bad.repository.worktree = temp.path().join("elsewhere"),
            2 => bad.entries[0].graph.width = 65,
            3 => bad.entries[0].graph.node = 64,
            4 => bad.entries[0].graph.lines.push(graph::Line {
                from: 0,
                from_y: -0.01,
                to: 0,
                to_y: 1.,
                color: 0,
            }),
            5 => {
                bad.entries[0].graph.lines = vec![
                    graph::Line {
                        from: 0,
                        from_y: 0.,
                        to: 0,
                        to_y: 1.,
                        color: 0
                    };
                    257
                ]
            }
            6 => bad.entries = vec![valid.entries[0].clone(); 201],
            7 => bad.vertical = -1.,
            8 => bad.offset = usize::MAX,
            _ => bad.repository.identity.fingerprint = "E".repeat(64),
        }
        fs::write(store.path(), serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(
            store.read(temp.path(), &valid.identity).is_none(),
            "case {index}"
        );
    }
    for bytes in [
        b"{".as_slice(),
        b"null",
        b"{}",
        b"{\"schema\":1,\"entries\":null}",
    ] {
        fs::write(store.path(), bytes).unwrap();
        assert!(store.read(temp.path(), &valid.identity).is_none());
        assert_eq!(fs::read(store.path()).unwrap(), bytes);
    }
    let mut bad = valid.clone();
    bad.horizontal = f32::NAN;
    assert!(!bad.valid(temp.path(), &valid.identity));
}
#[test]
fn cache_read_and_serialization_are_bounded_and_unwritable_store_is_disposable() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        directory: temp.path().join("profile"),
    };
    let mut value = preview(temp.path());
    store.save(Some(&value));
    fs::OpenOptions::new()
        .write(true)
        .open(store.path())
        .unwrap()
        .set_len(MAX_BYTES as u64 + 1)
        .unwrap();
    assert!(store.read(temp.path(), &value.identity).is_none());
    value.entries[0].commit.message = "x".repeat(MAX_BYTES);
    store.save(Some(&value));
    assert!(!store.path().exists());
    assert_eq!(fs::read_dir(&store.directory).unwrap().count(), 0);
    let blocked = temp.path().join("file-not-directory");
    fs::write(&blocked, b"keep").unwrap();
    let blocked_store = Store {
        directory: blocked.clone(),
    };
    value.entries[0].commit.message = "small".into();
    blocked_store.save(Some(&value));
    assert!(blocked_store.read(temp.path(), &value.identity).is_none());
    assert_eq!(fs::read(blocked).unwrap(), b"keep");
}
