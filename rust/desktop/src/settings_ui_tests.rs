use super::*;
use egui::{Event, Pos2, Rect, Vec2, epaint::Shape};

fn frame(
    ctx: &egui::Context,
    dialog: &mut Dialog,
    size: Vec2,
    events: Vec<Event>,
) -> egui::FullOutput {
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |_| {
            dialog.show(ctx, None);
        },
    )
}

fn settled(ctx: &egui::Context, dialog: &mut Dialog, size: Vec2) -> egui::FullOutput {
    for _ in 0..3 {
        frame(ctx, dialog, size, vec![]).drop_without_applying_deltas();
    }
    frame(ctx, dialog, size, vec![])
}

fn bounds(out: &egui::FullOutput, label: &str) -> Rect {
    let node = out
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some(label) && n.bounds().is_some())
        .unwrap_or_else(|| panic!("Missing control {label}"));
    let r = node.1.bounds().unwrap();
    Rect::from_min_max(
        egui::pos2(r.x0 as f32, r.y0 as f32),
        egui::pos2(r.x1 as f32, r.y1 as f32),
    )
}

fn click(ctx: &egui::Context, dialog: &mut Dialog, size: Vec2, pos: Pos2) {
    frame(ctx, dialog, size, vec![Event::PointerMoved(pos)]).drop_without_applying_deltas();
    for pressed in [true, false] {
        frame(
            ctx,
            dialog,
            size,
            vec![Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        )
        .drop_without_applying_deltas();
    }
}

fn painted(out: &egui::FullOutput, predicate: impl Fn(&str) -> bool) -> &egui::epaint::TextShape {
    out.shapes
        .iter()
        .find_map(|s| match &s.shape {
            Shape::Text(t) if predicate(t.galley.text()) => Some(t),
            _ => None,
        })
        .expect("visible text shape")
}

#[test]
fn vertical_categories_publish_linked_tabs_and_keep_arrow_navigation_inside_the_rail() {
    use egui::accesskit::{Action, NodeId, Orientation, TreeUpdate};

    fn current(tree: &TreeUpdate, key: &str) -> NodeId {
        let tabs: Vec<_> = tree
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == egui::Role::Tab)
            .collect();
        assert_eq!(tabs.len(), Page::ALL.len());
        let (_, list) = tree
            .nodes
            .iter()
            .find(|(_, node)| node.role() == egui::Role::TabList)
            .unwrap();
        assert_eq!(list.orientation(), Some(Orientation::Vertical));
        assert_eq!(list.children().len(), Page::ALL.len());
        let selected: Vec<_> = tabs
            .iter()
            .filter(|(_, node)| node.is_selected() == Some(true))
            .collect();
        assert_eq!(selected.len(), 1);
        let (id, tab) = selected[0];
        assert_eq!(tab.label(), Some(text(key).as_str()));
        assert!(list.children().contains(id));
        assert!(tab.supports_action(Action::Focus));
        assert!(
            tabs.iter()
                .all(|(candidate, node)| node.supports_action(Action::Focus) == (candidate == id))
        );
        let (panel_id, panel) = tree
            .nodes
            .iter()
            .find(|(_, node)| node.role() == egui::Role::TabPanel)
            .unwrap();
        assert_eq!(tab.controls(), [*panel_id]);
        assert_eq!(panel.labelled_by(), [*id]);
        *id
    }

    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut dialog = Dialog::new(&Settings::default());
    let size = egui::vec2(760., 520.);
    let opening = settled(&ctx, &mut dialog, size);
    let initial_tree = opening.platform_output.accesskit_update.as_ref().unwrap();
    let initial = current(initial_tree, Page::Appearance.key());
    frame(
        &ctx,
        &mut dialog,
        size,
        vec![Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: Action::Focus,
                target_tree: initial_tree.tree_id,
                target_node: initial,
                data: None,
            },
        )],
    )
    .drop_without_applying_deltas();

    let key = |key| {
        [true, false]
            .map(|pressed| Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
            .to_vec()
    };
    for (input, page) in [
        (egui::Key::ArrowDown, Page::History),
        (egui::Key::End, Page::Updates),
        (egui::Key::ArrowDown, Page::Appearance),
        (egui::Key::ArrowUp, Page::Updates),
        (egui::Key::Home, Page::Appearance),
    ] {
        frame(&ctx, &mut dialog, size, key(input)).drop_without_applying_deltas();
        let output = settled(&ctx, &mut dialog, size);
        let tree = output.platform_output.accesskit_update.as_ref().unwrap();
        assert_eq!(dialog.page, page);
        assert_eq!(tree.focus, current(tree, page.key()));
    }
    frame(&ctx, &mut dialog, size, key(egui::Key::Tab)).drop_without_applying_deltas();
    let output = settled(&ctx, &mut dialog, size);
    let tree = output.platform_output.accesskit_update.as_ref().unwrap();
    assert_eq!(dialog.page, Page::Appearance);
    let (_, control) = tree.nodes.iter().find(|(id, _)| *id == tree.focus).unwrap();
    assert_eq!(control.role(), egui::Role::RadioButton);
    assert_eq!(control.label(), Some(text("WorkspaceDark").as_str()));
}

#[test]
fn preview_uses_pending_layout_and_font_and_both_modes_keep_the_original_settings() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let size = egui::vec2(900., 620.);
    let mut saved = Settings::default();
    saved.set("DiffSideBySide", json!(false));
    saved.set("DiffFontSize", json!(12));
    let mut dialog = Dialog::new(&saved);
    let opening = settled(&ctx, &mut dialog, size);
    click(
        &ctx,
        &mut dialog,
        size,
        bounds(&opening, &text("SettingsPageDiff")).center(),
    );
    let unified = settled(&ctx, &mut dialog, size);
    let old = painted(&unified, |t| t.contains("-     10"));
    let new = painted(&unified, |t| t.contains("+     60"));
    assert_eq!(old.pos.x, new.pos.x);
    assert!(old.pos.y < new.pos.y);
    assert_eq!(new.galley.job.sections[0].format.font_id.size, 12.);
    click(
        &ctx,
        &mut dialog,
        size,
        bounds(&unified, &text("SettingsDiffDefault")).center(),
    );
    let controls = settled(&ctx, &mut dialog, size);
    let slider = controls
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, n)| n.role() == egui::Role::Slider)
        .unwrap()
        .1
        .bounds()
        .unwrap();
    click(
        &ctx,
        &mut dialog,
        size,
        egui::pos2(slider.x1 as f32 - 2., ((slider.y0 + slider.y1) / 2.) as f32),
    );
    let paired = settled(&ctx, &mut dialog, size);
    let old = painted(&paired, |t| t.contains("-     10"));
    let new = painted(&paired, |t| t.contains("+     60"));
    assert_eq!(old.pos.y, new.pos.y);
    assert!(old.pos.x < new.pos.x);
    assert_eq!(new.galley.job.sections[0].format.font_id.size, 24.);
    assert_eq!(saved.get("DiffSideBySide"), Some(&json!(false)));
    assert_eq!(saved.get("DiffFontSize"), Some(&json!(12)));
}

#[test]
fn tools_aligns_paths_and_stacks_them_in_a_narrow_window_with_a_fixed_footer() {
    for (size, wide) in [
        (egui::vec2(900., 620.), true),
        (egui::vec2(440., 390.), false),
    ] {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::workspace_style::apply(&ctx, !wide);
        let mut dialog = Dialog::new(&Settings::default());
        let open = settled(&ctx, &mut dialog, size);
        let rail: Vec<_> = Page::ALL
            .iter()
            .map(|p| bounds(&open, &text(p.key())))
            .collect();
        assert!(
            rail.windows(2)
                .all(|pair| pair[0].left() == pair[1].left() && pair[0].bottom() <= pair[1].top())
        );
        let save = bounds(&open, &text("SettingsSave"));
        assert!(bounds(&open, &text("Cancel")).right() < save.left());
        click(
            &ctx,
            &mut dialog,
            size,
            bounds(&open, &text("SettingsPageTools")).center(),
        );
        let out = settled(&ctx, &mut dialog, size);
        let rider = painted(&out, |s| s == text("SettingsRiderApp"));
        let field = painted(&out, |s| s == text("SettingsAutoDetect"));
        assert!(rider.pos.x > rail[0].right());
        if wide {
            assert!((rider.pos.y - field.pos.y).abs() <= 6. && rider.pos.x + 55. < field.pos.x);
            let next = painted(&out, |s| s == text("SettingsCodeApp"));
            assert_eq!(rider.pos.x, next.pos.x);
        } else {
            assert!(field.pos.y > rider.pos.y && (field.pos.x - rider.pos.x).abs() < 12.);
        }
        let footer = bounds(&out, &text("SettingsSave"));
        assert_eq!(save, footer);
        assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(footer));
    }
}

#[test]
fn tool_path_edits_survive_validation_and_cancel_discards_the_draft() {
    use crate::workspace_interaction_tests::{frame as workspace_frame, sidebar_click, workspace};
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    ctx.set_embed_viewports(true);
    let mut app = workspace();
    app.tabs.clear();
    app.settings_dialog = Some(Dialog::new(&app.settings));
    let click_control = |app: &mut crate::Workspace, key| {
        sidebar_click(&ctx, app, &text(key), egui::PointerButton::Primary);
    };
    click_control(&mut app, "SettingsPageTools");
    let out = workspace_frame(&ctx, &mut app, vec![]);
    let input = painted(&out, |s| s == text("SettingsAutoDetect")).pos + egui::vec2(4., 4.);
    for event in [
        Event::PointerMoved(input),
        Event::PointerButton {
            pos: input,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        Event::PointerButton {
            pos: input,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
        Event::Text("relative.exe".into()),
    ] {
        workspace_frame(&ctx, &mut app, vec![event]).drop_without_applying_deltas();
    }
    click_control(&mut app, "SettingsSave");
    let dialog = app.settings_dialog.as_ref().unwrap();
    assert_eq!(dialog.error, Some(text("SettingsToolPathInvalid")));
    assert_eq!(
        dialog.values.get("RiderExecutable"),
        Some(&json!("relative.exe"))
    );
    click_control(&mut app, "SettingsPageGit");
    click_control(&mut app, "SettingsPageTools");
    let returned = workspace_frame(&ctx, &mut app, vec![]);
    painted(&returned, |s| s == "relative.exe");
    click_control(&mut app, "Cancel");
    assert!(app.settings_dialog.is_none());
    assert_ne!(
        app.settings.get("RiderExecutable"),
        Some(&json!("relative.exe"))
    );
}
