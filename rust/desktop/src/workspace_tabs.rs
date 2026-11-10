//! Repository owners outlive their visible tabs. Closing only releases display reads.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum TabAction {
    Refresh(u64),
    Close(u64),
    CloseOthers(u64),
    Move(u64, isize),
}

impl Workspace {
    pub(super) fn same_path(a: &std::path::Path, b: &std::path::Path) -> bool {
        a.to_string_lossy().replace('/', "\\").to_uppercase()
            == b.to_string_lossy().replace('/', "\\").to_uppercase()
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
        egui::Popup::close_all(ctx);
        self.history_menu = None;
        self.reference_menu = None;
        // Reattach to the same initialization; reopening is not a second request.
        if !self.tabs[index].loading
            && (self.tabs[index].history.is_none() || self.tabs[index].refresh_again)
        {
            self.tabs[index].refresh_again = false;
            self.refresh_at(index, ctx);
        }
        self.remember_tabs();
    }

    pub(super) fn close_tab(&mut self, id: u64, ctx: &egui::Context) {
        let Some(index) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        let selected = self.tabs.get(self.selected).map(|t| t.id);
        let mut owner = self.tabs.remove(index);
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
        }
        self.remember_tabs();
        self.prune_retained_tabs();
    }

    pub(super) fn tab_action(&mut self, action: workspace_tabs::TabAction, ctx: &egui::Context) {
        use workspace_tabs::TabAction;
        match action {
            TabAction::Refresh(id) => {
                if let Some(index) = self.tabs.iter().position(|t| t.id == id) {
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
        let records = self.scheduler.records();
        // Keep the three most recently closed idle owners. Active owners never
        // compete for that allowance, and initialization always receives its result.
        let mut idle = 0;
        let mut remove = Vec::new();
        for (index, owner) in self.retained_tabs.iter().enumerate().rev() {
            let active = owner.loading
                || self.operations.has_pending(&owner.path)
                || owner.history.as_ref().is_some_and(|history| {
                    records.iter().any(|r| {
                        r.outcome.is_none() && r.repository == history.snapshot.identity.fingerprint
                    })
                })
                || self
                    .editors
                    .iter()
                    .any(|e| Self::same_path(&e.path, &owner.path) && !e.can_retire())
                || self
                    .stash_views
                    .iter()
                    .any(|v| Self::same_path(&v.path, &owner.path) && !v.can_retire());
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
            self.stash_views
                .retain(|v| !Self::same_path(&v.path, &owner.path));
        }
    }
}
