//! Repository owners outlive their visible tabs. Closing only releases display reads.
use super::*;

pub(super) fn paint_drag(
    ctx: &egui::Context,
    slot: egui::Rect,
    pointer: egui::Pos2,
    label: &str,
    palette: workspace_style::Palette,
) {
    let screen = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::unique("repository-tab-drag"),
    ));
    let tint = if palette.high_contrast {
        palette.active_selection
    } else {
        palette.active_selection.gamma_multiply(0.55)
    };
    painter.rect_filled(slot, 4., tint);
    painter.rect_stroke(
        slot,
        4.,
        egui::Stroke::new(2., palette.focus),
        egui::StrokeKind::Inside,
    );
    let text = painter.layout_no_wrap(
        crate::path_text::PathText::new(ctx, label)
            .visual()
            .to_owned(),
        egui::FontId::proportional(13.),
        palette.text,
    );
    let size = (text.size() + egui::vec2(18., 10.)).min(screen.size());
    let position = (pointer + egui::vec2(12., 14.))
        .min(screen.max - size)
        .max(screen.min);
    let card = egui::Rect::from_min_size(position, size);
    painter.rect_filled(card, 4., palette.raised);
    painter.rect_stroke(
        card,
        4.,
        egui::Stroke::new(1., palette.border),
        egui::StrokeKind::Inside,
    );
    painter
        .with_clip_rect(card)
        .galley(card.min + egui::vec2(9., 5.), text, palette.text);
}

#[derive(Clone, Copy)]
pub(super) enum TabAction {
    Refresh(u64),
    Close(u64),
    CloseOthers(u64),
    Move(u64, isize),
    Marker(u64, Option<tab_markers::Color>),
}

impl Tab {
    pub(super) fn name(&self) -> std::borrow::Cow<'_, str> {
        self.qualified_name
            .as_deref()
            .map(std::borrow::Cow::Borrowed)
            .unwrap_or_else(|| {
                self.path
                    .file_name()
                    .unwrap_or(self.path.as_os_str())
                    .to_string_lossy()
            })
    }
    pub(super) fn owns_editor(&self, editor: &commit::Editor) -> bool {
        self.matches_path(&editor.path)
            && editor.matches_owner(
                self.observed_snapshot()
                    .map(|s| s.identity.fingerprint.as_str()),
            )
    }
    pub(super) fn matches_path(&self, path: &std::path::Path) -> bool {
        Workspace::same_path(&self.path, path)
            || self
                .observed_snapshot()
                .is_some_and(|snapshot| Workspace::same_path(&snapshot.worktree, path))
    }
}

impl Workspace {
    /// Adjacent paths in reversed-component order share the longest suffix.
    /// Rebuild on membership changes so ordinary frames only read the label.
    pub(super) fn update_tab_names(&mut self) {
        let mut paths: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let components: Vec<_> = tab
                    .path
                    .components()
                    .rev()
                    .map(|c| shovel_core::case::path_key(c.as_ref()))
                    .collect();
                (components, i)
            })
            .collect();
        paths.sort_unstable();
        for (position, (parts, id)) in paths.iter().enumerate() {
            let shared = [position.checked_sub(1), Some(position + 1)]
                .into_iter()
                .flatten()
                .filter_map(|other| paths.get(other))
                .map(|(other, _)| parts.iter().zip(other).take_while(|(a, b)| a == b).count())
                .max()
                .unwrap_or(0);
            let tab = &mut self.tabs[*id];
            tab.qualified_name = (shared > 0).then(|| {
                let parts: Vec<_> = tab.path.components().rev().take(shared + 1).collect();
                parts
                    .into_iter()
                    .rev()
                    .collect::<PathBuf>()
                    .to_string_lossy()
                    .into_owned()
            });
        }
    }
    pub(super) fn same_path(a: &std::path::Path, b: &std::path::Path) -> bool {
        a == b || shovel_core::case::path_key(a) == shovel_core::case::path_key(b)
    }

    pub(super) fn remember_tabs(&mut self) {
        self.settings.set(
            "RepositoryPaths",
            serde_json::json!(self.tabs.iter().map(|t| &t.path).collect::<Vec<_>>()),
        );
        self.settings.set(
            "SelectedRepositoryPath",
            serde_json::json!(self.tabs.get(self.selected).map(|t| &t.path)),
        );
        self.preferences_dirty = true;
    }

    pub(super) fn select_tab(&mut self, index: usize, ctx: &egui::Context) {
        if index >= self.tabs.len() {
            return;
        }
        self.selected = index;
        self.tab_markers.invalidate(self.tabs[index].id);
        self.tabs[index].view.last_use = Some(std::time::Instant::now());
        egui::Popup::close_all(ctx);
        self.history_menu = None;
        self.reference_menu = None;
        self.sync_inline_editor(ctx);
        // Reattach to the same initialization; reopening is not a second request.
        self.activate_history(index, ctx);
        self.remember_tabs();
    }

    pub(super) fn close_tab(&mut self, id: u64, ctx: &egui::Context) {
        let Some(index) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        let selected = self.tabs.get(self.selected).map(|t| t.id);
        let mut owner = self.tabs.remove(index);
        self.update_tab_names();
        self.tab_markers.forget(id);
        if let Some(snapshot) = owner.observed_snapshot() {
            self.auto_fetch.forget(&snapshot.worktree);
        } else {
            self.auto_fetch.forget(&owner.path);
        }
        owner.locate_cancel.cancel();
        owner.locating = None;
        if let Some(pane) = &mut owner.comparison {
            pane.cancel();
        }
        owner.comparison = None;
        self.retained_tabs.push(owner);
        self.tab_drag = None;
        self.history_menu = None;
        self.reference_menu = None;
        self.selected = self
            .tabs
            .iter()
            .position(|t| Some(t.id) == selected)
            .unwrap_or_else(|| index.min(self.tabs.len().saturating_sub(1)));
        if !self.tabs.is_empty() {
            self.select_tab(self.selected, ctx);
        } else {
            self.sync_inline_editor(ctx);
        }
        self.remember_tabs();
        self.prune_retained_tabs();
    }

    pub(super) fn tab_action(&mut self, action: workspace_tabs::TabAction, ctx: &egui::Context) {
        use workspace_tabs::TabAction;
        match action {
            TabAction::Marker(id, marker) => self.set_tab_marker(id, marker, ctx),
            TabAction::Refresh(id) => {
                if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
                    self.tab_markers.invalidate(id);
                    self.refresh_at(index, ctx);
                }
            }
            TabAction::Close(id) => self.close_tab(id, ctx),
            TabAction::CloseOthers(id) => {
                let ids = self
                    .tabs
                    .iter()
                    .filter(|t| t.id != id)
                    .map(|t| t.id)
                    .collect::<Vec<_>>();
                for other in ids {
                    self.close_tab(other, ctx);
                }
            }
            TabAction::Move(id, delta) => {
                let Some(index) = self.tabs.iter().position(|t| t.id == id) else {
                    return;
                };
                let target =
                    (index as isize + delta).clamp(0, self.tabs.len() as isize - 1) as usize;
                let active = self.tabs[self.selected].id;
                let owner = self.tabs.remove(index);
                self.tabs.insert(target, owner);
                self.selected = self.tabs.iter().position(|t| t.id == active).unwrap();
                self.remember_tabs();
            }
        }
    }

    pub(super) fn prune_retained_tabs(&mut self) {
        if self.retained_tabs.len() <= 3 {
            return;
        }
        let records = self.scheduler.active_records();
        // Keep the three most recently closed idle owners. Active owners never
        // compete for that allowance, and initialization always receives its result.
        let mut idle = 0;
        let mut remove = Vec::new();
        for (index, owner) in self.retained_tabs.iter().enumerate().rev() {
            let active = owner.loading
                || self.operations.has_pending(&owner.path)
                || owner.observed_snapshot().is_some_and(|snapshot| {
                    records.iter().any(|r| r.repository == snapshot.identity.fingerprint)
                })
                || self
                    .editors
                    .iter()
                    .any(|e| Self::same_path(&e.path, &owner.path) && !e.can_retire())
                || self
                    .stash_views
                    .iter()
                    .any(|v| owner.matches_path(&v.path) && !v.can_retire());
            if !active {
                idle += 1;
                if idle > 3 {
                    remove.push(index);
                }
            }
        }
        for index in remove {
            let owner = self.retained_tabs.remove(index);
            self.editors
                .retain(|e| !Self::same_path(&e.path, &owner.path));
            self.stash_views.retain(|v| !owner.matches_path(&v.path));
        }
    }
}
