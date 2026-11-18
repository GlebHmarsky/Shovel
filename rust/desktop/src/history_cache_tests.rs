use super::*;
use crate::{
    history_view::tests::{Fixture, page, position, wait},
    workspace_interaction_tests::{frame, workspace},
};
use std::fs;

fn profile(app: &mut Workspace, root: &std::path::Path) {
    let mut store = SettingsStore::new(root.join("rust-profile"));
    store.load().unwrap();
    app.store = Some(store);
}
fn settle_save(app: &mut Workspace, ctx: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.saving || app.preferences_dirty || app.pending_exit {
        app.receive(ctx);
        assert!(Instant::now() < deadline, "save did not settle");
        thread::sleep(Duration::from_millis(5));
    }
}
fn await_preview(app: &mut Workspace, ctx: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app
        .tabs
        .get(app.selected)
        .is_none_or(|t| t.view.preview.is_none())
    {
        app.receive(ctx);
        assert!(Instant::now() < deadline, "preview not published");
        thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn orderly_close_reloads_selected_preview_without_commands_and_live_history_replaces_it() {
    let fixture = Fixture::new();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = fixture.app(&ctx);
    profile(&mut app, fixture._temp.path());
    let profile_dir = app.startup_store().unwrap().directory;
    app.ensure_tab(fixture.root.join("inactive-not-opened"));
    page(&mut app, &ctx, true);
    let expected = position(&mut app, &ctx, 12, egui::vec2(0., 230.));
    app.close(&ctx);
    settle_save(&mut app, &ctx);
    assert!(app.exit_ready);
    let cache = Store {
        directory: profile_dir.clone(),
    };
    let saved = cache
        .read(&fixture.root, &app.known_identity(&fixture.root).unwrap())
        .unwrap();
    assert_eq!(saved.offset, 200);
    assert_eq!(saved.selected.as_deref(), Some(expected.0.as_str()));
    assert_eq!(saved.vertical, expected.1.y);
    // Run the real initialized-event path with Git unavailable. The cached view is
    // still useful but cannot authorize Git, load comparison files or create an owner snapshot.
    let mut store = SettingsStore::new(profile_dir);
    let settings = store.load().unwrap();
    let mut reopened = workspace();
    reopened.tabs.clear();
    reopened
        .sender
        .send(Event::Initialized(
            Box::new(settings),
            Box::new(store),
            Err("fixture unavailable Git".into()),
            None,
        ))
        .unwrap();
    await_preview(&mut reopened, &ctx);
    assert!(reopened.tabs[1].page.is_none() && !reopened.tabs[1].loading);
    let tab = &reopened.tabs[0];
    assert!(tab.history.is_none() && tab.owner_snapshot.is_none());
    assert!(reopened.capture_startup_preview().is_none());
    assert_eq!(tab.page.as_ref().unwrap().cursor.offset, 200);
    assert_eq!(tab.selected.as_deref(), Some(expected.0.as_str()));
    let output = frame(&ctx, &mut reopened, vec![]);
    let labels = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter_map(|(_, n)| n.label().or_else(|| n.value()).map(str::to_owned))
        .collect::<Vec<_>>();
    output.drop_without_applying_deltas();
    assert!(
        labels
            .iter()
            .any(|v| v.starts_with("Last-known view from "))
    );
    assert!(labels.iter().any(|v| v == &expected.0));
    assert!(reopened.tabs[0].comparison.is_none());
    assert!(reopened.scheduler.records().is_empty());
    reopened.git = Some(Arc::new(fixture.git.clone()));
    reopened.refresh(&ctx);
    wait(&mut reopened, &ctx);
    assert!(reopened.tabs[0].view.preview.is_none());
    assert_eq!(reopened.tabs[0].page.as_ref().unwrap().cursor.offset, 0);
    assert!(reopened.tabs[0].history.is_some() && reopened.tabs[0].owner_snapshot.is_some());
    assert!(reopened.tabs[1].history.is_none());
    settle_save(&mut reopened, &ctx);
}
#[test]
fn late_preview_wrong_generation_and_changed_identity_never_replace_live_state() {
    let fixture = Fixture::new();
    let ctx = egui::Context::default();
    let mut app = fixture.app(&ctx);
    profile(&mut app, fixture._temp.path());
    let preview = app.capture_startup_preview().unwrap();
    app.startup_store().unwrap().save(Some(&preview));
    let id = app.tabs[0].id;
    let generation = app.tabs[0].view.generation;
    let allocation = app.tabs[0].page.as_ref().unwrap().entries.as_ptr() as usize;
    app.sender
        .send(Event::Preview(
            id,
            generation,
            Some(Box::new(preview.clone())),
        ))
        .unwrap();
    app.receive(&ctx);
    assert_eq!(
        app.tabs[0].page.as_ref().unwrap().entries.as_ptr() as usize,
        allocation
    );
    app.tabs[0].evict_history();
    app.sender
        .send(Event::Preview(
            id,
            generation.wrapping_sub(1),
            Some(Box::new(preview.clone())),
        ))
        .unwrap();
    app.receive(&ctx);
    assert!(app.tabs[0].page.is_none());
    app.closing = true;
    app.sender
        .send(Event::Preview(id, generation, Some(Box::new(preview))))
        .unwrap();
    app.receive(&ctx);
    assert!(app.tabs[0].page.is_none());
    app.closing = false;
    settle_save(&mut app, &ctx);
    fs::rename(fixture.root.join(".git"), fixture.root.join("old.git")).unwrap();
    fixture.command(&["init", "--initial-branch=replaced"], &[]);
    app.restore_startup_preview(&ctx);
    match app.receiver.recv_timeout(Duration::from_secs(5)).unwrap() {
        Event::Preview(_, _, None) => {}
        _ => panic!("changed directory produced a preview"),
    }
    assert!(app.tabs[0].page.is_none());
}
#[test]
fn three_tab_cache_evicts_idle_pages_but_retains_active_owner_and_observed_fetch_metadata() {
    use shovel_core::scheduler::{Lane, Outcome};
    let fixtures = (0..4).map(|_| Fixture::new()).collect::<Vec<_>>();
    let ctx = egui::Context::default();
    let mut app = fixtures[0].app(&ctx);
    let owner = app.tabs[0].observed_snapshot().unwrap().clone();
    let (release, held) = mpsc::channel();
    let (_, accepted) = app.scheduler.submit(
        owner.identity.fingerprint.clone(),
        owner.common_directory.to_string_lossy().into_owned(),
        "held-owner".into(),
        Lane::Exclusive,
        move || {
            held.recv_timeout(Duration::from_secs(25)).unwrap();
            Outcome::Succeeded
        },
        |_| {},
    );
    assert!(accepted);
    for (index, fixture) in fixtures.iter().enumerate().skip(1) {
        fixture.command(
            &[
                "remote",
                "add",
                "origin",
                "https://example.invalid/no-network",
            ],
            &[],
        );
        app.git = Some(Arc::new(fixture.git.clone()));
        app.open(fixture.root.clone(), &ctx);
        wait(&mut app, &ctx);
        if index == 1 {
            page(&mut app, &ctx, true);
        }
    }
    assert!(app.tabs[0].page.is_some(), "accepted owner was evicted");
    assert!(
        app.tabs[1].page.is_none(),
        "oldest idle page was not evicted"
    );
    let snapshot = app.tabs[1].observed_snapshot().unwrap();
    assert!(snapshot.references.is_empty());
    assert_eq!(snapshot.references.capacity(), 0);
    assert_eq!(snapshot.remotes, ["origin"]);
    assert_eq!(app.tabs.iter().filter(|t| t.page.is_some()).count(), 3);
    app.git = Some(Arc::new(fixtures[1].git.clone()));
    app.select_tab(1, &ctx);
    wait(&mut app, &ctx);
    assert_eq!(app.tabs[1].page.as_ref().unwrap().cursor.offset, 0);
    assert_eq!(app.tabs[1].page.as_ref().unwrap().entries.len(), 200);
    // Exercise the conservative budget independently of native Git's page output cap.
    for tab in &mut app.tabs {
        if tab.page.is_some() {
            tab.view.bytes = 20 * 1024 * 1024;
        }
    }
    app.evict_history_caches();
    assert_eq!(app.tabs.iter().filter(|t| t.page.is_some()).count(), 2);
    assert!(app.tabs[0].page.is_some() && app.tabs[1].page.is_some());
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.scheduler.has_accepted_work() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    app.receive(&ctx);
    assert!(app.tabs[0].page.is_none());
    assert_eq!(app.tabs[1].page.as_ref().unwrap().entries.len(), 200);
}
#[test]
fn evicted_repository_replacement_still_requires_explicit_reopen() {
    let fixture = Fixture::new();
    let ctx = egui::Context::default();
    let mut app = fixture.app(&ctx);
    app.tabs[0].evict_history();
    fs::rename(fixture.root.join(".git"), fixture.root.join("original.git")).unwrap();
    fixture.command(&["init", "--initial-branch=after-replacement"], &[]);
    app.select_tab(0, &ctx);
    wait(&mut app, &ctx);
    assert!(app.tabs[0].view.identity_blocked);
    assert!(app.tabs[0].page.is_none());
    app.open(fixture.root.clone(), &ctx);
    wait(&mut app, &ctx);
    assert!(!app.tabs[0].view.identity_blocked);
    assert_eq!(
        app.tabs[0].observed_snapshot().unwrap().branch.as_deref(),
        Some("after-replacement")
    );
}
#[test]
fn corrupt_cache_never_replaces_the_actual_error_or_changes_profile_settings() {
    let fixture = Fixture::new();
    let ctx = egui::Context::default();
    let mut app = fixture.app(&ctx);
    profile(&mut app, fixture._temp.path());
    settle_save(&mut app, &ctx);
    let store = app.startup_store().unwrap();
    fs::write(store.path(), b"not valid json").unwrap();
    let original = fs::read(app.store.as_ref().unwrap().path()).unwrap();
    app.tabs[0].evict_history();
    app.tabs[0].error = Some("actual failure".into());
    app.restore_startup_preview(&ctx);
    match app.receiver.recv_timeout(Duration::from_secs(5)).unwrap() {
        Event::Preview(_, _, None) => {}
        _ => panic!("malformed cache produced a preview"),
    }
    assert_eq!(app.tabs[0].error.as_deref(), Some("actual failure"));
    assert_eq!(
        fs::read(app.store.as_ref().unwrap().path()).unwrap(),
        original
    );
    assert_eq!(fs::read(store.path()).unwrap(), b"not valid json");
}

// A per-disposable-path gate holds only the test read before its real native Git
// call. It is absent from release builds and cannot affect another fixture.
type ReadGate = (mpsc::Sender<()>, mpsc::Receiver<()>);
fn read_gates() -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, ReadGate>> {
    static GATES: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<PathBuf, ReadGate>>,
    > = std::sync::OnceLock::new();
    GATES.get_or_init(Default::default)
}
pub(crate) fn hold_read(path: PathBuf) -> (mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (ready, started) = mpsc::channel();
    let (release, held) = mpsc::channel();
    read_gates().lock().unwrap().insert(path, (ready, held));
    (started, release)
}
pub(crate) fn wait_for_read(path: &std::path::Path) {
    let gate = read_gates().lock().unwrap().remove(path);
    if let Some((started, release)) = gate {
        let _ = started.send(());
        let _ = release.recv_timeout(Duration::from_secs(20));
    }
}
#[test]
fn preview_publishes_before_held_native_initialization_and_actions_wait_for_live_snapshot() {
    let fixture = Fixture::new();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut original = fixture.app(&ctx);
    profile(&mut original, fixture._temp.path());
    let (selected, offset) = position(&mut original, &ctx, 12, egui::vec2(0., 230.));
    original.close(&ctx);
    settle_save(&mut original, &ctx);
    let directory = original.startup_store().unwrap().directory;
    let mut store = SettingsStore::new(directory);
    let settings = store.load().unwrap();
    let mut app = workspace();
    app.tabs.clear();
    let (ready, started) = mpsc::channel();
    let (release, held) = mpsc::channel();
    read_gates()
        .lock()
        .unwrap()
        .insert(fixture.root.clone(), (ready, held));
    app.sender
        .send(Event::Initialized(
            Box::new(settings),
            Box::new(store),
            Ok(fixture.git.clone()),
            None,
        ))
        .unwrap();
    app.receive(&ctx);
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    await_preview(&mut app, &ctx);
    assert!(app.tabs[0].loading && app.tabs[0].history.is_none());
    assert_eq!(app.tabs[0].view.offset.y, 230.);
    assert!(app.tabs[0].owner_snapshot.is_none());
    crate::history_view::tests::click_label(&mut app, &ctx, "Fetch");
    let records = app.scheduler.records();
    assert_eq!(records.len(), 1);
    assert!(
        !records[0].running && records[0].outcome.is_none(),
        "preview authorized a command"
    );
    assert!(app.scheduler.remove_waiting(records[0].id));
    release.send(()).unwrap();
    wait(&mut app, &ctx);
    assert!(app.tabs[0].view.preview.is_none());
    assert!(app.tabs[0].history.is_some());
    assert_eq!(app.tabs[0].selected.as_ref(), Some(&selected));
    assert_eq!(app.tabs[0].view.offset, offset);
    assert_eq!(app.tabs[0].page.as_ref().unwrap().entries.len(), 200);
    settle_save(&mut app, &ctx);
}

#[test]
fn automatic_fetch_updates_an_evicted_repository_without_preloading_inactive_history() {
    let fixture = Fixture::new();
    let second = Fixture::new();
    let remote = fixture._temp.path().join("remote.git");
    fixture.command(&["clone", "--bare", ".", remote.to_str().unwrap()], &[]);
    fixture.command(&["remote", "add", "origin", remote.to_str().unwrap()], &[]);
    let ctx = egui::Context::default();
    let mut app = fixture.app(&ctx);
    let expected = fixture.command(&["rev-parse", "HEAD"], &[]);
    app.open(second.root.clone(), &ctx);
    wait(&mut app, &ctx);
    app.tabs[0].evict_history();
    let generation = app.tabs[0].view.generation;
    app.settings
        .set("AutoFetchEnabled", serde_json::json!(true));
    app.auto_fetch.due_now(fixture.root.clone());
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        app.receive(&ctx);
        if app.operations.background_count() > 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "automatic local fetch did not finish"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        fixture.command(&["rev-parse", "refs/remotes/origin/main"], &[]),
        expected
    );
    assert!(app.tabs[0].refresh_again);
    assert!(!app.tabs[0].loading && app.tabs[0].history.is_none() && app.tabs[0].page.is_none());
    assert_eq!(app.tabs[0].view.generation, generation);
    assert!(app.tabs[1].page.is_some());
    app.settings
        .set("AutoFetchEnabled", serde_json::json!(false));
    app.select_tab(0, &ctx);
    wait(&mut app, &ctx);
    assert!(
        app.tabs[0]
            .history
            .as_ref()
            .unwrap()
            .snapshot
            .references
            .iter()
            .any(|r| r.name == "refs/remotes/origin/main")
    );
}

#[test]
fn closing_during_an_earlier_preferences_save_still_finishes_the_final_save() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    let mut app = workspace();
    profile(&mut app, temp.path());
    for tab in &mut app.tabs {
        tab.path = temp.path().join(&tab.path);
        fs::create_dir(&tab.path).unwrap();
    }
    let earlier_store = app.store.take().unwrap();
    app.saving = true;
    app.close(&ctx);
    assert!(app.closing && app.preferences_dirty && !app.exit_ready);
    app.sender
        .send(Event::Saved(Box::new(earlier_store), Ok(())))
        .unwrap();
    app.receive(&ctx);
    assert!(app.closing && app.saving && !app.exit_ready);
    settle_save(&mut app, &ctx);
    assert!(app.exit_ready);
    let saved = SettingsStore::new(temp.path().join("rust-profile"))
        .load()
        .unwrap();
    assert_eq!(saved.repositories(), app.tabs.iter().map(|tab| tab.path.clone()).collect::<Vec<_>>());
}

#[test]
fn shutdown_waits_for_closed_tab_history_worker_and_blocks_new_history_reads() {
    let fixture = Fixture::new();
    let ctx = egui::Context::default();
    let mut app = fixture.app(&ctx);
    profile(&mut app, fixture._temp.path());
    settle_save(&mut app, &ctx);
    let owner = app.tabs[0].id;
    let (ready, started) = mpsc::channel();
    let (release, held) = mpsc::channel();
    read_gates()
        .lock()
        .unwrap()
        .insert(fixture.root.clone(), (ready, held));
    app.refresh(&ctx);
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    app.close_tab(owner, &ctx);
    assert!(app.tabs.is_empty());
    app.close(&ctx);
    assert!(app.pending_exit && !app.exit_ready && !app.closing);
    assert_eq!(app.notice, Some(strings::text("RustShutdownReads")));
    let generation = app.retained_tabs[0].view.generation;
    // Even a stale refresh request cannot replace the tracked pending worker.
    app.refresh_owner(owner, &ctx);
    for _ in 0..3 {
        app.receive(&ctx);
    }
    assert_eq!(app.retained_tabs[0].view.generation, generation);
    assert!(app.pending_exit && !app.exit_ready);
    release.send(()).unwrap();
    settle_save(&mut app, &ctx);
    assert!(app.exit_ready);
    assert!(app.read_workers.cancel_and_drained());
    assert!(!app.retained_tabs[0].loading);
    let saved = SettingsStore::new(fixture._temp.path().join("rust-profile"))
        .load()
        .unwrap();
    assert!(saved.repositories().is_empty());
}

#[test]
fn shutdown_waits_for_invalidated_metadata_workers_after_cache_replacement() {
    let fixture = Fixture::new();
    for kind in 0..3 {
        let ctx = egui::Context::default();
        let mut app = fixture.app(&ctx);
        profile(&mut app, fixture._temp.path());
        settle_save(&mut app, &ctx);
        // Drain preceding startup workers before introducing the controlled read.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !app.read_workers.cancel_and_drained() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        }
        let snapshot = app.tabs[0].history.as_ref().unwrap().snapshot.clone();
        let (ready, started) = mpsc::channel();
        let (release, held) = mpsc::channel();
        read_gates()
            .lock()
            .unwrap()
            .insert(fixture.root.clone(), (ready, held));
        let git = app.git.clone().unwrap();
        match kind {
            0 => app.tabs[0]
                .sidebar
                .start(git, &snapshot, &ctx, &app.read_workers),
            1 => app.tabs[0]
                .remotes
                .start(git, &snapshot, &ctx, &app.read_workers),
            _ => app.tabs[0]
                .divergence
                .start(git, &snapshot, &ctx, &app.read_workers),
        }
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        // Eviction/replacement loses presentation state but must retain ownership.
        match kind {
            0 => app.tabs[0].sidebar = Default::default(),
            1 => app.tabs[0].remotes = Default::default(),
            _ => app.tabs[0].divergence = Default::default(),
        }
        app.close(&ctx);
        assert!(app.pending_exit && !app.exit_ready && !app.closing);
        assert_eq!(app.notice, Some(strings::text("RustShutdownReads")));
        release.send(()).unwrap();
        settle_save(&mut app, &ctx);
        assert!(app.exit_ready);
        assert!(app.read_workers.cancel_and_drained());
    }
}

#[test]
fn shutdown_tracks_removed_comparison_and_commit_reads_and_blocks_new_comparisons() {
    let fixture = Fixture::new();
    for editor_read in [false, true] {
        let ctx = egui::Context::default();
        let mut app = fixture.app(&ctx);
        profile(&mut app, fixture._temp.path());
        settle_save(&mut app, &ctx);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !app.read_workers.cancel_and_drained() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        }
        app.read_workers.resume();
        let git = app.git.clone().unwrap();
        let commit = app.tabs[0].page.as_ref().unwrap().entries[0].commit.clone();
        let (ready, started) = mpsc::channel();
        let (release, held) = mpsc::channel();
        read_gates()
            .lock()
            .unwrap()
            .insert(fixture.root.clone(), (ready, held));
        if editor_read {
            let mut editor = crate::commit::Editor::new(
                fixture.root.clone(),
                git.clone(),
                app.scheduler.clone(),
                shovel_core::drafts::Store {
                    directory: fixture._temp.path().join("drafts"),
                    legacy_directory: None,
                },
            );
            editor.read_workers = app.read_workers.clone();
            editor.refresh(&ctx);
            started.recv_timeout(Duration::from_secs(5)).unwrap();
            drop(editor);
        } else {
            let mut pane = crate::comparison::Pane::new(fixture.root.clone(), git.clone())
                .with_read_workers(app.read_workers.clone());
            pane.select(&commit, &ctx);
            started.recv_timeout(Duration::from_secs(5)).unwrap();
            drop(pane);
        }
        app.close(&ctx);
        assert!(app.pending_exit && !app.exit_ready);
        let mut next = crate::comparison::Pane::new(fixture.root.clone(), git)
            .with_read_workers(app.read_workers.clone());
        next.select(&commit, &ctx);
        assert!(
            !next.is_loading(),
            "new comparison must not start while shutdown drains"
        );
        release.send(()).unwrap();
        settle_save(&mut app, &ctx);
        assert!(app.exit_ready && app.read_workers.cancel_and_drained());
        // The same registry can admit real reads again after a Stay Open decision.
        app.read_workers.resume();
        next.select(&commit, &ctx);
        assert!(next.is_loading());
        let deadline = Instant::now() + Duration::from_secs(5);
        while next.is_loading() {
            next.poll(&ctx);
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        }
    }
}
