//! Verified, repository-owned recovery controls in History and Local Changes.
use super::*;
use shovel_core::{
    git::Snapshot,
    recovery,
    scheduler::{Lane, Scheduler},
};

pub(super) fn continue_label(kind: recovery::Kind) -> String {
    strings::format(
        "RecoveryContinueOperation",
        &[&branches::recovery_name(kind)],
    )
}

pub(super) fn sequence_busy(snapshot: &Snapshot, scheduler: &Scheduler) -> bool {
    let common = shovel_core::case::path_key(&snapshot.common_directory);
    scheduler
        .active_records()
        .iter()
        .any(|record| record.lane == Lane::Exclusive && record.common == common)
}

pub(super) fn submit_continue(
    operations: &mut operations::Controller,
    snapshot: &Snapshot,
    git: &Arc<Git>,
    scheduler: &Scheduler,
    ctx: &egui::Context,
) {
    if !sequence_busy(snapshot, scheduler) {
        operations.submit_recovery(
            "ActionTitleContinue",
            recovery::Action::Continue,
            snapshot,
            git.clone(),
            scheduler,
            ctx,
        );
    }
}

impl Workspace {
    pub(super) fn show_recovery_banner(&mut self, ui: &mut egui::Ui, enabled: bool) {
        let Some(tab) = self.tabs.get(self.selected) else {
            return;
        };
        let Some(history) = &tab.history else { return };
        let snapshot = &history.snapshot;
        if snapshot.operations.is_empty() || tab.view.identity_blocked {
            return;
        }
        let palette = workspace_style::Palette::current(ui);
        let busy = sequence_busy(snapshot, &self.scheduler);
        let conflicts = if tab.loading || tab.view.stale || busy {
            None
        } else {
            self.tab_markers.verified_conflicts(tab)
        };
        let mut resolve = false;
        egui::Panel::top("repository-recovery")
            .frame(workspace_style::pane(palette.chrome, 6, 3))
            .show(ui, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    let Some(state) = &snapshot.recovery else {
                        ui.label(strings::text(if snapshot.operations == ["bisect"] {
                            "RepositoryBisectActive"
                        } else {
                            "RecoveryUnavailable"
                        }));
                        return;
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(strings::format(
                            "RecoverySequencePaused",
                            &[&branches::recovery_name(state.kind)],
                        ));
                        match conflicts {
                            Some(true) => {
                                let (fill, text) = if palette.high_contrast {
                                    (palette.active_selection, palette.selection_text)
                                } else if ui.visuals().dark_mode {
                                    (palette.accent, palette.chrome)
                                } else {
                                    (palette.accent, palette.selection_text)
                                };
                                resolve = ui
                                    .add_enabled(
                                        self.git.is_some() && self.draft_store.is_some(),
                                        egui::Button::new(
                                            RichText::new(strings::text("ConflictResolve"))
                                                .strong()
                                                .color(text),
                                        )
                                        .fill(fill),
                                    )
                                    .clicked();
                            }
                            Some(false) => {
                                if ui
                                    .add_enabled(
                                        self.git.is_some(),
                                        egui::Button::new(continue_label(state.kind)),
                                    )
                                    .on_hover_text(strings::text("RecoveryContinueHelp"))
                                    .clicked()
                                    && let Some(git) = &self.git
                                {
                                    submit_continue(
                                        &mut self.operations,
                                        snapshot,
                                        git,
                                        &self.scheduler,
                                        ui.ctx(),
                                    );
                                }
                            }
                            None => {
                                ui.weak(strings::text("RecoveryStatusChecking"));
                            }
                        }
                        if ui
                            .add_enabled(
                                !busy && self.git.is_some(),
                                egui::Button::new(strings::text("ActionTitleAbort")),
                            )
                            .on_hover_text(strings::text("RecoveryAbortHelp"))
                            .clicked()
                        {
                            self.branches
                                .confirm_recovery(snapshot, recovery::Action::Abort);
                        }
                    });
                });
            });
        if resolve {
            self.open_commit();
            let tab = &self.tabs[self.selected];
            if let Some(editor) = self
                .editors
                .iter_mut()
                .find(|editor| editor.open && tab.owns_editor(editor))
            {
                editor.reveal_conflict();
            }
            ui.ctx().request_repaint();
        }
    }
}
