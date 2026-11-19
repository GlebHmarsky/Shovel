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
    OpenTool(u64, &'static str),
    Refresh(u64),
    Close(u64),
    CloseOthers(u64),
    Move(u64, isize),
    Marker(u64, Option<tab_markers::Color>),
}

impl Tab {
    pub(super) fn name(&self) -> std::borrow::Cow<'_, str> {
        self.sidebar.submodule_name
            .as_deref()
            .or(self.qualified_name.as_deref())
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
    pub(super) fn recent_repositories(&self) -> Vec<PathBuf> {
        self.settings.get("RecentRepositoryPaths")
            .and_then(serde_json::Value::as_array)
            .map(|paths| paths.iter().filter_map(serde_json::Value::as_str).take(12).map(PathBuf::from).collect())
            .unwrap_or_else(|| self.settings.repositories().into_iter().take(12).collect())
    }

    pub(super) fn remember_recent_repository(&mut self, path: &std::path::Path) {
        let mut recent = self.recent_repositories();
        if self.settings.get("RecentRepositoryPaths").is_some()
            && recent.first().is_some_and(|previous| Self::same_path(previous, path))
        {
            return;
        }
        recent.retain(|previous| !Self::same_path(previous, path));
        recent.insert(0, path.to_owned());
        recent.truncate(12);
        self.settings.set("RecentRepositoryPaths", serde_json::json!(recent));
        self.preferences_dirty = true;
    }

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
        if self.selected != index
            && let Some(previous) = self.tabs.get_mut(self.selected)
        {
            previous.view.reopen_commit_owner = None;
        }
        self.selected = index;
        if self.tabs[index].history.is_some() && !self.tabs[index].view.identity_blocked {
            let path = self.tabs[index].path.clone();
            self.remember_recent_repository(&path);
        }
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
        self.close_tabs(|tab| tab == id, ctx);
    }

    fn close_tabs(&mut self, close: impl Fn(u64) -> bool, ctx: &egui::Context) {
        let selected = self.tabs.get(self.selected).map(|t| t.id);
        let next = self.tabs[..self.selected.min(self.tabs.len())]
            .iter()
            .filter(|tab| !close(tab.id))
            .count();
        let closing: Vec<_> = self.tabs.extract_if(.., |tab| close(tab.id)).collect();
        if closing.is_empty() {
            return;
        }
        self.update_tab_names();
        for mut owner in closing {
            owner.view.reopen_commit_owner = None;
            self.tab_markers.forget(owner.id);
            if let Some(snapshot) = owner.observed_snapshot() {
                self.auto_fetch.forget(&snapshot.worktree);
            } else {
                self.auto_fetch.forget(&owner.path);
            }
            owner.locate_cancel.cancel();
            owner.locating = None;
            self.retained_tabs.push(owner);
        }
        self.tab_drag = None;
        if let Some(index) = self.tabs.iter().position(|t| Some(t.id) == selected) {
            self.selected = index;
            self.remember_tabs();
        } else {
            self.selected = next.min(self.tabs.len().saturating_sub(1));
            if self.tabs.is_empty() {
                self.history_menu = None;
                self.reference_menu = None;
                self.sync_inline_editor(ctx);
                self.remember_tabs();
            } else {
                self.select_tab(self.selected, ctx);
            }
        }
        self.prune_retained_tabs();
    }

    pub(super) fn tab_action(&mut self, action: workspace_tabs::TabAction, ctx: &egui::Context) {
        use workspace_tabs::TabAction;
        match action {
            TabAction::OpenTool(id, kind) => {
                if let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) {
                    let path = tab.path.clone();
                    let settings = self.settings.clone();
                    self.begin_tool_handoff(ctx, move || {
                        repository_tools::launch(kind, &path, &settings)
                    });
                }
            }
            TabAction::Marker(id, marker) => self.set_tab_marker(id, marker, ctx),
            TabAction::Refresh(id) => {
                if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
                    self.tab_markers.invalidate(id);
                    self.refresh_at(index, ctx);
                }
            }
            TabAction::Close(id) => self.close_tab(id, ctx),
            TabAction::CloseOthers(id) => {
                if self.tabs.iter().any(|t| t.id == id) {
                    self.close_tabs(|tab| tab != id, ctx);
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
        // Retain observation for the three most recently closed tabs. Accepted
        // owners can outlive this allowance without keeping OS watchers open.
        for owner in self.retained_tabs.iter_mut().rev().skip(3) {
            if owner.view.watch.take().is_some() {
                owner.view.stale = true;
            }
        }
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
