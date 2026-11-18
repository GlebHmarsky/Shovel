//! Persist window dimensions in Windows logical units, independent of app zoom.
//! Egui viewport rectangles/commands instead use zoomed UI points.
use eframe::egui;

pub fn points(ctx: &egui::Context, native: egui::Vec2) -> egui::Vec2 {
    native / ctx.zoom_factor()
}

pub fn saved_layout<'a>(
    settings: &'a shovel_core::settings::Settings,
    key: &str,
) -> Option<&'a serde_json::Value> {
    settings.get("WindowLayouts")?.as_array()?.iter().find(|v| {
        v.get("Key").and_then(|k| k.as_str()) == Some(key)
            && v.get("Width")
                .and_then(|n| n.as_f64())
                .is_some_and(|n| n.is_finite() && n > 0.)
            && v.get("Height")
                .and_then(|n| n.as_f64())
                .is_some_and(|n| n.is_finite() && n > 0.)
    })
}

pub fn save_layout(
    settings: &mut shovel_core::settings::Settings,
    key: &str,
    size: egui::Vec2,
    maximized: bool,
) {
    let mut layouts = settings
        .get("WindowLayouts")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let index = layouts
        .iter()
        .position(|v| v.get("Key").and_then(|k| k.as_str()) == Some(key));
    let mut value = index
        .and_then(|i| layouts[i].as_object().cloned())
        .unwrap_or_default();
    for (key, data) in [
        ("Key", serde_json::json!(key)),
        ("Width", serde_json::json!(size.x)),
        ("Height", serde_json::json!(size.y)),
        ("Maximized", serde_json::json!(maximized)),
    ] {
        value.insert(key.into(), data);
    }
    value
        .entry("Panes")
        .or_insert_with(|| serde_json::json!([]));
    if let Some(index) = index {
        layouts[index] = value.into();
    } else {
        layouts.push(value.into());
    }
    settings.set("WindowLayouts", serde_json::json!(layouts));
}

#[derive(Default)]
pub struct Geometry {
    last_zoom: Option<f32>,
    pub native_size: Option<egui::Vec2>,
}
impl Geometry {
    pub fn observe(&mut self, ctx: &egui::Context) {
        let zoom = ctx.zoom_factor();
        let previous = self.last_zoom.replace(zoom);
        // Integration input can contain the preceding zoom's rectangle in the
        // first pass after set_zoom_factor. Wait for fresh native viewport input.
        if previous != Some(zoom) {
            return;
        }
        ctx.input(|input| {
            let viewport = input.viewport();
            if viewport.maximized.unwrap_or(false) || viewport.minimized.unwrap_or(false) {
                return;
            }
            if let Some(rect) = viewport.inner_rect {
                let size = rect.size() * zoom;
                if size.is_finite() && size.x > 0. && size.y > 0. {
                    self.native_size = Some(size);
                }
            }
        });
    }
}

pub(super) struct InspectorLayout {
    key: &'static str,
    size: egui::Vec2,
    maximized: bool,
    geometry: Geometry,
    restored: bool,
    changed: bool,
}
impl InspectorLayout {
    pub fn new(key: &'static str, size: egui::Vec2) -> Self {
        Self {
            key,
            size,
            maximized: false,
            geometry: Geometry::default(),
            restored: false,
            changed: false,
        }
    }
    pub fn builder(&self, ctx: &egui::Context) -> egui::ViewportBuilder {
        egui::ViewportBuilder::default()
            .with_inner_size(points(ctx, self.size))
            .with_min_inner_size(points(ctx, egui::vec2(900., 620.)))
            .with_maximized(self.maximized)
            .with_minimize_button(false)
    }
    pub fn new_viewport(&mut self) {
        self.geometry = Geometry::default();
    }
    pub fn observe(&mut self, ctx: &egui::Context, class: egui::ViewportClass) {
        if class == egui::ViewportClass::EmbeddedWindow {
            return;
        }
        let stable_zoom = self.geometry.last_zoom == Some(ctx.zoom_factor());
        self.geometry.observe(ctx);
        if !stable_zoom {
            return;
        }
        let (minimized, maximized) =
            ctx.input(|i| (i.viewport().minimized, i.viewport().maximized));
        if minimized == Some(true) {
            return;
        }
        if let Some(maximized) = maximized {
            self.changed |= self.maximized != maximized;
            self.maximized = maximized;
        }
        if let Some(size) = self.geometry.native_size {
            self.changed |= self.size != size;
            self.size = size;
        }
    }
    fn sync(
        &mut self,
        settings: &mut shovel_core::settings::Settings,
        open: bool,
        exiting: bool,
    ) -> bool {
        if open && !self.restored {
            if let Some(layout) = saved_layout(settings, self.key) {
                self.size = egui::vec2(
                    layout["Width"].as_f64().unwrap().clamp(900., 7680.) as f32,
                    layout["Height"].as_f64().unwrap().clamp(620., 4320.) as f32,
                );
                self.maximized = layout["Maximized"].as_bool().unwrap_or(false);
            }
            self.restored = true;
        }
        let save = (!open || exiting) && std::mem::take(&mut self.changed);
        if save {
            save_layout(settings, self.key, self.size, self.maximized);
        }
        if !open && self.restored {
            self.restored = false;
            self.geometry = Geometry::default();
        }
        save
    }
}

impl crate::Workspace {
    pub(super) fn sync_inspector_layouts(&mut self, exiting: bool) {
        for view in &mut self.stash_views {
            self.preferences_dirty |= view.layout.sync(&mut self.settings, view.open, exiting);
        }
        for inspector in self
            .editors
            .iter_mut()
            .filter_map(|editor| editor.inspector.as_mut())
        {
            self.preferences_dirty |=
                inspector
                    .layout
                    .sync(&mut self.settings, inspector.open, exiting);
        }
    }
    pub(super) fn restore_main_window_layout(&mut self, ctx: &egui::Context) {
        if !self.restore_window_layout {
            return;
        }
        if (ctx.zoom_factor() - self.settings.ui_scale() as f32).abs() > 0.001 {
            ctx.request_repaint();
            return;
        }
        let layout = saved_layout(&self.settings, "MainWindow");
        let dimension = |key, legacy, default, min| {
            layout
                .and_then(|v| v.get(key))
                .or_else(|| self.settings.get(legacy))
                .and_then(|v| v.as_f64())
                .unwrap_or(default)
                .clamp(min, 8192.) as f32
        };
        let native = egui::vec2(
            dimension("Width", "WindowWidth", 1200., 900.),
            dimension("Height", "WindowHeight", 800., 620.),
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(points(ctx, native)));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(
            layout
                .and_then(|v| v.get("Maximized"))
                .or_else(|| self.settings.get("IsMaximized"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        ));
        self.main_geometry = Geometry::default();
        self.restore_window_layout = false;
    }
}

#[cfg(test)]
pub(crate) fn fixture_input(
    size_in_points: egui::Vec2,
    native_scale: f32,
    maximized: bool,
) -> egui::RawInput {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, size_in_points);
    let mut input = egui::RawInput {
        screen_rect: Some(rect),
        ..Default::default()
    };
    let viewport = input.viewports.entry(egui::ViewportId::ROOT).or_default();
    viewport.native_pixels_per_point = Some(native_scale);
    viewport.inner_rect = Some(rect);
    viewport.maximized = Some(maximized);
    input
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SettingsStore, workspace_interaction_tests::workspace};

    #[test]
    fn native_window_geometry_keeps_normal_bounds_when_first_close_is_maximized() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = SettingsStore::new(temp.path().into());
        store.load().unwrap();
        let mut app = workspace();
        app.tabs.clear();
        app.store = Some(store);
        app.settings.set(
            "WindowLayouts",
            serde_json::json!([
                {"Key":"MainWindow", "Width":1100, "Height":740, "Maximized":false, "Panes":[]}
            ]),
        );
        let ctx = egui::Context::default();
        ctx.run_ui(fixture_input(egui::vec2(1920., 1080.), 1., true), |ui| {
            app.close(ui.ctx())
        })
        .drop_without_applying_deltas();
        let start = std::time::Instant::now();
        while app.saving {
            assert!(start.elapsed() < std::time::Duration::from_secs(5));
            app.receive(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let settings = SettingsStore::new(temp.path().into()).load().unwrap();
        let layout = saved_layout(&settings, "MainWindow").unwrap();
        assert_eq!(layout["Width"].as_f64(), Some(1100.));
        assert_eq!(layout["Height"].as_f64(), Some(740.));
        assert_eq!(layout["Maximized"], true);
    }

    #[test]
    fn native_window_geometry_roundtrips_saved_scale_without_shrinking() {
        let native = egui::vec2(1100., 740.);
        for zoom in [0.7, 1.1, 2.0] {
            for dpi in [1., 1.25, 2.] {
                let temp = tempfile::tempdir().unwrap();
                let mut store = SettingsStore::new(temp.path().into());
                let mut settings = store.load().unwrap();
                settings.set("UiScale", serde_json::json!(zoom));
                settings.set("WindowWidth", serde_json::json!(native.x));
                settings.set("WindowHeight", serde_json::json!(native.y));
                settings.set("RustSidebarRatio", serde_json::json!(0.21));
                settings.set("RustDetailsRatio", serde_json::json!(0.31));
                settings.set("WindowLayouts", serde_json::json!([
                    {"Key":"MainWindow", "Width":1100, "Height":740, "Maximized":false,
                        "Panes":[{"Key":"legacy-pane","Columns":[{"Value":2,"Star":true}],"Rows":[]}]},
                    {"Key":"StashWindow", "Width":950, "Height":700, "Maximized":false, "Panes":[]}
                ]));
                settings.set("WindowWidth", serde_json::json!(1500));
                store.save(&settings).unwrap();
                for _ in 0..3 {
                    let ctx = egui::Context::default();
                    let mut app = workspace();
                    app.tabs.clear();
                    let mut store = SettingsStore::new(temp.path().into());
                    app.settings = store.load().unwrap();
                    app.store = Some(store);
                    app.restore_window_layout = true;
                    // Production startup sets zoom while the old pass is still active.
                    let output = ctx.run_ui(fixture_input(native, dpi, false), |ui| {
                        ctx.set_zoom_factor(zoom);
                        app.restore_main_window_layout(ui.ctx());
                    });
                    assert!(app.restore_window_layout);
                    assert!(
                        !output.viewport_output[&egui::ViewportId::ROOT]
                            .commands
                            .iter()
                            .any(|c| matches!(c, egui::ViewportCommand::InnerSize(_)))
                    );
                    output.drop_without_applying_deltas();
                    let output = ctx.run_ui(fixture_input(native, dpi, false), |ui| {
                        app.restore_main_window_layout(ui.ctx())
                    });
                    let requested = output.viewport_output[&egui::ViewportId::ROOT]
                        .commands
                        .iter()
                        .find_map(|c| match c {
                            egui::ViewportCommand::InnerSize(size) => Some(*size),
                            _ => None,
                        })
                        .unwrap();
                    assert!((requested * ctx.zoom_factor() - native).length() < 0.01);
                    output.drop_without_applying_deltas();
                    // Feed the actual post-resize rectangle at this UI/monitor scale.
                    for _ in 0..2 {
                        ctx.run_ui(fixture_input(native / zoom, dpi, false), |ui| {
                            app.main_geometry.observe(ui.ctx())
                        })
                        .drop_without_applying_deltas();
                    }
                    ctx.run_ui(fixture_input(native / zoom, dpi, false), |ui| {
                        app.close(ui.ctx())
                    })
                    .drop_without_applying_deltas();
                    let start = std::time::Instant::now();
                    while app.saving {
                        assert!(start.elapsed() < std::time::Duration::from_secs(5));
                        app.receive(&ctx);
                        std::thread::sleep(std::time::Duration::from_millis(2));
                    }
                    let saved = SettingsStore::new(temp.path().into()).load().unwrap();
                    assert!(
                        (saved.get("WindowWidth").unwrap().as_f64().unwrap() - 1100.).abs() < 0.01
                    );
                    assert!(
                        (saved.get("WindowHeight").unwrap().as_f64().unwrap() - 740.).abs() < 0.01
                    );
                    assert_eq!(
                        saved.get("RustSidebarRatio"),
                        Some(&serde_json::json!(0.21))
                    );
                    assert_eq!(
                        saved.get("RustDetailsRatio"),
                        Some(&serde_json::json!(0.31))
                    );
                    assert_eq!(saved_layout(&saved, "StashWindow").unwrap()["Width"], 950);
                    assert_eq!(
                        saved_layout(&saved, "MainWindow").unwrap()["Panes"][0]["Key"],
                        "legacy-pane"
                    );
                }
            }
        }
    }

    #[test]
    fn native_window_geometry_ignores_transition_and_maximized_rectangles() {
        let ctx = egui::Context::default();
        let mut geometry = Geometry::default();
        let normal = egui::vec2(1100., 740.);
        for _ in 0..2 {
            ctx.run_ui(fixture_input(normal, 1., false), |ui| {
                geometry.observe(ui.ctx())
            })
            .drop_without_applying_deltas();
        }
        ctx.set_zoom_factor(1.1);
        // egui-winit can supply an old-zoom rectangle during this first new-zoom pass.
        ctx.run_ui(fixture_input(normal, 1., false), |ui| {
            geometry.observe(ui.ctx())
        })
        .drop_without_applying_deltas();
        assert_eq!(geometry.native_size, Some(normal));
        ctx.run_ui(fixture_input(egui::vec2(1800., 1000.), 1., true), |ui| {
            geometry.observe(ui.ctx())
        })
        .drop_without_applying_deltas();
        assert_eq!(geometry.native_size, Some(normal));
        ctx.run_ui(fixture_input(normal / 1.1, 2., false), |ui| {
            geometry.observe(ui.ctx())
        })
        .drop_without_applying_deltas();
        assert!((geometry.native_size.unwrap() - normal).length() < 0.01);
    }
}
