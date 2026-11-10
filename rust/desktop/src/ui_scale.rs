//! Shared C#-compatible scale shortcuts, dispatched before widgets in every viewport.
use eframe::egui;
use std::sync::Arc;

fn state_id() -> egui::Id {
    egui::Id::unique("shovel-ui-scale")
}
#[derive(Clone, Copy)]
struct State {
    desired: f32,
    revision: u64,
    changed: bool,
}
pub fn install(ctx: &egui::Context) {
    ctx.options_mut(|options| options.zoom_with_keyboard = false);
    let zoom = ctx.zoom_factor();
    let installed = ctx.data_mut(|data| {
        if data.get_temp::<State>(state_id()).is_some() {
            true
        } else {
            data.insert_temp(
                state_id(),
                State {
                    desired: zoom,
                    revision: 0,
                    changed: false,
                },
            );
            false
        }
    });
    if !installed {
        ctx.on_begin_pass("Shovel scale shortcuts", Arc::new(|ui| shortcut(ui.ctx())));
    }
}
pub fn set(ctx: &egui::Context, value: f32) {
    install(ctx);
    let value = if value.is_finite() {
        ((value * 10.).round() / 10.).clamp(0.7, 2.)
    } else {
        1.
    };
    ctx.data_mut(|data| {
        let mut state = data.get_temp::<State>(state_id()).unwrap();
        state.desired = value;
        state.changed = false;
        data.insert_temp(state_id(), state);
    });
    ctx.set_zoom_factor(value);
}
pub fn revision(ctx: &egui::Context) -> u64 {
    ctx.data(|data| data.get_temp::<State>(state_id()).map_or(0, |s| s.revision))
}
pub fn current(ctx: &egui::Context) -> f32 {
    ctx.data(|data| data.get_temp::<State>(state_id()).map(|s| s.desired))
        .unwrap_or_else(|| ctx.zoom_factor())
}
pub fn take_change(ctx: &egui::Context) -> Option<f32> {
    ctx.data_mut(|data| {
        let mut state = data.get_temp::<State>(state_id())?;
        if !state.changed {
            return None;
        }
        state.changed = false;
        data.insert_temp(state_id(), state);
        Some(state.desired)
    })
}
fn shortcut(ctx: &egui::Context) {
    let mut steps = Vec::new();
    ctx.input_mut(|input| {
        input.events.retain(|event| {
            if let egui::Event::Key {
                key,
                pressed,
                repeat,
                modifiers,
                ..
            } = event
                && modifiers.ctrl
                && matches!(key, egui::Key::Plus | egui::Key::Equals | egui::Key::Minus)
            {
                if *pressed && !repeat {
                    steps.push(if *key == egui::Key::Minus { -1. } else { 1. });
                }
                return false;
            }
            true
        })
    });
    // Input is replayed during a layout discard; consume it on each pass but
    // apply it only once. This also works for independent native child viewports.
    if ctx.current_pass_index() != 0 || steps.is_empty() {
        return;
    }
    let next = ctx.data_mut(|data| {
        let mut state = data.get_temp::<State>(state_id()).unwrap();
        let before = state.desired;
        for step in steps {
            state.desired = ((state.desired * 10. + step).round() / 10.).clamp(0.7, 2.);
        }
        if state.desired == before {
            return None;
        }
        state.revision += 1;
        state.changed = true;
        data.insert_temp(state_id(), state);
        Some(state.desired)
    });
    if let Some(next) = next {
        ctx.set_zoom_factor(next);
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }
}
impl crate::Workspace {
    pub(super) fn sync_ui_scale(&mut self, ctx: &egui::Context) {
        if let Some(scale) = take_change(ctx) {
            self.settings.set("UiScale", serde_json::json!(scale));
            if let Some(dialog) = &mut self.settings_dialog {
                dialog.values.set("UiScale", serde_json::json!(scale));
            }
            self.preferences_dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Event, SettingsStore, settings_ui, workspace_interaction_tests::workspace};

    fn frame(ctx: &egui::Context, key: egui::Key, pressed: bool, ctrl: bool, child: bool) {
        let id = if child {
            egui::ViewportId::from_hash_of("scale-child")
        } else {
            egui::ViewportId::ROOT
        };
        let mut input = egui::RawInput {
            viewport_id: id,
            ..Default::default()
        };
        input.viewports.entry(id).or_default();
        let modifiers = egui::Modifiers {
            ctrl,
            ..Default::default()
        };

        input.events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        });
        ctx.run_ui(input, |_| {}).drop_without_applying_deltas();
    }
    fn tap(ctx: &egui::Context, key: egui::Key, child: bool) {
        frame(ctx, key, true, true, child);
        frame(ctx, key, false, true, child);
    }
    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 0.00001, "{a} != {b}");
    }

    #[test]
    fn shortcuts_are_shared_bounded_and_ignore_held_keys() {
        let ctx = egui::Context::default();
        set(&ctx, 1.);
        install(&ctx);
        frame(&ctx, egui::Key::Plus, true, true, false);
        near(current(&ctx) as f64, 1.1);
        frame(&ctx, egui::Key::Plus, true, true, false);
        near(current(&ctx) as f64, 1.1);
        frame(&ctx, egui::Key::Plus, false, true, false);
        tap(&ctx, egui::Key::Equals, true);
        near(current(&ctx) as f64, 1.2);
        assert_eq!(revision(&ctx), 2);
        for _ in 0..20 {
            tap(&ctx, egui::Key::Plus, false);
        }
        near(current(&ctx) as f64, 2.);
        for _ in 0..20 {
            tap(&ctx, egui::Key::Minus, true);
        }
        near(current(&ctx) as f64, 0.7);
        frame(&ctx, egui::Key::Plus, true, false, false);
        near(current(&ctx) as f64, 0.7);
        assert!(take_change(&ctx).is_some());
        assert!(take_change(&ctx).is_none());
    }

    fn drain(app: &mut crate::Workspace, ctx: &egui::Context) {
        let start = std::time::Instant::now();
        loop {
            app.receive(ctx);
            if !app.saving && !app.preferences_dirty {
                break;
            }
            assert!(start.elapsed() < std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    #[test]
    fn shortcuts_save_without_exit_and_survive_delayed_preferences_completion() {
        let temp = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        set(&ctx, 1.);
        let mut app = workspace();
        app.tabs.clear();
        let mut store = SettingsStore::new(temp.path().into());
        app.settings = store.load().unwrap();
        app.store = Some(store);
        app.settings_dialog = Some(settings_ui::Dialog::new(&app.settings));
        tap(&ctx, egui::Key::Plus, true);
        app.sync_ui_scale(&ctx);
        near(app.settings_dialog.as_ref().unwrap().values.ui_scale(), 1.1);
        drain(&mut app, &ctx);
        near(
            SettingsStore::new(temp.path().into())
                .load()
                .unwrap()
                .ui_scale(),
            1.1,
        );

        let saved_revision = revision(&ctx);
        let old = app.settings.clone();
        let mut store = app.store.take().unwrap();
        store.save(&old).unwrap();
        app.saving = true;
        tap(&ctx, egui::Key::Plus, false);
        app.sender
            .send(Event::PreferencesSaved(
                Box::new(store),
                Box::new(old),
                Ok(()),
                saved_revision,
            ))
            .unwrap();
        drain(&mut app, &ctx);
        near(app.settings.ui_scale(), 1.2);
        near(current(&ctx) as f64, 1.2);
        near(
            SettingsStore::new(temp.path().into())
                .load()
                .unwrap()
                .ui_scale(),
            1.2,
        );
        assert!(app.settings_dialog.is_none());
    }
}
