use super::*;
use eframe::App;

pub(super) fn workspace() -> Workspace {
    let (sender, receiver) = mpsc::channel();
    let tabs = ["Alpha", "Bravo", "Charlie"]
        .into_iter()
        .enumerate()
        .map(|(i, name)| Tab {
            id: i as u64 + 1,
            path: PathBuf::from(name),
            history: None,
            owner_snapshot: None,
            divergence: Default::default(),
            remotes: Default::default(),
            page: None,
            selected: None,
            previous: vec![],
            view: Default::default(),
            loading: false,
            refresh_again: false,
            locating: None,
            locate_cancel: Default::default(),
            scroll_selected: false,
            error: None,
            comparison: None,
        })
        .collect();
    Workspace {
        settings: Settings::default(),
        store: None,
        git: None,
        tabs,
        retained_tabs: vec![],
        restore_window_layout: false,
        main_geometry: Default::default(),
        selected: 1,
        next_id: 4,
        sender,
        receiver,
        notice: None,
        initialized: true,
        saving: false,
        preferences_dirty: false,
        closing: false,
        pending_exit: false,
        exit_ready: false,
        confirm_exit_without_save: false,
        activity_open: false,
        activity_all: false,
        auto_fetch: Default::default(),
        settings_dialog: None,
        updater: Default::default(),
        external_tools_opened: false,
        tab_drag: None,
        scheduler: Default::default(),
        editors: vec![],
        draft_store: None,
        operations: Default::default(),
        branches: Default::default(),
        remotes: Default::default(),
        initialization: Default::default(),
        stash_views: vec![],
        history_menu: None,
        reference_menu: None,
    }
}
pub(super) fn frame(
    ctx: &egui::Context,
    app: &mut Workspace,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900., 620.),
            )),
            events,
            ..Default::default()
        },
        |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
    )
}
fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}
#[test]
fn tab_drag_preserves_selection_restores_on_escape_and_saves_on_drop() {
    let mut app = workspace();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let result = frame(&ctx, &mut app, vec![]);
    let center = |label| {
        let (_, node) = result
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some(label))
            .unwrap();
        let r = node.bounds().unwrap();
        egui::pos2(((r.x0 + r.x1) / 2.) as f32, ((r.y0 + r.y1) / 2.) as f32)
    };
    let a = center("Alpha");
    let b = center("Bravo");
    let c = center("Charlie");
    result.drop_without_applying_deltas();
    let send =
        |app: &mut Workspace, events| frame(&ctx, app, events).drop_without_applying_deltas();
    for cancel in [true, false] {
        for events in [
            vec![egui::Event::PointerMoved(a)],
            vec![button(a, true)],
            vec![egui::Event::PointerMoved(a + egui::vec2(10., 0.))],
            vec![egui::Event::PointerMoved(b)],
            vec![egui::Event::PointerMoved(c)],
        ] {
            send(&mut app, events);
        }
        assert_eq!(app.tabs.iter().map(|t| t.id).collect::<Vec<_>>(), [2, 3, 1]);
        assert_eq!(app.tabs[app.selected].id, 2);
        if cancel {
            send(
                &mut app,
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        send(&mut app, vec![button(c, false)]);
        if cancel {
            assert_eq!(app.tabs.iter().map(|t| t.id).collect::<Vec<_>>(), [1, 2, 3]);
        } else {
            assert_eq!(
                app.settings.repositories(),
                ["Bravo", "Charlie", "Alpha"].map(PathBuf::from)
            );
        }
    }
}

#[test]
fn details_splitter_reserves_history_and_saves_drag_proportion() {
    let mut app = workspace();
    let ctx = egui::Context::default();
    let send =
        |app: &mut Workspace, events| frame(&ctx, app, events).drop_without_applying_deltas();
    send(&mut app, vec![]);
    let grip = egui::pos2(550., 442.);
    let moved = grip + egui::vec2(0., 30.);
    for events in [
        vec![egui::Event::PointerMoved(grip)],
        vec![button(grip, true)],
        vec![egui::Event::PointerMoved(moved)],
        vec![button(moved, false)],
    ] {
        send(&mut app, events);
    }
    let size = app
        .settings
        .get("DetailsHeight")
        .and_then(|v| v.as_f64())
        .unwrap();
    assert!((145.0..155.0).contains(&size), "details height {size}");
    let ratio = app
        .settings
        .get("RustDetailsRatio")
        .and_then(|v| v.as_f64())
        .unwrap();
    assert!(ratio > 0.27 && ratio < 0.34);
}

#[test]
fn reference_navigation_finds_old_tags_and_stashes_beyond_first_page() {
    use shovel_core::process::{self, Lifetime};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_owned();
    let mut git = Git::discover(None, &Cancellation::default()).unwrap();
    git.environment
        .insert("GIT_CONFIG_GLOBAL".into(), Some("NUL".into()));
    git.environment
        .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
    for key in ["GIT_AUTHOR_NAME", "GIT_COMMITTER_NAME"] {
        git.environment.insert(key.into(), Some("Fixture".into()));
    }
    for key in ["GIT_AUTHOR_EMAIL", "GIT_COMMITTER_EMAIL"] {
        git.environment
            .insert(key.into(), Some("fixture@example.invalid".into()));
    }
    let command = |args: &[&str], input: Vec<u8>| {
        let mut request = git.request(
            &root,
            args.iter().copied(),
            Lifetime::AcceptedCommand,
            1024 * 1024,
        );
        request.input = input;
        let result = process::run(&request, &Cancellation::default(), None).unwrap();
        assert!(result.success(), "{result:?}");
        String::from_utf8(result.stdout).unwrap().trim().to_owned()
    };
    command(&["init", "-b", "main"], vec![]);
    std::fs::write(root.join("file"), "base").unwrap();
    command(&["add", "file"], vec![]);
    command(
        &["-c", "commit.gpgsign=false", "commit", "-m", "base"],
        vec![],
    );
    let base = command(&["rev-parse", "HEAD"], vec![]);
    command(&["tag", "old-tag"], vec![]);
    std::fs::write(root.join("file"), "stash").unwrap();
    command(&["stash", "push", "-m", "old stash"], vec![]);
    let stash = command(&["rev-parse", "refs/stash"], vec![]);
    let mut stream = String::new();
    for n in 0..220 {
        let message = format!("commit {n}");
        stream += &format!(
            "commit refs/heads/main\nmark :{}\ncommitter Fixture <fixture@example.invalid> {} +0000\ndata {}\n{}\nfrom {}\n\n",
            n + 1,
            1800000000 + n,
            message.len(),
            message,
            if n == 0 {
                base.clone()
            } else {
                format!(":{n}")
            }
        );
    }
    command(&["fast-import", "--quiet"], stream.into_bytes());
    let history = git
        .history(
            git.inspect(&root, &Cancellation::default()).unwrap(),
            &Cancellation::default(),
        )
        .unwrap();
    let page = git
        .page(&history, &Cursor::default(), &Cancellation::default())
        .unwrap();
    let mut app = workspace();
    app.tabs.truncate(1);
    app.selected = 0;
    app.tabs[0].path = root;
    app.git = Some(Arc::new(git));
    app.tabs[0].history = Some(history);
    app.tabs[0].page = Some(page);
    app.settings
        .set("AutoFetchEnabled", serde_json::json!(false));
    let ctx = egui::Context::default();
    for hash in [base, stash] {
        app.locate(hash.clone(), &ctx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while app.tabs[0].locating.is_some() {
            assert!(std::time::Instant::now() < deadline);
            app.receive(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(app.tabs[0].selected.as_ref(), Some(&hash));
        assert!(
            app.tabs[0]
                .page
                .as_ref()
                .unwrap()
                .entries
                .iter()
                .any(|e| e.commit.hash == hash)
        );
        assert!(!app.tabs[0].previous.is_empty());
        assert!(app.tabs[0].scroll_selected);
    }
}

struct RepositoryFixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    git: Arc<Git>,
}
impl RepositoryFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        std::fs::create_dir(&root).unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        for (key, value) in [
            ("GIT_CONFIG_GLOBAL", "NUL"),
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_AUTHOR_NAME", "Fixture"),
            ("GIT_COMMITTER_NAME", "Fixture"),
            ("GIT_AUTHOR_EMAIL", "fixture@example.invalid"),
            ("GIT_COMMITTER_EMAIL", "fixture@example.invalid"),
        ] {
            git.environment.insert(key.into(), Some(value.into()));
        }
        let fixture = Self {
            _temp: temp,
            root,
            git: Arc::new(git),
        };
        fixture.command(&["init", "-b", "main"]);
        std::fs::write(fixture.root.join("file"), b"base\n").unwrap();
        fixture.command(&["add", "file"]);
        fixture.command(&["-c", "commit.gpgsign=false", "commit", "-m", "base"]);
        fixture
    }
    fn command(&self, args: &[&str]) -> Vec<u8> {
        let result = shovel_core::process::run(
            &self.git.request(
                &self.root,
                args.iter().copied(),
                shovel_core::process::Lifetime::AcceptedCommand,
                1024 * 1024,
            ),
            &Cancellation::default(),
            None,
        )
        .unwrap();
        assert!(result.success(), "{result:?}");
        result.stdout
    }
    fn workspace(&self) -> Workspace {
        let mut app = workspace();
        app.tabs.clear();
        app.git = Some(self.git.clone());
        app.settings
            .set("AutoFetchEnabled", serde_json::json!(false));
        app
    }
}
fn until(app: &mut Workspace, ctx: &egui::Context, ready: impl Fn(&Workspace) -> bool) {
    let start = std::time::Instant::now();
    while !ready(app) {
        assert!(start.elapsed() < std::time::Duration::from_secs(15));
        app.receive(ctx);
        thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn closing_and_reopening_during_initialization_keeps_owner_and_result() {
    let fixture = RepositoryFixture::new();
    let mut app = fixture.workspace();
    let ctx = egui::Context::default();
    app.open(fixture.root.clone(), &ctx);
    let id = app.tabs[0].id;
    assert!(app.tabs[0].loading);
    app.close_tab(id, &ctx);
    assert!(app.tabs.is_empty());
    assert!(app.retained_tabs[0].loading);
    app.open(fixture.root.clone(), &ctx);
    assert_eq!(app.tabs[0].id, id);
    assert!(
        !app.tabs[0].refresh_again,
        "reattachment must not schedule duplicate initialization"
    );
    app.close_tab(id, &ctx);
    until(&mut app, &ctx, |app| !app.retained_tabs[0].loading);
    assert!(app.retained_tabs[0].error.is_none());
    assert_eq!(app.retained_tabs[0].page.as_ref().unwrap().entries.len(), 1);
    app.open(fixture.root.clone(), &ctx);
    assert_eq!(app.tabs[0].id, id);
    assert!(!app.tabs[0].loading);
    assert!(app.retained_tabs.is_empty());
    assert_eq!(app.settings.repositories(), [fixture.root]);
}

#[test]
fn accepted_checkout_hook_finishes_with_no_tabs_and_reopening_observes_result() {
    use shovel_core::branches::{Action, Changes, Checkout};
    let fixture = RepositoryFixture::new();
    let mut app = fixture.workspace();
    let ctx = egui::Context::default();
    app.open(fixture.root.clone(), &ctx);
    until(&mut app, &ctx, |app| !app.tabs[0].loading);
    let id = app.tabs[0].id;
    let before_head = fixture.command(&["rev-parse", "HEAD"]);
    let before_index = fixture.command(&["ls-files", "--stage", "-z"]);
    std::fs::write(fixture.root.join(".git/hooks/post-checkout"),
        "#!/bin/sh\ntouch .git/hook-started\nwhile test ! -f .git/hook-release; do sleep .05; done\ntouch .git/hook-finished\n").unwrap();
    // Ensure an assertion cannot strand the deliberately held native command.
    struct Release(PathBuf);
    impl Drop for Release {
        fn drop(&mut self) {
            let _ = std::fs::write(&self.0, b"");
        }
    }
    let release = Release(fixture.root.join(".git/hook-release"));
    let snapshot = app.tabs[0].history.as_ref().unwrap().snapshot.clone();
    let start = snapshot
        .references
        .iter()
        .find(|r| r.name == "refs/heads/main")
        .unwrap()
        .clone();
    let review = Arc::new(
        fixture
            .git
            .working_copy(&fixture.root, &Cancellation::default())
            .unwrap(),
    );
    app.operations.submit_branch(
        "ActionTitleSwitchBranch",
        Action::Checkout {
            target: Checkout::New {
                name: "retained".into(),
                start,
            },
            review,
            changes: Changes::Keep,
            stash_message: String::new(),
        },
        &snapshot,
        fixture.git.clone(),
        &app.scheduler,
        &ctx,
    );
    until(&mut app, &ctx, |_| {
        fixture.root.join(".git/hook-started").exists()
    });
    app.close_tab(id, &ctx);
    assert!(app.tabs.is_empty() && app.scheduler.has_accepted_work());
    app.close(&ctx);
    assert!(
        !app.exit_ready && !app.closing,
        "accepted work must block shutdown"
    );
    app.prune_retained_tabs();
    assert!(app.retained_tabs.iter().any(|t| t.id == id));
    drop(release);
    until(&mut app, &ctx, |app| !app.scheduler.has_accepted_work());
    app.receive(&ctx);
    assert!(fixture.root.join(".git/hook-finished").exists());
    assert_eq!(
        fixture.command(&["symbolic-ref", "--short", "HEAD"]),
        b"retained\n"
    );
    assert_eq!(fixture.command(&["rev-parse", "HEAD"]), before_head);
    assert_eq!(
        fixture.command(&["ls-files", "--stage", "-z"]),
        before_index
    );
    assert_eq!(std::fs::read(fixture.root.join("file")).unwrap(), b"base\n");
    assert!(app.retained_tabs[0].refresh_again);
    app.open(fixture.root.clone(), &ctx);
    until(&mut app, &ctx, |app| !app.tabs[0].loading);
    assert_eq!(app.tabs[0].id, id);
    assert_eq!(
        app.tabs[0]
            .history
            .as_ref()
            .unwrap()
            .snapshot
            .branch
            .as_deref(),
        Some("retained")
    );
}

#[test]
fn idle_retirement_never_evicts_initializing_owner_and_keeps_recent_three() {
    let mut app = workspace();
    let ctx = egui::Context::default();
    app.tabs[0].loading = true;
    for name in ["Delta", "Echo", "Foxtrot", "Golf"] {
        app.open(name.into(), &ctx);
    }
    let ids = app.tabs.iter().map(|t| t.id).collect::<Vec<_>>();
    for id in ids {
        app.close_tab(id, &ctx);
    }
    assert_eq!(app.retained_tabs.len(), 4);
    assert_eq!(app.retained_tabs[0].id, 1);
    app.sender
        .send(Event::Failed(
            1,
            0,
            history_view::Failure {
                message: "fixture failure".into(),
                identity: false,
            },
        ))
        .unwrap();
    app.receive(&ctx);
    assert_eq!(app.retained_tabs.len(), 3);
    assert!(!app.retained_tabs.iter().any(|t| t.id == 1));
}

#[test]
fn middle_close_and_keyboard_selection_preserve_tab_owner_and_saved_order() {
    let mut app = workspace();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let output = frame(&ctx, &mut app, vec![]);
    let node = &output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some("Bravo"))
        .unwrap()
        .1;
    let rect = node.bounds().unwrap();
    let pos = egui::pos2(
        (rect.x0 + rect.x1) as f32 / 2.,
        (rect.y0 + rect.y1) as f32 / 2.,
    );
    output.drop_without_applying_deltas();
    for events in [
        vec![egui::Event::PointerMoved(pos)],
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Middle,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Middle,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    ] {
        frame(&ctx, &mut app, events).drop_without_applying_deltas();
    }
    assert_eq!(app.tabs.iter().map(|t| t.id).collect::<Vec<_>>(), [1, 3]);
    assert_eq!(app.tabs[app.selected].id, 3);
    assert_eq!(app.retained_tabs[0].id, 2);
    // Egui 0.37 tracks held modifiers through its explicit modifier event.
    let modifiers = egui::Modifiers {
        ctrl: true,
        ..Default::default()
    };
    ctx.run_ui(
        egui::RawInput {
            events: vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
            ],
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900., 620.),
            )),
            ..Default::default()
        },
        |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
    )
    .drop_without_applying_deltas();
    assert_eq!(app.tabs[app.selected].id, 1);
    app.tab_action(workspace_tabs::TabAction::Move(1, 1), &ctx);
    assert_eq!(app.tabs[app.selected].id, 1);
    assert_eq!(
        app.settings.repositories(),
        ["Charlie", "Alpha"].map(PathBuf::from)
    );
    app.tab_action(workspace_tabs::TabAction::CloseOthers(1), &ctx);
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.tabs[0].id, 1);
}

fn sidebar_point(
    ctx: &egui::Context,
    app: &mut Workspace,
    label: &str,
) -> Option<(egui::Pos2, bool)> {
    let out = frame(ctx, app, vec![]);
    let value = out
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter(|(_, n)| {
            n.label() == Some(label) && n.bounds().is_some() && n.role() != egui::Role::Window
        })
        // Toolbar and sidebar actions may share the same localized label.
        .max_by(|(_, a), (_, b)| a.bounds().unwrap().y0.total_cmp(&b.bounds().unwrap().y0))
        .map(|(_, n)| {
            let r = n.bounds().unwrap();
            (
                egui::pos2(((r.x0 + r.x1) / 2.) as f32, ((r.y0 + r.y1) / 2.) as f32),
                n.is_disabled(),
            )
        });
    out.drop_without_applying_deltas();
    value
}
fn sidebar_click(
    ctx: &egui::Context,
    app: &mut Workspace,
    label: &str,
    button: egui::PointerButton,
) {
    frame(ctx, app, vec![]).drop_without_applying_deltas();
    let (p, disabled) = sidebar_point(ctx, app, label).unwrap_or_else(|| panic!("Missing {label}"));
    assert!(!disabled, "Disabled {label}");
    frame(ctx, app, vec![egui::Event::PointerMoved(p)]).drop_without_applying_deltas();
    for pressed in [true, false] {
        frame(
            ctx,
            app,
            vec![egui::Event::PointerButton {
                pos: p,
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        )
        .drop_without_applying_deltas();
    }
}
#[test]
fn sidebar_remote_fetch_uses_clicked_target_without_changing_toolbar_flags() {
    use crate::history_view::tests::{Fixture, wait};
    use shovel_core::scheduler::{Lane, Outcome};
    use std::time::{Duration, Instant};
    let f = Fixture::new();
    let root = f.root.to_str().unwrap();
    let mut tips = vec![];
    for name in ["origin", "other"] {
        let path = f._temp.path().join(format!("{name}.git"));
        let path = path.to_str().unwrap();
        f.command(&["init", "--bare", path], &[]);
        f.command(&["remote", "add", name, path], &[]);
        f.command(&["push", name, "main"], &[]);
        let tip = f.command(
            &[
                "--git-dir",
                path,
                "commit-tree",
                "refs/heads/main^{tree}",
                "-p",
                "refs/heads/main",
            ],
            name.as_bytes(),
        );
        f.command(
            &["--git-dir", path, "update-ref", "refs/heads/main", &tip],
            &[],
        );
        tips.push(tip);
    }
    let old = f.command(&["rev-parse", "HEAD"], &[]);
    let before_config = f.command(&["config", "--local", "--list"], &[]);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = f.app(&ctx);
    app.operations.set_authentication_helper(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/ShovelRustAskPass.exe"),
    );
    let snapshot = app.tabs[0].history.as_ref().unwrap().snapshot.clone();
    let prefs = serde_json::json!([{"Path":snapshot.worktree.to_string_lossy(),"Action":2,"Branch":"main",
        "SourceBranch":"main","Remote":"other","RemoteBranch":"main","Strategy":0,"AllRemotes":true,"AllTags":false,
        "SetUpstream":false,"AutoStash":false,"ExplicitTarget":true}]);
    app.settings.set("NetworkActions", prefs.clone());
    let common = snapshot.common_directory.to_string_lossy().to_uppercase();
    let (held, _) = app.scheduler.reserve(
        snapshot.identity.fingerprint.clone(),
        common.clone(),
        "held".into(),
        Lane::Exclusive,
        |_| {},
    );
    for _ in 0..2 {
        sidebar_click(&ctx, &mut app, "origin", egui::PointerButton::Secondary);
        sidebar_click(
            &ctx,
            &mut app,
            &strings::text("ActionTitleFetch"),
            egui::PointerButton::Primary,
        );
    }
    assert_eq!(
        app.scheduler
            .records()
            .iter()
            .filter(|r| r.outcome.is_none())
            .count(),
        2,
        "duplicate same-remote request must coalesce"
    );
    assert!(
        app.scheduler
            .records()
            .iter()
            .any(|r| r.outcome.is_none() && r.key == "network:Fetch:origin"),
        "context menu must submit the explicit remote, not the toolbar action"
    );
    assert_eq!(app.settings.get("NetworkActions"), Some(&prefs));
    let owner = app.tabs[0].id;
    app.close_tab(owner, &ctx);
    assert!(app.scheduler.remove_waiting(held));
    let deadline = Instant::now() + Duration::from_secs(20);
    while app.scheduler.has_accepted_work() || app.operations.has_pending(&snapshot.worktree) {
        app.receive(&ctx);
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    app.receive(&ctx);
    assert_eq!(
        f.command(&["rev-parse", "refs/remotes/origin/main"], &[]),
        tips[0]
    );
    assert_eq!(
        f.command(&["rev-parse", "refs/remotes/other/main"], &[]),
        old
    );
    assert_eq!(f.command(&["rev-parse", "HEAD"], &[]), old);
    assert_eq!(
        f.command(&["config", "--local", "--list"], &[]),
        before_config
    );
    assert!(
        app.scheduler
            .records()
            .iter()
            .any(|r| r.outcome == Some(Outcome::Succeeded))
    );
    app.open(PathBuf::from(root), &ctx);
    wait(&mut app, &ctx);
    let snapshot = app.tabs[0].history.as_ref().unwrap().snapshot.clone();
    let (held, _) = app.scheduler.reserve(
        snapshot.identity.fingerprint.clone(),
        common,
        "held-again".into(),
        Lane::Exclusive,
        |_| {},
    );
    for name in ["origin", "other"] {
        sidebar_click(&ctx, &mut app, name, egui::PointerButton::Secondary);
        sidebar_click(
            &ctx,
            &mut app,
            &strings::text("ActionTitleFetch"),
            egui::PointerButton::Primary,
        );
    }
    assert_eq!(
        app.scheduler
            .records()
            .iter()
            .filter(|r| r.outcome.is_none())
            .count(),
        3,
        "different targets must remain separate requests"
    );
    assert!(app.scheduler.remove_waiting(held));
    let deadline = Instant::now() + Duration::from_secs(20);
    while app.scheduler.has_accepted_work() || app.operations.has_pending(&snapshot.worktree) {
        app.receive(&ctx);
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    app.receive(&ctx);
    wait(&mut app, &ctx);
    assert_eq!(
        f.command(&["rev-parse", "refs/remotes/other/main"], &[]),
        tips[1]
    );
    assert_eq!(f.command(&["rev-parse", "HEAD"], &[]), old);
    assert_eq!(app.settings.get("NetworkActions"), Some(&prefs));
}
#[test]
fn sidebar_enter_and_double_click_open_review_then_switch_the_captured_branch() {
    use crate::history_view::tests::{Fixture, wait};
    use std::time::{Duration, Instant};
    let f = Fixture::new();
    let tip = f.command(&["rev-parse", "HEAD~350"], &[]);
    f.command(&["branch", "target", &tip], &[]);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    ctx.set_embed_viewports(true);
    let mut app = f.app(&ctx);
    sidebar_click(&ctx, &mut app, "target", egui::PointerButton::Primary);
    wait(&mut app, &ctx);
    assert_eq!(app.tabs[0].selected.as_deref(), Some(tip.as_str()));
    assert!(!app.branches.is_open());
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    )
    .drop_without_applying_deltas();
    assert!(
        app.branches.is_open(),
        "Enter should open the reviewed checkout"
    );
    assert_eq!(
        f.command(&["symbolic-ref", "--short", "HEAD"], &[]),
        "main",
        "opening review cannot switch"
    );
    for expected in ["target", "main"] {
        if expected == "main" {
            // Native Git elapsed time does not advance egui's synthetic frame
            // clock. Separate this gesture from the earlier confirmation click.
            let time = ctx.input(|i| i.time) + 0.5;
            ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 620.),
                    )),
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
            )
            .drop_without_applying_deltas();
            sidebar_click(&ctx, &mut app, "main", egui::PointerButton::Primary);
            sidebar_click(&ctx, &mut app, "main", egui::PointerButton::Primary);
            assert!(
                app.branches.is_open(),
                "double-click should open the reviewed checkout"
            );
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        while sidebar_point(&ctx, &mut app, &strings::text("ActionTitleSwitchBranch"))
            .is_none_or(|(_, disabled)| disabled)
        {
            app.receive(&ctx);
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        sidebar_click(
            &ctx,
            &mut app,
            &strings::text("ActionTitleSwitchBranch"),
            egui::PointerButton::Primary,
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        while app.scheduler.has_accepted_work() {
            app.receive(&ctx);
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        app.receive(&ctx);
        wait(&mut app, &ctx);
        assert_eq!(
            f.command(&["symbolic-ref", "--short", "HEAD"], &[]),
            expected
        );
    }
}
