use super::*;
use eframe::App;

pub(super) fn workspace() -> Workspace {
    let (sender, receiver) = mpsc::channel();
    let tabs = ["Alpha", "Bravo", "Charlie"]
        .into_iter()
        .enumerate()
        .map(|(i, name)| Tab {
            qualified_name: None,
            id: i as u64 + 1,
            path: PathBuf::from(name),
            local_changes: false,
            history: None,
            owner_snapshot: None,
            divergence: Default::default(),
            remotes: Default::default(),
            sidebar: Default::default(),
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
        git_selection: Default::default(),
        repository_picker: None,
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
        discard_settings_on_exit: false,
        read_workers: Default::default(),
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
        repository_dialog: None,
        remotes: Default::default(),
        initialization: Default::default(),
        stash_views: vec![],
        history_menu: None,
        reference_menu: None,
        deferred_history: Default::default(),
        tab_markers: Default::default(),
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
        let below = egui::pos2(c.x, 400.);
        for events in [
            vec![egui::Event::PointerMoved(a)],
            vec![button(a, true)],
            vec![egui::Event::PointerMoved(a + egui::vec2(10., 0.))],
            vec![egui::Event::PointerMoved(b)],
            vec![egui::Event::PointerMoved(below)],
        ] {
            send(&mut app, events);
        }
        let preview = frame(&ctx, &mut app, vec![]);
        let floating_label = |output: &egui::FullOutput| {
            output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "Alpha" && t.pos.y > below.y))
        };
        assert!(
            floating_label(&preview),
            "the drag label follows the pointer below the strip"
        );
        assert!(preview.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.rect.contains(c) && r.rect.width() > (c.x - b.x) * 0.9 && r.stroke.width == 2.)));
        preview.drop_without_applying_deltas();
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
        let finished = frame(&ctx, &mut app, vec![button(below, false)]);
        assert!(!floating_label(&finished));
        finished.drop_without_applying_deltas();
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
fn restoring_twenty_tabs_keeps_the_last_selected_repository_and_loads_only_its_history() {
    let fixture = RepositoryFixture::new();
    let profile = fixture._temp.path().join("profile");
    let mut store = SettingsStore::new(profile.clone());
    let mut settings = store.load().unwrap();
    let candidates: Vec<_> = (0..23)
        .map(|i| fixture._temp.path().join(format!("other-{i}")))
        .collect();
    let mut saved = vec![
        String::from("relative"),
        candidates[0].to_string_lossy().replace('\\', "/"),
    ];
    saved.extend(candidates.iter().map(|p| p.to_string_lossy().into_owned()));
    saved.push(fixture.root.to_string_lossy().into_owned());
    settings.set("RepositoryPaths", serde_json::json!(saved));
    settings.set(
        "SelectedRepositoryPath",
        serde_json::json!(fixture.root.to_string_lossy().to_uppercase()),
    );
    settings.set("AutoFetchEnabled", serde_json::json!(false));
    store.save(&settings).unwrap();
    let mut store = SettingsStore::new(profile);
    let restored = store.load().unwrap();
    let paths = restored.repositories();
    assert_eq!(paths.len(), 20);
    assert!(
        paths[..19]
            .iter()
            .zip(&candidates[..19])
            .all(|(a, b)| Workspace::same_path(a, b))
    );
    assert_eq!(paths[19], fixture.root);
    let ctx = egui::Context::default();
    let mut app = workspace();
    app.tabs.clear();
    app.sender
        .send(Event::Initialized(
            Box::new(restored),
            Box::new(store),
            Ok((*fixture.git).clone()),
            None,
        ))
        .unwrap();
    app.receive(&ctx);
    assert_eq!(app.selected, 19);
    assert!(
        app.tabs[..19]
            .iter()
            .all(|t| !t.loading && t.history.is_none() && t.error.is_none())
    );
    until(&mut app, &ctx, |a| !a.tabs[19].loading);
    assert_eq!(
        app.tabs[19].history.as_ref().unwrap().snapshot.worktree,
        fixture.root
    );
    assert!(
        app.tabs[..19]
            .iter()
            .all(|t| t.page.is_none() && t.history.is_none() && t.error.is_none())
    );
}

#[test]
fn duplicate_tab_names_use_the_shortest_distinct_path_and_keep_equal_slots() {
    let mut app = workspace();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.tabs.clear();
    let paths = [
        r"C:\projects\client\tools\Shovel",
        r"D:\projects\server\tools\shovel",
        r"C:\games\Aurora",
        r"Z:\",
    ];
    for path in paths {
        app.ensure_tab(PathBuf::from(path));
    }
    let names = [
        r"client\tools\Shovel",
        r"server\tools\shovel",
        "Aurora",
        r"Z:\",
    ];
    assert_eq!(app.tabs.iter().map(Tab::name).collect::<Vec<_>>(), names);
    let out = frame(&ctx, &mut app, vec![]);
    let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
    let mut widths = Vec::new();
    for (name, path) in names.into_iter().zip(paths) {
        let node = nodes
            .iter()
            .map(|(_, node)| node)
            .find(|node| node.label() == Some(name) && node.description() == Some(path))
            .unwrap();
        let rect = node.bounds().unwrap();
        widths.push(rect.x1 - rect.x0);
    }
    assert!(widths.iter().all(|w| (*w - widths[0]).abs() < 1.1));
    out.drop_without_applying_deltas();
    app.close_tab(app.tabs[1].id, &ctx);
    assert_eq!(app.tabs[0].name(), "Shovel");
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
fn inactive_tab_menu_targets_its_own_repository_without_activating_it() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = workspace();
    app.tabs.truncate(2);
    app.selected = 0;
    app.tabs[0].path = temp.path().join("active");
    app.tabs[1].path = temp.path().join("inactive");
    std::fs::create_dir(&app.tabs[0].path).unwrap();
    app.settings.set("BashExecutable", serde_json::json!(temp.path().join("absent-bash.exe")));
    sidebar_click(&ctx, &mut app, "inactive", egui::PointerButton::Secondary);
    assert!(sidebar_point(&ctx, &mut app, &strings::text("RepositoryOpenExplorer"))
        .is_some_and(|(_, disabled)| !disabled));
    sidebar_click(&ctx, &mut app, &strings::text("RepositoryOpenConsole"), egui::PointerButton::Primary);
    until(&mut app, &ctx, |app| app.notice.is_some());
    assert_eq!(app.notice.as_deref(), Some(strings::text("RepositoryUnavailable").as_str()));
    assert_eq!(app.selected, 0);
}

#[test]
fn reference_navigation_finds_old_tags_and_inspects_stashes_from_the_sidebar() {
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
    ctx.enable_accesskit();
    ctx.set_embed_viewports(true);
    app.locate(base.clone(), &ctx);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.tabs[0].locating.is_some() {
        assert!(std::time::Instant::now() < deadline);
        app.receive(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(app.tabs[0].selected.as_ref(), Some(&base));
    assert!(!app.tabs[0].previous.is_empty());
    assert!(app.tabs[0].scroll_selected);
    let entries = &app.tabs[0].page.as_ref().unwrap().entries;
    let leaf = entries.iter().position(|entry| entry.commit.hash == stash).unwrap();
    assert!(entries[leaf].graph.is_stash());
    assert_eq!(entries[leaf + 1].commit.hash, base);
    app.first_page(&ctx);
    crate::history_view::tests::wait(&mut app, &ctx);
    assert!(!app.tabs[0].page.as_ref().unwrap().entries.iter().any(|entry| entry.commit.hash == stash));
    app.settings.set("CommitInWorkspace", serde_json::json!(true));
    app.draft_store = Some(shovel_core::drafts::Store { directory: temp.path().join("drafts"), legacy_directory: None });
    app.open_commit();
    assert!(app.tabs[0].local_changes);
    let label = "stash@{0}  On main: old stash";
    sidebar_click(&ctx, &mut app, label, egui::PointerButton::Primary);
    crate::history_view::tests::wait(&mut app, &ctx);
    assert_eq!(app.tabs[0].selected.as_ref(), Some(&stash));
    assert!(!app.tabs[0].local_changes);
    assert!(app.stash_views.is_empty());
    let (point, _) = sidebar_point(&ctx, &mut app, label).unwrap();
    // A single sidebar click leaves the stash row focused for keyboard inspection.
    assert!(point.y > 0.);
    frame(&ctx, &mut app, vec![egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: Some(egui::Key::Enter),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]).drop_without_applying_deltas();
    assert!(app.stash_views.iter().any(|view| view.open));
    assert_eq!(app.tabs[0].selected.as_ref(), Some(&stash));
}

struct RepositoryFixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    git: Arc<Git>,
}

#[test]
fn sidebar_stash_with_detached_base_opens_inspector_without_changing_history_selection() {
    let fixture = RepositoryFixture::new();
    fixture.command(&["switch", "--detach", "HEAD"]);
    fixture.command(&["commit", "--allow-empty", "-m", "detached base"]);
    std::fs::write(fixture.root.join("file"), b"detached changes\n").unwrap();
    fixture.command(&["stash", "push", "-m", "detached saved"]);
    fixture.command(&["switch", "main"]);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    ctx.set_embed_viewports(true);
    let mut app = fixture.workspace();
    app.open(fixture.root.clone(), &ctx);
    crate::history_view::tests::wait(&mut app, &ctx);
    let selected = app.tabs[0].selected.clone();
    let output = frame(&ctx, &mut app, vec![]);
    let label = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find_map(|(_, node)| {
            node.label()
                .filter(|label| label.starts_with("stash@{0}") && label.contains("detached saved"))
        })
        .unwrap()
        .to_owned();
    output.drop_without_applying_deltas();
    sidebar_click(&ctx, &mut app, &label, egui::PointerButton::Primary);
    crate::history_view::tests::wait(&mut app, &ctx);
    assert!(
        app.stash_views
            .iter()
            .any(|view| view.open && view.path == fixture.root)
    );
    assert_eq!(app.tabs[0].selected, selected);
    assert!(app.tabs[0].error.is_none());
    assert!(
        app.tabs[0]
            .page
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .all(|entry| !entry.graph.is_stash())
    );
}

#[test]
fn history_stash_menu_applies_pops_and_confirms_drop_for_the_clicked_stash() {
    use crate::history_view::tests::wait;
    let fixture = RepositoryFixture::new();
    let file = fixture.root.join("file");
    let output = |args: &[&str]| {
        String::from_utf8(fixture.command(args))
            .unwrap()
            .trim_end()
            .to_owned()
    };
    std::fs::write(&file, b"older stash\n").unwrap();
    fixture.command(&["stash", "push", "-m", "older stash"]);
    let older = output(&["rev-parse", "refs/stash"]);
    std::fs::write(&file, b"newer stash\n").unwrap();
    fixture.command(&["stash", "push", "-m", "newer stash"]);
    let newer = output(&["rev-parse", "refs/stash"]);

    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = fixture.workspace();
    app.open(fixture.root.clone(), &ctx);
    wait(&mut app, &ctx);
    let open_menu = |app: &mut Workspace, hash: &str, selector: &str| {
        let out = frame(&ctx, app, vec![]);
        let row = out
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|(_, node)| {
                node.label()
                    .filter(|label| node.role() == egui::Role::Button && label.contains(hash))
            })
            .unwrap()
            .to_owned();
        out.drop_without_applying_deltas();
        sidebar_click(&ctx, app, &row, egui::PointerButton::Secondary);
        let menu = app.history_menu.as_ref().unwrap();
        assert_eq!(menu.commit.hash, hash);
        assert_eq!(menu.stash.as_ref().unwrap().name, selector);
        for action in [
            "ActionTitleStashApply",
            "ActionTitleStashPop",
            "ActionTitleStashDrop",
        ] {
            assert!(
                sidebar_point(&ctx, app, &strings::text(action))
                    .is_some_and(|(_, disabled)| !disabled)
            );
        }
        assert!(sidebar_point(&ctx, app, &strings::text("HistoryCopySha")).is_none());
    };
    let finish = |app: &mut Workspace| {
        until(app, &ctx, |app| {
            !app.scheduler.has_accepted_work() && !app.operations.has_pending(&fixture.root)
        });
        app.receive(&ctx);
        wait(app, &ctx);
    };

    open_menu(&mut app, &older, "stash@{1}");
    sidebar_click(
        &ctx,
        &mut app,
        &strings::text("ActionTitleStashApply"),
        egui::PointerButton::Primary,
    );
    finish(&mut app);
    assert_eq!(std::fs::read(&file).unwrap(), b"older stash\n");
    assert_eq!(
        output(&["stash", "list", "--format=%H"]),
        format!("{newer}\n{older}")
    );

    fixture.command(&["restore", "--", "file"]);
    open_menu(&mut app, &older, "stash@{1}");
    sidebar_click(
        &ctx,
        &mut app,
        &strings::text("ActionTitleStashPop"),
        egui::PointerButton::Primary,
    );
    finish(&mut app);
    assert_eq!(std::fs::read(&file).unwrap(), b"older stash\n");
    assert_eq!(output(&["stash", "list", "--format=%H"]), newer);

    open_menu(&mut app, &newer, "stash@{0}");
    sidebar_click(
        &ctx,
        &mut app,
        &strings::text("ActionTitleStashDrop"),
        egui::PointerButton::Primary,
    );
    assert!(app.branches.is_open());
    assert_eq!(output(&["stash", "list", "--format=%H"]), newer);
    sidebar_click(
        &ctx,
        &mut app,
        &strings::text("RustConfirmStashDrop"),
        egui::PointerButton::Primary,
    );
    finish(&mut app);
    assert!(output(&["stash", "list", "--format=%H"]).is_empty());
    assert_eq!(std::fs::read(&file).unwrap(), b"older stash\n");
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

pub(super) fn sidebar_point(
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
pub(super) fn sidebar_click(
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
fn conflict_banner_opens_the_configured_editor_and_footer_continues_the_verified_merge() {
    use shovel_core::process::{self, Lifetime};
    for inline in [true, false] {
        let fixture = RepositoryFixture::new();
        let file = fixture.root.join("file");
        fixture.command(&["switch", "-c", "incoming"]);
        std::fs::write(&file, b"incoming\n").unwrap();
        fixture.command(&["commit", "-am", "incoming"]);
        fixture.command(&["switch", "main"]);
        std::fs::write(&file, b"current\n").unwrap();
        fixture.command(&["commit", "-am", "current"]);
        let merge = fixture.git.request(
            &fixture.root,
            ["merge", "--no-edit", "incoming"],
            Lifetime::AcceptedCommand,
            65536,
        );
        assert!(
            !process::run(&merge, &Cancellation::default(), None)
                .unwrap()
                .success()
        );
        let original = fixture.command(&["rev-parse", "HEAD"]);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.set_embed_viewports(true);
        let mut app = fixture.workspace();
        app.settings
            .set("CommitInWorkspace", serde_json::json!(inline));
        app.draft_store = Some(shovel_core::drafts::Store {
            directory: fixture._temp.path().join("drafts"),
            legacy_directory: None,
        });
        app.open(fixture.root.clone(), &ctx);
        until(&mut app, &ctx, |app| {
            app.tab_markers.verified_conflicts(&app.tabs[0]) == Some(true)
        });
        let continue_label = recovery_ui::continue_label(shovel_core::recovery::Kind::Merge);
        assert!(sidebar_point(&ctx, &mut app, &continue_label).is_none());
        assert!(sidebar_point(&ctx, &mut app, &strings::text("ActionTitleSkip")).is_none());
        let resolve_label = strings::text("ConflictResolve");
        let (resolve, disabled) = sidebar_point(&ctx, &mut app, &resolve_label).unwrap();
        assert!(resolve.x > 250. && !disabled);
        sidebar_click(&ctx, &mut app, &resolve_label, egui::PointerButton::Primary);
        assert_eq!(app.tabs[0].local_changes, inline);
        assert!(
            app.editors
                .iter()
                .any(|editor| editor.open && editor.inline == inline)
        );
        frame(&ctx, &mut app, vec![]).drop_without_applying_deltas();
        until(&mut app, &ctx, |app| {
            app.editors[0].current_context().is_some()
        });
        assert!(sidebar_point(&ctx, &mut app, &continue_label).is_none());
        assert!(fixture.root.join(".git/MERGE_HEAD").exists());

        fixture.command(&["checkout", "--ours", "--", "file"]);
        fixture.command(&["add", "file"]);
        app.editors[0].request_refresh(&ctx);
        until(&mut app, &ctx, |app| {
            app.editors[0]
                .current_context()
                .is_some_and(|c| c.group(shovel_core::working::Group::Conflicted).is_empty())
        });
        if inline {
            sidebar_click(
                &ctx,
                &mut app,
                &continue_label,
                egui::PointerButton::Primary,
            );
        } else {
            frame(
                &ctx,
                &mut app,
                vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: Some(egui::Key::Enter),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                }],
            )
            .drop_without_applying_deltas();
        }
        until(&mut app, &ctx, |app| {
            !app.scheduler.has_accepted_work() && !app.operations.has_pending(&fixture.root)
        });
        assert!(
            !fixture.root.join(".git/MERGE_HEAD").exists(),
            "inline={inline}, scheduler={:?}",
            app.scheduler.records()
        );
        assert_ne!(fixture.command(&["rev-parse", "HEAD"]), original);
        assert_eq!(std::fs::read(&file).unwrap(), b"current\n");
    }
}

#[test]
fn undo_last_commit_requires_review_and_keeps_the_index_and_worktree() {
    let fixture = RepositoryFixture::new();
    let file = fixture.root.join("file");
    let base = fixture.command(&["rev-parse", "HEAD"]);
    std::fs::write(&file, b"committed\n").unwrap();
    fixture.command(&["commit", "-am", "last commit"]);
    let tip = fixture.command(&["rev-parse", "HEAD"]);
    std::fs::write(&file, b"staged\n").unwrap();
    fixture.command(&["add", "file"]);
    std::fs::write(&file, b"worktree\n").unwrap();
    let index = std::fs::read(fixture.root.join(".git/index")).unwrap();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = fixture.workspace();
    app.open(fixture.root.clone(), &ctx);
    crate::history_view::tests::wait(&mut app, &ctx);
    let open = |app: &mut Workspace| {
        sidebar_click(
            &ctx,
            app,
            &strings::text("RepositoryMenu"),
            egui::PointerButton::Primary,
        );
        let output = frame(&ctx, app, vec![]);
        let node_y = |label: &str| {
            output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    node.label()
                        .filter(|name| name.starts_with(label))
                        .and_then(|_| node.bounds())
                        .map(|bounds| bounds.y0)
                })
        };
        assert!(
            node_y(&strings::text("HistoryUndoLastCommit"))
                .zip(node_y(&strings::text("RefreshRepository")))
                .is_some_and(|(undo, refresh)| undo > refresh)
        );
        output.drop_without_applying_deltas();
        sidebar_click(
            &ctx,
            app,
            &strings::text("HistoryUndoLastCommit"),
            egui::PointerButton::Primary,
        );
        assert!(app.branches.is_open());
    };
    open(&mut app);
    assert!(!app.scheduler.has_accepted_work());
    assert_eq!(fixture.command(&["rev-parse", "HEAD"]), tip);
    sidebar_click(
        &ctx,
        &mut app,
        &strings::text("CancelOperation"),
        egui::PointerButton::Primary,
    );
    assert!(!app.branches.is_open());
    open(&mut app);
    let submit = strings::text("HistoryUndoSubmit");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while !sidebar_point(&ctx, &mut app, &submit).is_some_and(|(_, disabled)| !disabled) {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(fixture.command(&["rev-parse", "HEAD"]), tip);
    sidebar_click(&ctx, &mut app, &submit, egui::PointerButton::Primary);
    until(&mut app, &ctx, |app| {
        !app.scheduler.has_accepted_work() && !app.operations.has_pending(&fixture.root)
    });
    assert_eq!(fixture.command(&["rev-parse", "HEAD"]), base);
    assert_eq!(
        std::fs::read(fixture.root.join(".git/index")).unwrap(),
        index
    );
    assert_eq!(std::fs::read(file).unwrap(), b"worktree\n");
}

#[test]
fn history_cherry_pick_opens_review_and_can_apply_without_committing() {
    let fixture = RepositoryFixture::new();
    let head = fixture.command(&["rev-parse", "HEAD"]);
    fixture.command(&["switch", "-c", "source"]);
    std::fs::write(fixture.root.join("picked"), b"picked\n").unwrap();
    fixture.command(&["add", "picked"]);
    fixture.command(&["commit", "-m", "source change"]);
    let source = String::from_utf8(fixture.command(&["rev-parse", "HEAD"])).unwrap();
    fixture.command(&["switch", "main"]);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = fixture.workspace();
    app.open(fixture.root.clone(), &ctx);
    crate::history_view::tests::wait(&mut app, &ctx);
    let output = frame(&ctx, &mut app, vec![]);
    let commit_row = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find_map(|(_, node)| node.label().filter(|label| label.contains(source.trim())))
        .unwrap()
        .to_owned();
    output.drop_without_applying_deltas();
    sidebar_click(&ctx, &mut app, &commit_row, egui::PointerButton::Secondary);
    let cherry_pick = strings::text("HistoryCherryPick");
    sidebar_click(&ctx, &mut app, &cherry_pick, egui::PointerButton::Primary);
    assert!(app.branches.is_open());
    assert!(!app.scheduler.has_accepted_work());
    assert!(!fixture.root.join("picked").exists());
    sidebar_click(
        &ctx,
        &mut app,
        &strings::text("CherryPickCommitChanges"),
        egui::PointerButton::Primary,
    );
    assert!(
        sidebar_point(&ctx, &mut app, &strings::text("CherryPickAppendOrigin"))
            .is_some_and(|(_, disabled)| disabled)
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while !sidebar_point(&ctx, &mut app, &cherry_pick).is_some_and(|(_, disabled)| !disabled) {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(fixture.command(&["rev-parse", "HEAD"]), head);
    assert!(!fixture.root.join("picked").exists());
    sidebar_click(&ctx, &mut app, &cherry_pick, egui::PointerButton::Primary);
    until(&mut app, &ctx, |app| {
        !app.scheduler.has_accepted_work() && !app.operations.has_pending(&fixture.root)
    });
    assert_eq!(fixture.command(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.command(&["show", ":picked"]), b"picked\n");
}

#[test]
fn commit_layout_setting_switches_sidebar_view_and_preserves_the_commit_editor() {
    let repo = RepositoryFixture::new();
    let other = RepositoryFixture::new();
    std::fs::write(repo.root.join("file"), b"updated\n").unwrap();
    repo.command(&["add", "file"]);
    let mut app = repo.workspace();
    let profile = repo._temp.path().join("profile");
    let mut store = SettingsStore::new(profile.clone());
    app.settings = store.load().unwrap();
    app.settings
        .set("AutoFetchEnabled", serde_json::json!(false));
    app.store = Some(store);
    app.draft_store = Some(shovel_core::drafts::Store {
        directory: profile.join("CommitDrafts"),
        legacy_directory: None,
    });
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    ctx.set_embed_viewports(true);
    app.open(repo.root.clone(), &ctx);
    until(&mut app, &ctx, |app| {
        app.tabs[0].history.is_some() && !app.saving && !app.preferences_dirty
    });

    let find = |app: &mut Workspace, test: &dyn Fn(&egui::accesskit::Node) -> bool| {
        let output = frame(&ctx, app, vec![]);
        let point = output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, node)| {
                (test(node) && !node.is_disabled())
                    .then(|| node.bounds())
                    .flatten()
            })
            .map(|r| egui::pos2(((r.x0 + r.x1) / 2.) as f32, ((r.y0 + r.y1) / 2.) as f32))
            .next();
        output.drop_without_applying_deltas();
        point
    };
    let toolbar_commit = |node: &egui::accesskit::Node| {
        node.label() == Some("Commit") && node.bounds().is_some_and(|r| r.y0 < 100.)
    };
    let click_at = |app: &mut Workspace, pos| {
        frame(&ctx, app, vec![egui::Event::PointerMoved(pos)]).drop_without_applying_deltas();
        for pressed in [true, false] {
            frame(&ctx, app, vec![button(pos, pressed)]).drop_without_applying_deltas();
        }
    };
    let save_layout = |app: &mut Workspace, key: &str| {
        until(app, &ctx, |app| !app.saving && !app.preferences_dirty);
        app.settings_dialog = Some(settings_ui::Dialog::new(&app.settings));
        for key in [key, "SettingsSave"] {
            sidebar_click(&ctx, app, &strings::text(key), egui::PointerButton::Primary);
        }
        until(app, &ctx, |app| {
            app.settings_dialog.is_none() && !app.saving
        });
    };
    assert!(find(&mut app, &toolbar_commit).is_some());
    assert!(sidebar_point(&ctx, &mut app, "Local Changes").is_none());
    save_layout(&mut app, "SettingsCommitSidebar");
    assert!(
        SettingsStore::new(profile.clone())
            .load()
            .unwrap()
            .commit_in_workspace()
    );
    assert!(find(&mut app, &toolbar_commit).is_none());

    let local_changes = |node: &egui::accesskit::Node| {
        node.label()
            .is_some_and(|label| label.starts_with("Local Changes"))
    };
    let right_side = egui::vec2(65., 0.);
    let point = find(&mut app, &local_changes).unwrap();
    click_at(&mut app, point + right_side);
    assert!(app.inline_editor().is_some());
    until(&mut app, &ctx, |app| {
        !app.editors[0].blocks_exit() && app.editors[0].context.is_some()
    });
    let subject = find(&mut app, &|node| node.role() == egui::Role::TextInput)
        .expect("inline commit subject input");
    click_at(&mut app, subject);
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Text("Inline commit".into())],
    )
    .drop_without_applying_deltas();

    let history = find(&mut app, &|node| node.label() == Some("History")).unwrap();
    click_at(&mut app, history + right_side);
    assert!(app.inline_editor().is_none());
    assert!(sidebar_point(&ctx, &mut app, "Commit subject").is_none());
    let point = find(&mut app, &local_changes).unwrap();
    click_at(&mut app, point + right_side);
    app.open(other.root.clone(), &ctx);
    assert!(app.inline_editor().is_none());
    app.select_tab(0, &ctx);
    assert!(app.inline_editor().is_some());
    assert_eq!(app.editors.len(), 1);
    until(&mut app, &ctx, |app| !app.editors[0].blocks_exit());
    assert!(find(&mut app, &|node| node.value() == Some("Inline commit")).is_some());

    let start = std::time::Instant::now();
    let commit = loop {
        if let Some(point) = find(&mut app, &|node| node.label() == Some("Commit")) {
            break point;
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(15));
        app.receive(&ctx);
        thread::sleep(std::time::Duration::from_millis(5));
    };
    let snapshot = &app.editors[0].context.as_ref().unwrap().repository;
    let (release, waiting) = mpsc::channel::<()>();
    let (_, accepted) = app.scheduler.submit(
        snapshot.identity.fingerprint.clone(),
        shovel_core::case::path_key(&snapshot.common_directory),
        "fixture:hold-working-copy".into(),
        shovel_core::scheduler::Lane::WorkingCopy,
        move || {
            let _ = waiting.recv();
            shovel_core::scheduler::Outcome::Succeeded
        },
        |_| {},
    );
    assert!(accepted);
    click_at(&mut app, commit);
    assert!(app.editors[0].busy && app.inline_editor().is_some());
    app.receive(&ctx);
    assert_eq!(
        app.operations.progress(&repo.root, &app.scheduler).as_deref(),
        Some("Queued: Commit")
    );
    assert!(find(&mut app, &|node| {
        node.label().or_else(|| node.value()) == Some("Queued: Commit")
    }).is_some());
    for top in [true, false] {
        assert_eq!(
            find(&mut app, &|node| {
                node.role() == egui::Role::ProgressIndicator
                    && node
                        .bounds()
                        .is_some_and(|r| if top { r.y0 < 100. } else { r.y0 > 350. })
            })
            .is_some(),
            top,
            "progress belongs only in the top repository header"
        );
    }
    let full_path = repo.root.to_string_lossy();
    let spoken = path_text::PathText::new(&ctx, &full_path);
    let tab = find(&mut app, &|node| {
        node.role() == egui::Role::Button && node.description() == Some(spoken.spoken())
    })
    .expect("the selected repository tab exposes its full path");
    assert!(
        tab.y <= 108.,
        "active toolbar pushed the tab strip to {tab:?}"
    );
    drop(release);
    until(&mut app, &ctx, |app| !app.editors[0].busy);
    assert!(app.inline_editor().is_some());
    assert_eq!(
        repo.command(&["log", "-1", "--format=%s"]),
        b"Inline commit\n"
    );
    until(&mut app, &ctx, |app| {
        app.editors[0].progress().is_none() && !app.tabs[0].loading && !app.tabs[0].refresh_again
    });
    assert!(
        find(&mut app, &|node| node.role()
            == egui::Role::ProgressIndicator)
        .is_none()
    );

    save_layout(&mut app, "SettingsCommitWindow");
    assert!(
        !SettingsStore::new(profile)
            .load()
            .unwrap()
            .commit_in_workspace()
    );
    assert!(app.inline_editor().is_none());
    assert!(sidebar_point(&ctx, &mut app, "Local Changes").is_none());
    let point = find(&mut app, &toolbar_commit).expect("toolbar Commit after switching back");
    click_at(&mut app, point);
    assert!(app.editors[0].open && !app.editors[0].inline);
    assert_eq!(app.editors.len(), 1);
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
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/ShovelAskPass.exe"),
    );
    let snapshot = app.tabs[0].history.as_ref().unwrap().snapshot.clone();
    let prefs = serde_json::json!([{"Path":snapshot.worktree.to_string_lossy(),"Action":2,"Branch":"main",
        "SourceBranch":"main","Remote":"other","RemoteBranch":"main","Strategy":0,"AllRemotes":true,"AllTags":false,
        "SetUpstream":false,"AutoStash":false,"ExplicitTarget":true}]);
    app.settings.set("NetworkActions", prefs.clone());
    let common = shovel_core::case::path_key(&snapshot.common_directory);
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
            .any(|r| {
                r.outcome.is_none()
                    && !r.running
                    && r.lane == Lane::Fetch
                    && r.repository == snapshot.identity.fingerprint
                    && r.common == common
            }),
        "the selected remote fetch waits behind the repository's held operation"
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

#[test]
fn selected_dos_alias_binds_waiting_fetch_and_reuses_the_verified_tab_and_preview() {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;
    let mut fixture = RepositoryFixture::new();
    let long = fixture._temp.path().join("Repository With A Long Name");
    std::fs::rename(&fixture.root, &long).unwrap();
    fixture.root = long;
    let wide: Vec<_> = fixture
        .root
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut buffer = vec![0; 32768];
    let len = unsafe { GetShortPathNameW(wide.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32) }
        as usize;
    assert!(len > 0 && len < buffer.len());
    let alias = PathBuf::from(std::ffi::OsString::from_wide(&buffer[..len]));
    let remote = fixture._temp.path().join("remote.git");
    fixture.command(&["clone", "--bare", ".", remote.to_str().unwrap()]);
    fixture.command(&["remote", "add", "origin", remote.to_str().unwrap()]);
    let ctx = egui::Context::default();
    let mut app = fixture.workspace();
    let mut store = SettingsStore::new(fixture._temp.path().join("profile"));
    store.load().unwrap();
    app.store = Some(store);
    let identity = shovel_core::identity::read(&alias).unwrap();
    app.remember_identity(
        &alias,
        identity
            .previous_fingerprint()
            .unwrap_or(&identity.fingerprint),
    );
    app.open(alias.clone(), &ctx);
    let owner = app.tabs[0].id;
    app.operations.defer_network(
        crate::operations::Kind::Fetch,
        &alias,
        &app.settings,
        &app.scheduler,
        &ctx,
    );
    app.operations.defer_network(
        crate::operations::Kind::Fetch,
        &fixture.root,
        &app.settings,
        &app.scheduler,
        &ctx,
    );
    app.close_tab(owner, &ctx);
    until(&mut app, &ctx, |app| !app.scheduler.has_accepted_work());
    let records = app.scheduler.records();
    let completed = |outcome| {
        records
            .iter()
            .filter(|r| r.outcome == Some(outcome))
            .count()
    };
    assert_eq!(completed(shovel_core::scheduler::Outcome::Succeeded), 1);
    if !Workspace::same_path(&alias, &fixture.root) {
        assert_eq!(completed(shovel_core::scheduler::Outcome::Coalesced), 1);
    }
    assert_eq!(
        fixture.command(&["rev-parse", "refs/remotes/origin/main"]),
        fixture.command(&["rev-parse", "HEAD"])
    );
    assert_eq!(
        app.known_identity(&alias).as_deref(),
        Some(identity.fingerprint.as_str())
    );
    app.open(fixture.root.clone(), &ctx);
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.tabs[0].id, owner);
    until(&mut app, &ctx, |app| {
        !app.tabs[0].loading && !app.tabs[0].view.stale
    });
    let preview = app.capture_startup_preview().unwrap();
    assert_eq!(preview.directory, alias);
    let cache = app.startup_store().unwrap();
    cache.save(Some(&preview));
    assert!(cache.read(&alias, &identity.fingerprint).is_some());
}

#[test]
fn distinct_unicode_repository_paths_keep_separate_tabs_and_pending_owners() {
    let temporary = tempfile::tempdir().unwrap();
    let mut app = workspace();
    app.tabs.clear();
    for name in ["straße", "strasse", "ﬃ", "ffi", "ı", "i"] {
        let path = temporary.path().join(name);
        std::fs::create_dir(&path).unwrap();
        app.ensure_tab(path);
    }
    assert_eq!(
        app.tabs.len(),
        6,
        "full uppercase must not collapse distinct directories"
    );
    let first = app.tabs[0].id;
    app.ensure_tab(temporary.path().join("STRAßE"));
    assert_eq!(app.tabs.len(), 6);
    assert_eq!(app.tabs[0].id, first);
    app.remember_tabs();
    assert_eq!(
        app.settings
            .get("RepositoryPaths")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        6
    );
    let ctx = egui::Context::default();
    for tab in &app.tabs {
        app.operations
            .defer_new_branch(&tab.path, &app.settings, &app.scheduler, &ctx);
    }
    assert_eq!(app.scheduler.records().len(), 6);
    let mut owners: Vec<_> = app
        .scheduler
        .records()
        .into_iter()
        .map(|r| r.repository)
        .collect();
    owners.sort();
    owners.dedup();
    assert_eq!(owners.len(), 6);
    for record in app.scheduler.records() {
        assert!(app.scheduler.remove_waiting(record.id));
    }
    app.operations.poll(&ctx);
    assert!(!app.scheduler.has_accepted_work());
}

#[test]
fn shutdown_save_completion_and_failed_save_exit_recheck_accepted_native_work() {
    use shovel_core::branches::{Action, Changes, Checkout};
    let fixture = RepositoryFixture::new();
    let mut app = fixture.workspace();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.open(fixture.root.clone(), &ctx);
    until(&mut app, &ctx, |app| !app.tabs[0].loading);
    let profile = fixture._temp.path().join("profile");
    let mut store = SettingsStore::new(profile.clone());
    store.load().unwrap();
    store.save(&app.settings).unwrap();
    app.store = Some(store);

    // Start the shutdown save, then accept work before processing its completion.
    app.closing = true;
    app.save_preferences(&ctx);
    std::fs::write(fixture.root.join(".git/hooks/post-checkout"),
        "#!/bin/sh\ntouch .git/exit-hook-started\nwhile test ! -f .git/exit-hook-release; do sleep .05; done\ntouch .git/exit-hook-finished\n").unwrap();
    struct Release(PathBuf);
    impl Drop for Release {
        fn drop(&mut self) {
            let _ = std::fs::write(&self.0, b"");
        }
    }
    let release = Release(fixture.root.join(".git/exit-hook-release"));
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
                name: "shutdown-held".into(),
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
        fixture.root.join(".git/exit-hook-started").exists()
    });
    until(&mut app, &ctx, |app| !app.saving);
    assert!(!app.exit_ready && !app.closing);
    assert_eq!(
        app.notice.as_deref(),
        Some(strings::text("AppCommandRunningClose").as_str())
    );
    assert!(app.scheduler.has_accepted_work());

    // A competing writer causes a real failed save; the choice must not bypass
    // accepted work even if it was already open when a separate view submitted.
    let foreign = b"preserve competing profile bytes";
    std::fs::write(profile.join("settings.json"), foreign).unwrap();
    app.closing = true;
    app.save_preferences(&ctx);
    until(&mut app, &ctx, |app| !app.saving);
    assert!(app.confirm_exit_without_save);
    let loading = app.tabs[0].loading;
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::F5,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    )
    .drop_without_applying_deltas();
    assert_eq!(
        app.tabs[0].loading, loading,
        "failed-save decision must gate workspace shortcuts"
    );
    sidebar_click(
        &ctx,
        &mut app,
        &strings::text("RustExitWithoutSaving"),
        egui::PointerButton::Primary,
    );
    assert!(!app.exit_ready && !app.confirm_exit_without_save);
    assert!(app.scheduler.has_accepted_work());
    assert_eq!(
        std::fs::read(profile.join("settings.json")).unwrap(),
        foreign
    );
    assert!(!fixture.root.join(".git/exit-hook-finished").exists());

    drop(release);
    until(&mut app, &ctx, |app| !app.scheduler.has_accepted_work());
    assert!(fixture.root.join(".git/exit-hook-finished").exists());
    assert_eq!(
        fixture.command(&["symbolic-ref", "--short", "HEAD"]),
        b"shutdown-held\n"
    );
    assert_eq!(std::fs::read(fixture.root.join("file")).unwrap(), b"base\n");
    app.confirm_exit_without_save = true;
    sidebar_click(
        &ctx,
        &mut app,
        &strings::text("RustExitWithoutSaving"),
        egui::PointerButton::Primary,
    );
    // The accepted checkout may have started a fresh history read. The
    // discard choice remains pending until that native read has drained.
    assert!(app.exit_ready || (app.pending_exit && app.discard_settings_on_exit));
    until(&mut app, &ctx, |app| app.exit_ready);
    assert_eq!(
        std::fs::read(profile.join("settings.json")).unwrap(),
        foreign
    );
}

#[test]
fn shutdown_failed_save_stay_open_and_retry_preserve_competing_profile() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = workspace();
    app.tabs.clear();
    let mut store = SettingsStore::new(temp.path().into());
    app.settings = store.load().unwrap();
    store.save(&app.settings).unwrap();
    app.store = Some(store);
    let foreign = b"other writer";
    std::fs::write(temp.path().join("settings.json"), foreign).unwrap();
    app.close(&ctx);
    until(&mut app, &ctx, |app| !app.saving);
    assert!(app.confirm_exit_without_save);
    sidebar_click(
        &ctx,
        &mut app,
        &strings::text("RustStayOpen"),
        egui::PointerButton::Primary,
    );
    assert!(!app.confirm_exit_without_save && !app.exit_ready && !app.closing);
    // A deliberate second close retries, but cannot overwrite another writer.
    app.close(&ctx);
    until(&mut app, &ctx, |app| !app.saving);
    assert!(app.confirm_exit_without_save);
    assert_eq!(
        std::fs::read(temp.path().join("settings.json")).unwrap(),
        foreign
    );
    sidebar_click(
        &ctx,
        &mut app,
        &strings::text("RustExitWithoutSaving"),
        egui::PointerButton::Primary,
    );
    assert!(app.exit_ready);
    assert_eq!(
        std::fs::read(temp.path().join("settings.json")).unwrap(),
        foreign
    );
}

#[test]
fn shutdown_drains_real_automatic_fetch_then_preserves_save_or_discard_intent() {
    for discard in [false, true] {
        let fixture = RepositoryFixture::new();
        let remote = fixture._temp.path().join("remote.git");
        fixture.command(&["clone", "--bare", ".", remote.to_str().unwrap()]);
        fixture.command(&["remote", "add", "origin", remote.to_str().unwrap()]);
        let script = fixture._temp.path().join("held-upload-pack.sh");
        let marker = fixture._temp.path().join("fetch-started");
        let marker_shell = marker.to_string_lossy().replace('\\', "/");
        std::fs::write(
            &script,
            format!("#!/bin/sh\ntouch '{marker_shell}'\nsleep 60\nexit 1\n"),
        )
        .unwrap();
        let upload = format!("sh '{}'", script.to_string_lossy().replace('\\', "/"));
        fixture.command(&["config", "remote.origin.uploadpack", &upload]);
        let ctx = egui::Context::default();
        let mut app = fixture.workspace();
        app.open(fixture.root.clone(), &ctx);
        until(&mut app, &ctx, |app| !app.tabs[0].loading);
        let profile = fixture._temp.path().join("profile");
        let mut store = SettingsStore::new(profile.clone());
        store.load().unwrap();
        store.save(&app.settings).unwrap();
        app.store = Some(store);
        app.settings
            .set("AutoFetchEnabled", serde_json::json!(true));
        app.auto_fetch.due_now(fixture.root.clone());
        until(&mut app, &ctx, |_| marker.exists());
        assert!(!app.auto_fetch.drained());
        if !discard {
            use shovel_core::{
                scheduler::{Lane, Outcome},
                working::Action,
            };
            std::fs::write(fixture.root.join("during-fetch"), b"staged while fetching\n").unwrap();
            let review = fixture.git.working_copy(&fixture.root, &Cancellation::default()).unwrap();
            let entry = review.entries.iter().find(|e| e.path == "during-fetch").unwrap().clone();
            let git = fixture.git.clone();
            let (sent, staged) = mpsc::channel();
            app.scheduler.submit(
                review.repository.identity.fingerprint.clone(),
                shovel_core::case::path_key(&review.repository.common_directory),
                "fixture stage during automatic fetch".into(),
                Lane::WorkingCopy,
                move || {
                    let completed = git
                        .execute_working(&review, &Action::Stage(vec![entry]), None)
                        .unwrap();
                    sent.send(completed.succeeded()).unwrap();
                    Outcome::Succeeded
                },
                |_| panic!("stage was removed"),
            );
            assert!(staged.recv_timeout(std::time::Duration::from_secs(10)).unwrap());
            assert_eq!(fixture.command(&["show", ":during-fetch"]), b"staged while fetching\n");
            assert!(app.scheduler.active_records().iter().any(|r| r.running && r.lane == Lane::Fetch));
        }
        let original_head = fixture.command(&["rev-parse", "HEAD"]);
        let original_index = fixture.command(&["ls-files", "--stage", "-z"]);
        let foreign = b"other writer; exit without saving must preserve this";
        if discard {
            std::fs::write(profile.join("settings.json"), foreign).unwrap();
            app.confirm_exit_without_save = true;
            app.exit_without_saving(&ctx);
        } else {
            app.close(&ctx);
        }
        assert!(app.pending_exit && !app.exit_ready && !app.closing);
        assert_eq!(app.discard_settings_on_exit, discard);
        assert_eq!(app.notice, Some(strings::text("RustShutdownBackground")));
        let started = std::time::Instant::now();
        until(&mut app, &ctx, |app| app.exit_ready);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "automatic fetch cancellation must drain instead of waiting for the 60-second transport"
        );
        assert!(app.auto_fetch.drained());
        assert!(!app.scheduler.has_accepted_work());
        assert_eq!(
            app.operations.background_count(),
            1,
            "shutdown must not start another automatic fetch"
        );
        assert_eq!(fixture.command(&["rev-parse", "HEAD"]), original_head);
        assert_eq!(
            fixture.command(&["ls-files", "--stage", "-z"]),
            original_index
        );
        assert_eq!(std::fs::read(fixture.root.join("file")).unwrap(), b"base\n");
        if discard {
            assert_eq!(
                std::fs::read(profile.join("settings.json")).unwrap(),
                foreign
            );
        } else {
            let saved = SettingsStore::new(profile).load().unwrap();
            assert_eq!(saved.repositories(), vec![fixture.root.clone()]);
        }
    }
}

#[test]
fn shutdown_waits_for_tool_handoff_completion() {
    let ctx = egui::Context::default();
    let mut app = workspace();
    app.tabs.clear();
    let profile = tempfile::tempdir().unwrap();
    let mut store = SettingsStore::new(profile.path().into());
    app.settings = store.load().unwrap();
    app.store = Some(store);
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    app.begin_tool_handoff(&ctx, move || {
        started_tx.send(()).unwrap();
        release_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        Ok(())
    });
    started_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    app.close(&ctx);
    assert!(app.pending_exit && !app.exit_ready);
    assert!(!app.external_tools_opened);
    release_tx.send(()).unwrap();
    until(&mut app, &ctx, |app| app.exit_ready);
    assert!(app.external_tools_opened);
    assert!(app.read_workers.cancel_and_drained());
}

#[test]
fn update_restart_checks_known_identity_and_index_conflicts_without_touching_git_state() {
    use crate::update_restart::{Repository, check_repositories};
    let repo = RepositoryFixture::new();
    let mut target = Repository {
        path: repo.root.clone(),
        observed: None,
        identity_blocked: false,
    };
    let inspect =
        |target: &Repository| check_repositories(Some(&repo.git), std::slice::from_ref(target));
    assert!(check_repositories(None, &[]).is_ok());
    assert!(
        check_repositories(None, std::slice::from_ref(&target))
            .unwrap_err()
            .contains(repo.root.to_str().unwrap())
    );
    std::fs::write(repo.root.join("untracked"), "user edit").unwrap();
    inspect(&target).unwrap();
    target.observed = Some(shovel_core::identity::read(&repo.root).unwrap());
    let other = RepositoryFixture::new();
    target.path = other.root.clone();
    assert!(inspect(&target).is_err());
    target.path = repo.root.clone();
    target.identity_blocked = true;
    assert!(inspect(&target).is_err());
    target.identity_blocked = false;
    repo.command(&["switch", "-c", "side"]);
    std::fs::write(repo.root.join("file"), "side\n").unwrap();
    repo.command(&["commit", "-am", "side"]);
    repo.command(&["switch", "main"]);
    std::fs::write(repo.root.join("file"), "main\n").unwrap();
    repo.command(&["commit", "-am", "main"]);
    let result = shovel_core::process::run(
        &repo.git.request(
            &repo.root,
            ["merge", "--no-edit", "side"],
            shovel_core::process::Lifetime::AcceptedCommand,
            65_536,
        ),
        &Cancellation::default(),
        None,
    )
    .unwrap();
    assert_eq!(result.exit_code, 1);
    assert!(repo.root.join(".git/MERGE_HEAD").is_file());
    assert!(inspect(&target).is_err());
    repo.command(&["merge", "--quit"]);
    assert!(!repo.root.join(".git/MERGE_HEAD").exists());
    let unresolved = repo.command(&["ls-files", "-u", "-z"]);
    assert!(!unresolved.is_empty());
    let index = std::fs::read(repo.root.join(".git/index")).unwrap();
    assert!(inspect(&target).is_err());
    assert_eq!(std::fs::read(repo.root.join(".git/index")).unwrap(), index);
    assert_eq!(repo.command(&["ls-files", "-u", "-z"]), unresolved);
    std::fs::write(repo.root.join("file"), "resolved\n").unwrap();
    repo.command(&["add", "file"]);
    inspect(&target).unwrap();
    assert_eq!(
        std::fs::read(repo.root.join("untracked")).unwrap(),
        b"user edit"
    );
}

#[test]
fn update_restart_blocks_an_active_bisect_and_preserves_its_state() {
    use crate::update_restart::{Repository, check_repositories};
    let repo = RepositoryFixture::new();
    let target = Repository {
        path: repo.root.clone(),
        observed: Some(shovel_core::identity::read(&repo.root).unwrap()),
        identity_blocked: false,
    };
    for content in ["middle\n", "tip\n"] {
        std::fs::write(repo.root.join("file"), content).unwrap();
        repo.command(&["commit", "-am", content.trim()]);
    }
    repo.command(&["bisect", "start", "HEAD", "HEAD~2"]);
    let state = repo.root.join(".git/BISECT_START");
    let before = std::fs::read(&state).unwrap();
    let head = repo.command(&["rev-parse", "HEAD"]);
    let index = std::fs::read(repo.root.join(".git/index")).unwrap();
    let targets = std::slice::from_ref(&target);
    assert!(check_repositories(Some(&repo.git), targets).is_err());
    assert_eq!(std::fs::read(state).unwrap(), before);
    assert_eq!(repo.command(&["rev-parse", "HEAD"]), head);
    assert_eq!(std::fs::read(repo.root.join(".git/index")).unwrap(), index);
    repo.command(&["bisect", "reset"]);
    check_repositories(Some(&repo.git), targets).unwrap();
}

#[test]
fn update_restart_checks_index_without_enumerating_large_reference_sets() {
    use crate::update_restart::{Repository, check_repositories};
    use std::fmt::Write;

    let repo = RepositoryFixture::new();
    let head = repo.command(&["rev-parse", "HEAD"]);
    let oid = std::str::from_utf8(&head).unwrap().trim();
    let suffix = "long-reference".repeat(15);
    let mut packed = String::from("# pack-refs with: sorted\n");
    for n in 0..17_000 {
        writeln!(packed, "{oid} refs/tags/{n:05}-{suffix}").unwrap();
    }
    let refs = repo.root.join(".git/packed-refs");
    std::fs::write(&refs, &packed).unwrap();
    assert!(repo.git.inspect(&repo.root, &Cancellation::default()).is_err());
    let target = Repository {
        path: repo.root.clone(),
        observed: Some(shovel_core::identity::read(&repo.root).unwrap()),
        identity_blocked: false,
    };
    check_repositories(Some(&repo.git), &[target]).unwrap();
    assert_eq!(std::fs::read_to_string(refs).unwrap(), packed);
    assert_eq!(repo.command(&["rev-parse", "HEAD"]), head);
}

#[test]
fn shutdown_refuses_open_repository_picker_and_keeps_single_picker() {
    let ctx = egui::Context::default();
    let mut app = workspace();
    app.tabs.clear();
    let profile = tempfile::tempdir().unwrap();
    let mut store = SettingsStore::new(profile.path().into());
    app.settings = store.load().unwrap();
    app.store = Some(store);
    let (release_tx, release_rx) = mpsc::channel();
    app.begin_repository_picker(&ctx, move || {
        release_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        None
    });
    app.begin_repository_picker(&ctx, || panic!("duplicate picker"));
    app.close(&ctx);
    assert!(!app.pending_exit && !app.closing && !app.exit_ready);
    assert_eq!(app.notice, Some(strings::text("RustRepositoryChooserOpen")));
    release_tx.send(()).unwrap();
    until(&mut app, &ctx, |app| app.repository_picker.is_none());
    app.close(&ctx);
    until(&mut app, &ctx, |app| app.exit_ready);
    assert!(app.read_workers.cancel_and_drained());
}
