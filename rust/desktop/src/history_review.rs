//! Centered reviews for captured History actions.
use crate::{
    history_row,
    read_workers::Workers,
    strings::{self, text},
    workspace_style::{self, Palette},
};
use eframe::egui::{self, RichText};
use shovel_core::{
    git::{Commit, Git, Snapshot},
    history_actions::{Action, CherryPickPreview, Reset},
    process::Cancellation,
};
use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    thread,
};

pub(super) enum Decision {
    Visible,
    Close,
    Submit(Action),
}

pub(super) enum Review {
    CherryPick(Box<CherryPick>),
    UndoLast(UndoLast),
    Reset(Box<ResetReview>),
}

impl Review {
    pub fn label(&self) -> &'static str {
        match self {
            Self::CherryPick(_) => "HistoryCherryPick",
            Self::UndoLast(_) => "HistoryUndoLastCommit",
            Self::Reset(_) => "HistoryResetMenu",
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        git: &Arc<Git>,
        snapshot: &Snapshot,
        workers: &Workers,
        enabled: bool,
    ) -> Decision {
        match self {
            Self::CherryPick(review) => review.show(ctx, git, snapshot, workers, enabled),
            Self::UndoLast(review) => review.show(ctx, git, snapshot, workers, enabled),
            Self::Reset(review) => review.show(ctx, snapshot, enabled),
        }
    }
}

fn centered_review(
    ctx: &egui::Context,
    id: &'static str,
    title: &str,
    enabled: bool,
    body: impl FnOnce(&mut egui::Ui) -> Decision,
) -> Decision {
    let (decision, close) =
        workspace_style::show_dialog(ctx, egui::Id::unique(id), title, 442., enabled, true, body);
    if close { Decision::Close } else { decision }
}

fn footer(ui: &mut egui::Ui, title: &str, prepared: Option<Action>) -> Decision {
    workspace_style::dialog_actions(ui, |ui| {
        let size = egui::vec2(94., 29.);
        let cancel = ui.add(egui::Button::new(text("CancelOperation")).min_size(size));
        let submit = ui
            .add_enabled(
                prepared.is_some(),
                egui::Button::new(title).min_size(size).truncate(),
            )
            .clicked();
        if cancel.clicked() {
            Decision::Close
        } else if submit && let Some(action) = prepared {
            Decision::Submit(action)
        } else {
            Decision::Visible
        }
    })
}

fn short_oid(hash: &str) -> &str {
    hash.get(..8).unwrap_or(hash)
}

pub(super) fn commit_card(ui: &mut egui::Ui, commit: &Commit, snapshot: &Snapshot) {
    let palette = Palette::current(ui);
    egui::Frame::new()
        .fill(palette.raised)
        .stroke(egui::Stroke::new(1., palette.border))
        .inner_margin(9)
        .corner_radius(5)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                let (avatar, _) =
                    ui.allocate_exact_size(egui::Vec2::splat(32.), egui::Sense::hover());
                history_row::paint_avatar(ui, avatar, commit, &palette, &snapshot.worktree);
                ui.vertical(|ui| {
                    ui.add(egui::Label::new(RichText::new(commit.subject()).strong()).truncate())
                        .on_hover_text(commit.subject());
                    ui.horizontal_wrapped(|ui| {
                        ui.monospace(short_oid(&commit.hash))
                            .on_hover_text(&commit.hash);
                        ui.weak(&commit.author)
                            .on_hover_text(history_row::author_details(ui.ctx(), commit));
                        ui.weak(crate::dates::local(ui.ctx(), &commit.authored));
                    });
                });
            });
        });
}

fn repository_target(ui: &mut egui::Ui, snapshot: &Snapshot) {
    let path = snapshot.worktree.to_string_lossy();
    ui.add(egui::Label::new(path.as_ref()).truncate())
        .on_hover_text(path);
    if let (Some(branch), Some(head)) = (&snapshot.branch, &snapshot.head) {
        ui.label(strings::format(
            "HistoryResetCurrent",
            &[branch, short_oid(head)],
        ))
        .on_hover_text(head);
    }
}

pub(super) struct ResetReview {
    target: Commit,
    mode: Reset,
}

impl ResetReview {
    pub fn new(target: Commit) -> Self {
        Self {
            target,
            mode: Reset::Mixed,
        }
    }

    fn show(&mut self, ctx: &egui::Context, snapshot: &Snapshot, enabled: bool) -> Decision {
        centered_review(
            ctx,
            "history-reset-review",
            &text("HistoryResetTitle"),
            enabled,
            |ui| {
                workspace_style::dialog_body(ui, |ui| {
                    repository_target(ui, snapshot);
                    ui.label(RichText::new(text("HistoryResetDestination")).strong());
                    commit_card(ui, &self.target, snapshot);
                    let palette = Palette::current(ui);
                    let amber = Palette::change_status('M', ui.visuals().dark_mode);
                    for (mode, label, description, color) in [
                        (
                            Reset::Soft,
                            "HistoryResetSoft",
                            "HistoryEffectResetSoft",
                            palette.diff_added_text,
                        ),
                        (
                            Reset::Mixed,
                            "HistoryResetMixed",
                            "HistoryEffectResetMixed",
                            amber,
                        ),
                        (
                            Reset::Hard,
                            "HistoryResetHard",
                            "HistoryEffectResetHard",
                            palette.diff_removed_text,
                        ),
                    ] {
                        let color = if palette.high_contrast {
                            palette.text
                        } else {
                            color
                        };
                        ui.radio_value(
                            &mut self.mode,
                            mode,
                            RichText::new(text(label)).strong().color(color),
                        );
                        ui.indent(label, |ui| {
                            ui.weak(text(description));
                        });
                    }
                    if self.mode == Reset::Hard {
                        ui.colored_label(
                            palette.diff_removed_text,
                            text("HistoryResetHardWarning"),
                        );
                    }
                });
                let branch = snapshot.branch.as_deref().unwrap_or_default();
                let label = if self.mode == Reset::Hard {
                    strings::format("HistoryResetHardSubmit", &[branch])
                } else {
                    strings::format("HistoryResetSubmit", &[branch])
                };
                footer(
                    ui,
                    &label,
                    (snapshot.branch.is_some() && snapshot.head.is_some()).then(|| Action::Reset {
                        target: self.target.hash.clone(),
                        mode: self.mode,
                    }),
                )
            },
        )
    }
}

fn spawn_read<T: Send + 'static>(
    ctx: &egui::Context,
    workers: &Workers,
    cancel: Cancellation,
    work: impl FnOnce(&Cancellation) -> T + Send + 'static,
) -> Option<Receiver<T>> {
    let (sender, receiver) = mpsc::channel();
    let ctx = ctx.clone();
    let lease = workers.start(cancel.clone());
    thread::Builder::new()
        .spawn(move || {
            let _lease = lease;
            let _ = sender.send(work(&cancel));
            ctx.request_repaint();
        })
        .ok()?;
    Some(receiver)
}

pub(super) struct CherryPick {
    source: Commit,
    mainline: usize,
    commit: bool,
    append_origin: bool,
    receiver: Option<Receiver<CherryPickPreview>>,
    preview: Option<CherryPickPreview>,
    cancel: Cancellation,
}

impl CherryPick {
    pub fn new(source: Commit, mainline: usize) -> Self {
        Self {
            source,
            mainline,
            commit: true,
            append_origin: false,
            receiver: None,
            preview: None,
            cancel: Cancellation::default(),
        }
    }

    fn poll(
        &mut self,
        ctx: &egui::Context,
        git: &Arc<Git>,
        snapshot: &Snapshot,
        workers: &Workers,
    ) {
        if self.preview.is_some() || workers.stopping() {
            return;
        }
        if let Some(receiver) = &self.receiver {
            self.preview = match receiver.try_recv() {
                Ok(preview) => Some(preview),
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => Some(CherryPickPreview::Unavailable),
            };
            self.receiver = None;
            return;
        }
        let (source, mainline, git, snapshot) = (
            self.source.clone(),
            self.mainline,
            git.clone(),
            snapshot.clone(),
        );
        self.receiver = spawn_read(ctx, workers, self.cancel.clone(), move |cancel| {
            #[cfg(test)]
            crate::history_cache::tests::wait_for_read(&snapshot.worktree);
            git.preview_cherry_pick(&snapshot, &source, mainline, cancel)
        });
        if self.receiver.is_none() {
            self.preview = Some(CherryPickPreview::Unavailable);
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        git: &Arc<Git>,
        snapshot: &Snapshot,
        workers: &Workers,
        enabled: bool,
    ) -> Decision {
        self.poll(ctx, git, snapshot, workers);
        let title = text("HistoryCherryPick");
        centered_review(ctx, "cherry-pick-review", &title, enabled, |ui| {
            workspace_style::dialog_body(ui, |ui| {
                let palette = Palette::current(ui);
                commit_card(ui, &self.source, snapshot);
                if let Some(parent) = self
                    .mainline
                    .checked_sub(1)
                    .and_then(|index| self.source.parents.get(index))
                {
                    ui.weak(strings::format(
                        "CherryPickParent",
                        &[&self.mainline.to_string(), short_oid(parent)],
                    ));
                }
                let destination = snapshot
                    .branch
                    .as_deref()
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        let head = snapshot.head.as_deref().unwrap_or_default();
                        strings::format("DetachedHead", &[short_oid(head)])
                    });
                ui.label(strings::format(
                    "HistoryReviewDestination",
                    &[&destination, &snapshot.worktree.to_string_lossy()],
                ));
                ui.separator();
                workspace_style::dialog_checkbox(
                    ui,
                    &mut self.commit,
                    text("CherryPickCommitChanges"),
                );
                ui.add_enabled_ui(self.commit, |ui| {
                    workspace_style::dialog_checkbox(
                        ui,
                        &mut self.append_origin,
                        text("CherryPickAppendOrigin"),
                    );
                });
                let (message, color) = match self.preview {
                    None => ("CherryPickChecking", palette.muted),
                    Some(CherryPickPreview::Clean) => ("CherryPickPreviewClean", palette.muted),
                    Some(CherryPickPreview::NoChanges) => {
                        ("CherryPickPreviewNoChanges", palette.accent)
                    }
                    Some(CherryPickPreview::Conflicts) => {
                        ("CherryPickPreviewConflicts", palette.accent)
                    }
                    Some(CherryPickPreview::Unavailable) => {
                        ("CherryPickPreviewUnavailable", palette.muted)
                    }
                    Some(CherryPickPreview::Stale) => ("CherryPickPreviewStale", palette.accent),
                };
                ui.colored_label(color, text(message));
            });
            let ready = self
                .preview
                .is_some_and(|preview| preview != CherryPickPreview::Stale);
            footer(
                ui,
                &title,
                ready.then(|| Action::CherryPick {
                    target: self.source.hash.clone(),
                    mainline: self.mainline,
                    commit: self.commit,
                    append_origin: self.commit && self.append_origin,
                }),
            )
        })
    }
}

impl Drop for CherryPick {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

struct UndoTarget {
    hash: String,
    commit: Option<Box<Commit>>,
}

#[derive(Default)]
pub(super) struct UndoLast {
    receiver: Option<Receiver<Result<UndoTarget, String>>>,
    target: Option<Result<UndoTarget, String>>,
    cancel: Cancellation,
}

impl UndoLast {
    fn poll(
        &mut self,
        ctx: &egui::Context,
        git: &Arc<Git>,
        snapshot: &Snapshot,
        workers: &Workers,
    ) {
        if self.target.is_some() || workers.stopping() {
            return;
        }
        if let Some(receiver) = &self.receiver {
            self.target = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("HistoryTargetUnavailable".into()))
                }
            };
            self.receiver = None;
            return;
        }
        let (git, snapshot) = (git.clone(), snapshot.clone());
        self.receiver = spawn_read(ctx, workers, self.cancel.clone(), move |cancel| {
            #[cfg(test)]
            crate::history_cache::tests::wait_for_read(&snapshot.worktree);
            git.prepare_undo_last_commit(&snapshot, cancel)
                .map(|hash| UndoTarget {
                    commit: git.history_commit(&snapshot, &hash, cancel).map(Box::new),
                    hash,
                })
                .map_err(|error| error.code())
        });
        if self.receiver.is_none() {
            self.target = Some(Err("HistoryTargetUnavailable".into()));
        }
    }

    fn show(
        &mut self,
        ctx: &egui::Context,
        git: &Arc<Git>,
        snapshot: &Snapshot,
        workers: &Workers,
        enabled: bool,
    ) -> Decision {
        self.poll(ctx, git, snapshot, workers);
        centered_review(
            ctx,
            "undo-last-commit",
            &text("HistoryUndoLastCommit"),
            enabled,
            |ui| {
                let target = workspace_style::dialog_body(ui, |ui| {
                    repository_target(ui, snapshot);
                    match &self.target {
                        Some(Ok(target)) => {
                            ui.label(RichText::new(text("HistoryResetDestination")).strong());
                            if let Some(commit) = &target.commit {
                                commit_card(ui, commit, snapshot);
                            } else {
                                ui.monospace(short_oid(&target.hash))
                                    .on_hover_text(&target.hash);
                            }
                            ui.label(text("HistoryUndoEffect"));
                            Some(Action::Reset {
                                target: target.hash.clone(),
                                mode: Reset::Soft,
                            })
                        }
                        None => {
                            ui.weak(text("HistoryUndoChecking"));
                            None
                        }
                        Some(Err(error)) => {
                            ui.label(text(error));
                            None
                        }
                    }
                });
                footer(ui, &text("HistoryUndoSubmit"), target)
            },
        )
    }
}

impl Drop for UndoLast {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
