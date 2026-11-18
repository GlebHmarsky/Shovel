//! Persist workspace divider positions.
//! Drag previews stay in egui memory; disk writes occur only on release.
use eframe::egui;
use shovel_core::settings::Settings;
pub enum Divider {
    Sidebar,
    Details,
    HistoryFiles,
}
impl Divider {
    fn keys(&self) -> (&'static str, Option<&'static str>) {
        match self {
            Self::Sidebar => ("SidebarWidth", Some("RustSidebarRatio")),
            Self::Details => ("DetailsHeight", Some("RustDetailsRatio")),
            Self::HistoryFiles => ("HistoryFilesWidth", None),
        }
    }
    fn limits(&self, span: f32) -> (f32, f32, f32) {
        match self {
            Self::Sidebar => (210., 160., (span * 0.6).max(160.)),
            Self::Details => (180., 80., (span - 300.).max(80.)),
            Self::HistoryFiles => (340., 160., (span - 210.).clamp(160., 3000.)),
        }
    }
    pub fn size(&self, ui: &egui::Ui, settings: &Settings, span: f32) -> f32 {
        let (px, ratio) = self.keys();
        let (default, min, max) = self.limits(span);
        let current = ui.ctx().data(|d| d.get_temp::<f32>(egui::Id::unique(px)));
        current
            .or_else(|| {
                ratio
                    .and_then(|key| settings.get(key))
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
        let id = egui::Id::unique(px);
        let mut grip = panel;
        let cursor = match self {
            Self::Sidebar | Self::HistoryFiles => {
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
        let active = r.hovered() || r.dragged();
        if matches!(self, Self::HistoryFiles) && !active {
            ui.painter().vline(
                grip.center().x,
                grip.y_range(),
                egui::Stroke::new(1., palette.border),
            );
        } else {
            ui.painter().rect_filled(
                grip,
                0.,
                if active {
                    palette.focus
                } else {
                    palette.border
                },
            );
        }
        if (r.dragged() || r.drag_stopped())
            && let Some(pos) = r.interact_pointer_pos()
        {
            let size = match self {
                Self::Sidebar | Self::HistoryFiles => pos.x - panel.left() + 2.5,
                Self::Details => panel.bottom() - pos.y + 2.5,
            };
            let (_, min, max) = self.limits(span);
            let size = size.clamp(min, max);
            if r.drag_stopped() {
                if let Some(ratio) = ratio {
                    settings.set(ratio, serde_json::json!(size / span.max(1.)));
                }
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
