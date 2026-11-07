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
            page: None,
            selected: None,
            previous: vec![],
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
