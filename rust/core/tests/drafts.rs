#![cfg(windows)]
use shovel_core::{
    drafts::{Error, Store, Text},
    identity,
    process::Cancellation,
};
use std::{fs, fs::OpenOptions, os::windows::fs::OpenOptionsExt, time::Duration};

fn fixture() -> (tempfile::TempDir, identity::Identity, Store) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repository");
    fs::create_dir_all(root.join(".git")).unwrap();
    let identity = identity::read(&root).unwrap();
    let store = Store {
        directory: temp.path().join("rust"),
        legacy_directory: Some(temp.path().join("csharp")),
    };
    (temp, identity, store)
}
#[test]
fn imported_draft_is_copied_and_revision_races_preserve_both_messages() {
    let (_temp, id, store) = fixture();
    let cancel = Cancellation::default();
    let legacy = Store {
        directory: store.legacy_directory.clone().unwrap(),
        legacy_directory: None,
    };
    let original = Text {
        subject: "שלום subject".into(),
        description: "body\r\nline".into(),
    };
    legacy.save(&id, None, &original, &cancel).unwrap();
    let key = Store::key(&id);
    let legacy_path = legacy.directory.join(format!("{key}.json"));
    let legacy_bytes = fs::read(&legacy_path).unwrap();
    let imported = store.load(&id, &cancel).unwrap();
    assert_eq!(imported.text, original);
    assert!(imported.revision.is_none());
    let first = store.save(&id, None, &original, &cancel).unwrap();
    let changed = Text {
        subject: "newer".into(),
        description: String::new(),
    };
    let second = store
        .save(&id, first.revision.as_deref(), &changed, &cancel)
        .unwrap();
    assert!(matches!(
        store.save(&id, first.revision.as_deref(), &original, &cancel),
        Err(Error::Changed)
    ));
    store
        .recovery(&id, "0123456789abcdef0123456789abcdef", &original, &cancel)
        .unwrap();
    let loaded = store.load(&id, &cancel).unwrap();
    assert_eq!(loaded.text, changed);
    assert!(loaded.has_recovery);
    store
        .save(&id, second.revision.as_deref(), &Text::default(), &cancel)
        .unwrap();
    assert!(store.load(&id, &cancel).unwrap().text.is_empty());
    assert_eq!(fs::read(legacy_path).unwrap(), legacy_bytes);
}
#[test]
fn invalid_newer_replaced_identity_and_cancelled_drafts_are_preserved() {
    let (_temp, id, store) = fixture();
    fs::create_dir_all(&store.directory).unwrap();
    let path = store.directory.join(format!("{}.json", Store::key(&id)));
    let cancel = Cancellation::default();
    fs::write(&path, b"corrupt").unwrap();
    assert!(matches!(store.load(&id, &cancel), Err(Error::Invalid)));
    assert!(store.save(&id, None, &Text::default(), &cancel).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"corrupt");
    let newer = serde_json::json!({"SchemaVersion":2,"Key":Store::key(&id),"Revision":"0123456789abcdef0123456789abcdef","Subject":"next","Description":""});
    fs::write(&path, serde_json::to_vec(&newer).unwrap()).unwrap();
    assert!(matches!(store.load(&id, &cancel), Err(Error::Newer)));
    fs::rename(id.root.join(".git"), id.root.join(".git.old")).unwrap();
    fs::create_dir(id.root.join(".git")).unwrap();
    assert!(matches!(store.load(&id, &cancel), Err(Error::Identity)));
    let cancelled = Cancellation::default();
    cancelled.cancel();
    assert!(matches!(store.load(&id, &cancelled), Err(Error::Cancelled)));
}
#[test]
fn transient_writer_lock_retries_and_then_saves() {
    let (_temp, id, store) = fixture();
    fs::create_dir_all(&store.directory).unwrap();
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(store.directory.join(format!("{}.lock", Store::key(&id))))
        .unwrap();
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(120));
        drop(lock);
    });
    let text = Text {
        subject: "survives sharing conflict".into(),
        description: String::new(),
    };
    let saved = store
        .save(&id, None, &text, &Cancellation::default())
        .unwrap();
    worker.join().unwrap();
    assert_eq!(saved.text, text);
}
