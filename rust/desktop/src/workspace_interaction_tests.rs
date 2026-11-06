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
