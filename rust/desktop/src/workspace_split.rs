//! Persist the user's divider position as a fraction of its available space.
//! Drag previews stay in egui memory; disk writes occur only on release.
use eframe::egui;
use shovel_core::settings::Settings;
pub enum Divider {
    Sidebar,
    Details,
}
impl Divider {
    fn keys(&self) -> (&'static str, &'static str) {
        match self {
            Self::Sidebar => ("SidebarWidth", "RustSidebarRatio"),
            Self::Details => ("DetailsHeight", "RustDetailsRatio"),
        }
    }
    pub fn size(&self, ui: &egui::Ui, settings: &Settings, span: f32) -> f32 {
        let (px, ratio) = self.keys();
        let (default, min, max) = match self {
            Self::Sidebar => (210., 160., (span * 0.6).max(160.)),
            Self::Details => (180., 80., (span - 300.).max(80.)),
        };
        let current = ui
            .ctx()
            .data(|d| d.get_temp::<f32>(egui::Id::unique(ratio)));
        current
            .or_else(|| {
                settings
                    .get(ratio)
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32 * span)
            })
            .or_else(|| settings.get(px).and_then(|v| v.as_f64()).map(|v| v as f32))
            .unwrap_or(default)
            .clamp(min, max)
    }
    pub fn handle(
        &self,
        ui: &egui::Ui,
        panel: egui::Rect,
        span: f32,
        settings: &mut Settings,
    ) -> bool {
        let (px, ratio) = self.keys();
        let id = egui::Id::unique(ratio);
        let mut grip = panel;
        let cursor = match self {
            Self::Sidebar => {
                grip.min.x = grip.max.x - 5.;
                egui::CursorIcon::ResizeHorizontal
            }
            Self::Details => {
                grip.max.y = grip.min.y + 5.;
                egui::CursorIcon::ResizeVertical
            }
        };
        let r = ui
            .interact(grip, id, egui::Sense::drag())
            .on_hover_cursor(cursor);
        let palette = crate::workspace_style::Palette::current(ui);
        ui.painter().rect_filled(
            grip,
            0.,
            if r.hovered() || r.dragged() {
                palette.focus
            } else {
                palette.border
            },
        );
        if (r.dragged() || r.drag_stopped())
            && let Some(pos) = r.interact_pointer_pos()
        {
            let size = match self {
                Self::Sidebar => (pos.x - panel.left() + 2.5).clamp(160., (span * 0.6).max(160.)),
                Self::Details => (panel.bottom() - pos.y + 2.5).clamp(80., (span - 300.).max(80.)),
            };
            if r.drag_stopped() {
                settings.set(ratio, serde_json::json!(size / span.max(1.)));
                settings.set(px, serde_json::json!(size));
                ui.ctx().data_mut(|d| d.remove::<f32>(id));
            } else {
                ui.ctx().data_mut(|d| d.insert_temp(id, size));
            }
            ui.ctx().request_repaint();
            return r.drag_stopped();
        }
        false
    }
}
