//! Repository-owned Local Changes state. The native window is only a view;
//! closing it never drops accepted work, draft saves, or reconciliation.
use crate::strings::{self, text};
use eframe::egui::{self, Color32, RichText};
use shovel_core::{
    drafts,
    git::Git,
    identity::{self, Identity},
    process::{Cancellation, OutputCallback},
    scheduler::{Lane, Outcome, Scheduler},
    working::{self, Action, Change, Context, Group},
};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

enum Event {
    Loaded(u64, Result<Context, working::Error>),
    Diff(u64, Result<String, working::Error>),
    DraftLoaded(Result<(Identity, Result<drafts::Snapshot, drafts::Error>), drafts::Error>),
    DraftSaved(u64, Result<drafts::Snapshot, String>),
    Started,
    Output(bool, Vec<u8>),
    Completed(
        Result<working::Completed, working::Error>,
        Option<drafts::Text>,
    ),
    Removed,
}
pub struct Editor {
    pub path: PathBuf,
    pub open: bool,
    pub context: Option<Arc<Context>>,
    pub failed_commit: bool,
    pub status: Option<String>,
    pub output: String,
    pub busy: bool,
    pub size: egui::Vec2,
    git: Arc<Git>,
    scheduler: Scheduler,
    store: drafts::Store,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    initialized: bool,
    loading: bool,
    generation: u64,
    diff_generation: u64,
    diff_cancel: Cancellation,
    selection: BTreeSet<(Group, String)>,
    focused: Option<Change>,
    diff: Option<String>,
    diff_error: Option<String>,
    draft: drafts::Text,
    identity: Option<Identity>,
    revision: Option<String>,
    draft_loaded: bool,
    draft_sequence: u64,
    draft_dirty: bool,
    draft_saving: bool,
    draft_error: Option<String>,
    draft_saved_at: Instant,
    recovery_id: String,
    amend: bool,
    confirm_amend: bool,
    refresh_history: bool,
}
impl Editor {
    pub fn new(path: PathBuf, git: Arc<Git>, scheduler: Scheduler, store: drafts::Store) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            path,
            open: true,
            context: None,
            failed_commit: false,
            status: None,
            output: String::new(),
            busy: false,
            size: egui::vec2(1100., 760.),
            git,
            scheduler,
            store,
            sender,
            receiver,
            initialized: false,
            loading: false,
            generation: 0,
            diff_generation: 0,
            diff_cancel: Cancellation::default(),
            selection: BTreeSet::new(),
            focused: None,
            diff: None,
            diff_error: None,
            draft: drafts::Text::default(),
            identity: None,
            revision: None,
            draft_loaded: false,
            draft_sequence: 0,
            draft_dirty: false,
            draft_saving: false,
            draft_error: None,
            draft_saved_at: Instant::now(),
            recovery_id: uuid::Uuid::new_v4().simple().to_string(),
            amend: false,
            confirm_amend: false,
            refresh_history: false,
        }
    }
    pub fn activate(&mut self) {
        self.open = true;
    }
    pub fn blocks_exit(&self) -> bool {
        self.draft_dirty || self.draft_saving || self.busy
    }
    pub fn draft_problem(&self) -> Option<&str> {
        self.draft_error.as_deref()
    }
    fn initialize(&mut self, ctx: &egui::Context) {
        self.initialized = true;
        self.refresh(ctx);
        let path = self.path.clone();
        let store = self.store.clone();
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let cancel = Cancellation::default();
            let result = identity::read(&path)
                .map_err(drafts::Error::Io)
                .map(|identity| {
                    let draft = store.load(&identity, &cancel);
                    (identity, draft)
                });
            let _ = tx.send(Event::DraftLoaded(result));
            ctx.request_repaint();
        });
    }
    pub fn refresh(&mut self, ctx: &egui::Context) {
        if self.loading || self.busy {
            return;
        }
        self.loading = true;
        self.generation += 1;
        let generation = self.generation;
        let git = self.git.clone();
        let path = self.path.clone();
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = git.working_copy(&path, &Cancellation::default());
            let _ = tx.send(Event::Loaded(generation, result));
            ctx.request_repaint();
        });
    }
    fn show_diff(&mut self, entry: Change, ctx: &egui::Context) {
        let Some(context) = self.context.clone() else {
            return;
        };
        self.diff_cancel.cancel();
        self.diff_cancel = Cancellation::default();
        self.diff_generation += 1;
        self.focused = Some(entry.clone());
        self.diff = None;
        self.diff_error = None;
        let generation = self.diff_generation;
        let cancel = self.diff_cancel.clone();
        let git = self.git.clone();
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = git.working_diff(&context, &entry, &cancel);
            let _ = tx.send(Event::Diff(generation, result));
            ctx.request_repaint();
        });
    }
    fn edited(&mut self) {
        self.draft_sequence += 1;
        self.draft_dirty = true;
        self.draft_error = None;
        self.draft_saved_at = Instant::now();
    }
    pub fn flush(&mut self, ctx: &egui::Context) {
        if !self.draft_dirty
            || self.draft_saving
            || !self.draft_loaded
            || self.draft_error.is_some()
        {
            return;
        }
        let Some(identity) = self.identity.clone() else {
            return;
        };
        self.draft_saving = true;
        let sequence = self.draft_sequence;
        let draft = self.draft.clone();
        let expected = self.revision.clone();
        let store = self.store.clone();
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        let copy = self.recovery_id.clone();
        thread::spawn(move || {
            let cancel = Cancellation::default();
            let result = store
                .save(&identity, expected.as_deref(), &draft, &cancel)
                .map_err(|error| {
                    let key = if store.recovery(&identity, &copy, &draft, &cancel).is_ok() {
                        "CommitDraftRecovery".to_owned()
                    } else {
                        error.to_string()
                    };
                    strings::format(&key, &[&store.directory.to_string_lossy()])
                });
            let _ = tx.send(Event::DraftSaved(sequence, result));
            ctx.request_repaint();
        });
    }
    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
        while let Ok(event) = self.receiver.try_recv() {
            match event {
                Event::Loaded(generation, result) if generation == self.generation => {
                    self.loading = false;
                    match result {
                        Ok(context) => self.apply(context, ctx),
                        Err(error) => self.status = Some(error_text(error)),
                    }
                }
                Event::Diff(generation, result) if generation == self.diff_generation => {
                    match result {
                        Ok(diff) => self.diff = Some(diff),
                        Err(error) => self.diff_error = Some(error_text(error)),
                    }
                }
                Event::DraftLoaded(result) => {
                    self.draft_loaded = true;
                    match result {
                        Ok((identity, result)) => {
                            self.identity = Some(identity);
                            match result {
                                Ok(snapshot) => {
                                    self.revision = snapshot.revision;
                                    if self.draft_sequence == 0 {
                                        self.draft = snapshot.text;
                                    }
                                    if snapshot.has_recovery {
                                        self.draft_error = Some(strings::format(
                                            "CommitDraftCopies",
                                            &[&self.store.directory.to_string_lossy()],
                                        ));
                                    }
                                }
                                Err(error) => {
                                    self.draft_error = Some(strings::format(
                                        &error.to_string(),
                                        &[&self.store.directory.to_string_lossy()],
                                    ));
                                }
                            }
                        }
                        Err(error) => {
                            self.draft_error = Some(strings::format(
                                &error.to_string(),
                                &[&self.store.directory.to_string_lossy()],
                            ))
                        }
                    }
                }
                Event::DraftSaved(sequence, result) => {
                    self.draft_saving = false;
                    match result {
                        Ok(snapshot) => {
                            self.revision = snapshot.revision;
                            if sequence == self.draft_sequence {
                                self.draft_dirty = false;
                            }
                        }
                        Err(error) => self.draft_error = Some(error),
                    }
                }
                Event::Started => {}
                Event::Output(stderr, bytes) => {
                    if self.output.len() < 1024 * 1024 {
                        let value = String::from_utf8_lossy(&bytes);
                        self.output.push_str(&value);
                        if stderr && !value.ends_with('\n') {
                            self.output.push('\n');
                        }
                    }
                }
                Event::Completed(result, submitted) => {
                    self.busy = false;
                    self.refresh_history = true;
                    match result {
                        Ok(result) => {
                            if result.succeeded() {
                                self.status = Some(
                                    if result.command.outcome
                                        == shovel_core::process::Outcome::OutputLimit
                                    {
                                        format!(
                                            "{} {}",
                                            text("ActionCompletedShort"),
                                            text("ActivityOutputLimit")
                                        )
                                    } else {
                                        text("ActionCompletedShort")
                                    },
                                );
                                self.failed_commit = false;
                                if let Some(submitted) = submitted
                                    && self.draft == submitted
                                {
                                    self.draft = drafts::Text::default();
                                    self.edited();
                                    self.amend = false;
                                }
                            } else {
                                self.status = Some(text("ActionFailedShort"));
                                self.failed_commit = submitted.is_some();
                            }
                            match result.updated {
                                Ok(context) => self.apply(context, ctx),
                                Err(error) => {
                                    self.status = Some(format!(
                                        "{}\n{}",
                                        self.status.clone().unwrap_or_default(),
                                        error_text(error)
                                    ))
                                }
                            }
                        }
                        Err(error) => {
                            self.status = Some(error_text(error));
                            self.failed_commit = submitted.is_some();
                        }
                    }
                }
                Event::Removed => {
                    self.busy = false;
                    self.status = Some(text("ActionStoppedShort"));
                }
                _ => {}
            }
        }
        if self.draft_dirty {
            ctx.request_repaint_after(Duration::from_millis(400));
            if self.draft_saved_at.elapsed() >= Duration::from_millis(400) {
                self.flush(ctx);
            }
        }
        std::mem::take(&mut self.refresh_history)
    }
    fn apply(&mut self, context: Context, ctx: &egui::Context) {
        self.selection.retain(|(group, path)| {
            context
                .entries
                .iter()
                .any(|e| &e.group == group && &e.path == path)
        });
        let focus = self
            .focused
            .as_ref()
            .and_then(|old| {
                context
                    .entries
                    .iter()
                    .find(|e| e.group == old.group && e.path == old.path)
            })
            .cloned();
        self.context = Some(Arc::new(context));
        if let Some(focus) = focus {
            self.show_diff(focus, ctx);
        } else {
            self.focused = None;
            self.diff = None;
            self.diff_error = None;
        }
    }
    fn submit(&mut self, action: Action, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        let Some(context) = self.context.clone() else {
            return;
        };
        self.busy = true;
        self.status = Some(text(match &action {
            Action::Stage(_) => "CommitPhaseStage",
            Action::Unstage(_) => "CommitPhaseUnstage",
            Action::Commit { .. } => "CommitPhaseCommit",
        }));
        self.output.clear();
        let committing = matches!(action, Action::Commit { .. });
        let submitted = if committing {
            self.flush(ctx);
            Some(self.draft.clone())
        } else {
            None
        };
        let key = match &action {
            Action::Stage(_) => "stage",
            Action::Unstage(_) => "unstage",
            Action::Commit { .. } => "commit",
        }
        .to_owned();
        let git = self.git.clone();
        let tx = self.sender.clone();
        let dropped = tx.clone();
        let ctx = ctx.clone();
        let drop_ctx = ctx.clone();
        let (_, accepted) = self.scheduler.submit(
            context.repository.identity.fingerprint.clone(),
            context
                .repository
                .common_directory
                .to_string_lossy()
                .to_uppercase(),
            key,
            Lane::WorkingCopy,
            move || {
                let _ = tx.send(Event::Started);
                ctx.request_repaint();
                let output_tx = tx.clone();
                let output_ctx = ctx.clone();
                let callback: OutputCallback = Arc::new(move |stderr, bytes| {
                    let _ = output_tx.send(Event::Output(stderr, bytes.to_vec()));
                    output_ctx.request_repaint();
                });
                let result = git.execute_working(&context, &action, Some(callback));
                let outcome = if result.as_ref().is_ok_and(|r| r.succeeded()) {
                    Outcome::Succeeded
                } else {
                    Outcome::Failed
                };
                let _ = tx.send(Event::Completed(result, submitted));
                ctx.request_repaint();
                outcome
            },
            move |_| {
                let _ = dropped.send(Event::Removed);
                drop_ctx.request_repaint();
            },
        );
        if accepted {
            if committing {
                self.open = false;
            }
        } else {
            self.busy = false;
            self.status = Some(text("OperationWaiting"));
        }
    }
    fn stage(&mut self, unstage: bool, all: bool, ctx: &egui::Context) {
        let Some(context) = &self.context else {
            return;
        };
        let entries = context
            .entries
            .iter()
            .filter(|e| {
                (if unstage {
                    e.group == Group::Staged
                } else {
                    !matches!(e.group, Group::Staged | Group::Ignored)
                }) && (all || self.selection.contains(&(e.group, e.path.clone())))
            })
            .cloned()
            .collect::<Vec<_>>();
        if !entries.is_empty() {
            self.submit(
                if unstage {
                    Action::Unstage(entries)
                } else {
                    Action::Stage(entries)
                },
                ctx,
            );
        }
    }
    fn commit_reason(&self) -> Option<&'static str> {
        let Some(context) = &self.context else {
            return Some("CommitWaitForRead");
        };
        if self.busy || self.loading {
            Some("CommitWaitForRead")
        } else if self.draft.subject.trim().is_empty() {
            Some("CommitMessageRequired")
        } else if self.draft.message().len() > 65536 {
            Some("CommitMessageLimit")
        } else if context.entries.iter().any(|e| e.group == Group::Conflicted) {
            Some("CommitUnresolved")
        } else if !context.repository.operations.is_empty()
            && context.repository.operations != ["merge"]
        {
            Some("ActionSequence")
        } else if !self.amend
            && context.repository.operations != ["merge"]
            && !context.entries.iter().any(|e| e.group == Group::Staged)
        {
            Some("CommitNothingStaged")
        } else {
            None
        }
    }
    fn request_commit(&mut self, ctx: &egui::Context) {
        if self.commit_reason().is_none() {
            if self.amend {
                self.confirm_amend = true;
            } else {
                self.submit(
                    Action::Commit {
                        message: self.draft.message(),
                        amend: false,
                    },
                    ctx,
                );
            }
        }
    }
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let id = egui::ViewportId::from_hash_of(("commit", &self.path));
        let title = format!(
            "{} · {}",
            text("LocalChanges"),
            self.path.file_name().unwrap_or_default().to_string_lossy()
        );
        ctx.show_viewport_immediate(
            id,
            egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size(self.size)
                .with_min_inner_size([760., 560.])
                .with_minimize_button(false)
                .with_maximize_button(false),
            |ui, _| {
                let ctx = ui.ctx().clone();
                if ctx.input(|i| i.viewport().close_requested()) {
                    self.open = false;
                    self.flush(&ctx);
                }
                if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
                    self.size = rect.size();
                }
                if ctx.input(|i| i.key_pressed(egui::Key::F5)) {
                    self.refresh(&ctx);
                }
                if ctx.input(|i| {
                    i.modifiers.ctrl && !i.modifiers.shift && i.key_pressed(egui::Key::Enter)
                }) {
                    self.request_commit(&ctx);
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !self.confirm_amend {
                    self.open = false;
                    self.flush(&ctx);
                }
                egui::Panel::top("commit_controls").show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !self.loading && !self.busy,
                                egui::Button::new(text("CommitRefresh")),
                            )
                            .clicked()
                        {
                            self.refresh(&ctx);
                        }
                        if let Some(context) = &self.context {
                            ui.label(context.repository.branch.as_deref().unwrap_or("HEAD"));
                        }
                        if self.loading {
                            ui.label(text("ChangesLoading"));
                        }
                        if let Some(status) = &self.status {
                            ui.label(status);
                        }
                    });
                });
                egui::Panel::bottom("commit_message")
                    .default_size(175.)
                    .min_size(140.)
                    .resizable(true)
                    .show(ui, |ui| {
                        if let Some(error) = &self.draft_error {
                            ui.colored_label(Color32::LIGHT_RED, error);
                        }
                        ui.label(text("CommitSubject"));
                        let subject = ui.add(
                            egui::TextEdit::singleline(&mut self.draft.subject)
                                .desired_width(f32::INFINITY),
                        );
                        ui.label(text("CommitDescription"));
                        let body = ui.add(
                            egui::TextEdit::multiline(&mut self.draft.description)
                                .desired_width(f32::INFINITY)
                                .desired_rows(3),
                        );
                        if subject.changed() || body.changed() {
                            self.edited();
                        }
                        ui.horizontal(|ui| {
                            let allow_amend = self.context.as_ref().is_some_and(|c| {
                                c.repository.head.is_some() && c.repository.operations.is_empty()
                            });
                            if ui
                                .add_enabled(
                                    allow_amend && !self.busy,
                                    egui::Checkbox::new(&mut self.amend, text("CommitAmend")),
                                )
                                .changed()
                                && self.amend
                                && self.draft.is_empty()
                                && let Some(context) = &self.context
                            {
                                let (subject, body) = context
                                    .head_message
                                    .split_once('\n')
                                    .unwrap_or((&context.head_message, ""));
                                self.draft = drafts::Text {
                                    subject: subject.into(),
                                    description: body.trim_start_matches(['\r', '\n']).into(),
                                };
                                self.edited();
                            }
                            let reason = self.commit_reason();
                            if ui
                                .add_enabled(reason.is_none(), egui::Button::new(text("Commit")))
                                .on_disabled_hover_text(text(reason.unwrap_or("Commit")))
                                .clicked()
                            {
                                self.request_commit(&ctx);
                            }
                            if self.draft_saving {
                                ui.label(text("CommitDraftSaving"));
                            }
                        });
                    });
                egui::Panel::left("commit_files")
                    .default_size(370.)
                    .min_size(220.)
                    .resizable(true)
                    .show(ui, |ui| {
                        let mut chosen = None;
                        for staged in [false, true] {
                            let rows = self
                                .context
                                .as_ref()
                                .map(|c| {
                                    c.entries
                                        .iter()
                                        .filter(|e| {
                                            (e.group == Group::Staged) == staged
                                                && e.group != Group::Ignored
                                        })
                                        .cloned()
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            ui.horizontal(|ui| {
                                ui.strong(strings::format(
                                    if staged {
                                        "CommitStagedCount"
                                    } else {
                                        "CommitUnstagedCount"
                                    },
                                    &[&rows.len().to_string()],
                                ));
                                let selected = rows
                                    .iter()
                                    .any(|e| self.selection.contains(&(e.group, e.path.clone())));
                                let all = ui.input(|i| i.modifiers.shift);
                                if ui
                                    .add_enabled(
                                        !self.busy
                                            && !self.loading
                                            && (selected || all && !rows.is_empty()),
                                        egui::Button::new(text(if staged {
                                            "CommitUnstage"
                                        } else {
                                            "CommitStage"
                                        })),
                                    )
                                    .clicked()
                                {
                                    self.stage(staged, all, &ctx);
                                }
                                if ui
                                    .add_enabled(
                                        !self.busy && !self.loading && !rows.is_empty(),
                                        egui::Button::new(text(if staged {
                                            "CommitUnstageAll"
                                        } else {
                                            "CommitStageAll"
                                        })),
                                    )
                                    .clicked()
                                {
                                    self.stage(staged, true, &ctx);
                                }
                            });
                            let height = (ui.available_height() / if staged { 1. } else { 2. }
                                - 32.)
                                .max(65.);
                            egui::ScrollArea::both()
                                .id_salt(("files", staged))
                                .max_height(height)
                                .min_scrolled_height(height)
                                .show_rows(ui, 22., rows.len(), |ui, range| {
                                    for index in range {
                                        let entry = &rows[index];
                                        let key = (entry.group, entry.path.clone());
                                        let selected = self.selection.contains(&key);
                                        let response = ui.selectable_label(
                                            selected,
                                            format!("{}  {}", entry.code, entry.path),
                                        );
                                        if response.clicked() {
                                            if !ui.input(|i| i.modifiers.ctrl) {
                                                self.selection.clear();
                                            }
                                            if selected && ui.input(|i| i.modifiers.ctrl) {
                                                self.selection.remove(&key);
                                            } else {
                                                self.selection.insert(key);
                                            }
                                            chosen = Some(entry.clone());
                                        }
                                        if response.double_clicked() {
                                            self.selection.clear();
                                            self.selection
                                                .insert((entry.group, entry.path.clone()));
                                            self.stage(staged, false, &ctx);
                                        }
                                    }
                                });
                            ui.separator();
                        }
                        if let Some(entry) = chosen {
                            self.show_diff(entry, &ctx);
                        }
                    });
                egui::CentralPanel::default().show(ui, |ui| {
                    if let Some(entry) = &self.focused {
                        ui.strong(&entry.path);
                    }
                    if let Some(error) = &self.diff_error {
                        ui.colored_label(Color32::LIGHT_RED, error);
                    } else if let Some(patch) = &self.diff {
                        draw_diff(ui, patch);
                    } else {
                        ui.label(text(if self.focused.is_some() {
                            "DiffLoading"
                        } else {
                            "DiffChooseFile"
                        }));
                    }
                });
                if self.confirm_amend {
                    egui::Modal::new(egui::Id::unique("confirm_amend")).show(&ctx, |ui| {
                        if let Some(context) = &self.context {
                            ui.label(strings::format(
                                "CommitConfirmAmend",
                                &[
                                    context.repository.branch.as_deref().unwrap_or("HEAD"),
                                    context.repository.head.as_deref().unwrap_or(""),
                                ],
                            ));
                        }
                        ui.horizontal(|ui| {
                            if ui.button(text("CommitAmend")).clicked() {
                                self.confirm_amend = false;
                                self.submit(
                                    Action::Commit {
                                        message: self.draft.message(),
                                        amend: true,
                                    },
                                    &ctx,
                                );
                            }
                            if ui.button(text("ActionCancel")).clicked() {
                                self.confirm_amend = false;
                            }
                        });
                    });
                }
            },
        );
        // Schedule work after the first window layout, so opening never prepares Git.
        if !self.initialized {
            self.initialize(ctx);
        }
    }
}
fn draw_diff(ui: &mut egui::Ui, patch: &str) {
    let lines = patch.lines().collect::<Vec<_>>();
    egui::ScrollArea::both().show_rows(ui, 18., lines.len(), |ui, range| {
        for i in range {
            let line = lines[i];
            let color = if line.starts_with('+') && !line.starts_with("+++") {
                Color32::from_rgb(135, 207, 150)
            } else if line.starts_with('-') && !line.starts_with("---") {
                Color32::from_rgb(235, 152, 152)
            } else if line.starts_with("@@") {
                Color32::from_rgb(139, 178, 225)
            } else {
                ui.visuals().text_color()
            };
            ui.label(RichText::new(line).monospace().color(color));
        }
    });
}
fn error_text(error: working::Error) -> String {
    match error {
        working::Error::Git(shovel_core::git::Error::Command { output, .. }) => {
            format!("{}\n{output}", text("ActionReadFailed"))
        }
        other => text(&other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shovel_core::process::{self, Lifetime};
    fn command(git: &Git, path: &std::path::Path, args: &[&str]) {
        let result = process::run(
            &git.request(
                path,
                args.iter().copied(),
                Lifetime::AcceptedCommand,
                1024 * 1024,
            ),
            &Cancellation::default(),
            None,
        )
        .unwrap();
        assert!(result.success(), "{result:?}");
    }
    fn settle(editor: &mut Editor, ctx: &egui::Context, ready: impl Fn(&Editor) -> bool) {
        let start = Instant::now();
        loop {
            editor.poll(ctx);
            if ready(editor) {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(28),
                "{}",
                editor.status.as_deref().unwrap_or("timeout")
            );
            thread::sleep(Duration::from_millis(15));
        }
    }
    #[test]
    fn accepted_commit_closes_window_preserves_new_typing_and_reopens_failed_draft() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        std::fs::create_dir(&root).unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        let config = temp.path().join("isolated.config");
        std::fs::write(
            &config,
            "[user]\nname = Fixture\nemail = fixture@example.invalid\n[commit]\ngpgsign = false\n",
        )
        .unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        command(&git, &root, &["init", "--initial-branch=main"]);
        std::fs::write(root.join("file"), "one\n").unwrap();
        command(&git, &root, &["add", "--", "file"]);
        let store = drafts::Store {
            directory: temp.path().join("drafts"),
            legacy_directory: None,
        };
        let scheduler = Scheduler::default();
        let mut editor = Editor::new(
            root.clone(),
            Arc::new(git),
            scheduler.clone(),
            store.clone(),
        );
        let ctx = egui::Context::default();
        editor.initialize(&ctx);
        editor.draft.subject = "submitted subject".into();
        editor.edited();
        settle(&mut editor, &ctx, |e| !e.loading && e.draft_loaded);
        assert_eq!(editor.draft.subject, "submitted subject");
        // Hold the native hook. Closing/reopening the view cannot cancel it.
        std::fs::write(
            root.join(".git/hooks/pre-commit"),
            "#!/bin/sh\ntouch hook-started\nwhile test ! -f hook-release; do sleep .05; done\n",
        )
        .unwrap();
        editor.request_commit(&ctx);
        assert!(!editor.open);
        assert!(editor.blocks_exit());
        let start = Instant::now();
        while !root.join("hook-started").exists() {
            editor.poll(&ctx);
            assert!(start.elapsed() < Duration::from_secs(20));
            thread::sleep(Duration::from_millis(15));
        }
        assert!(scheduler.has_accepted_work());
        editor.activate();
        editor.draft.subject = "next message".into();
        editor.edited();
        editor.open = false;
        std::fs::write(root.join("hook-release"), "").unwrap();
        settle(&mut editor, &ctx, |e| {
            !e.busy && !e.draft_dirty && !e.draft_saving
        });
        assert_eq!(editor.draft.subject, "next message");
        assert!(!editor.failed_commit);
        let identity = identity::read(&root).unwrap();
        assert_eq!(
            store
                .load(&identity, &Cancellation::default())
                .unwrap()
                .text
                .subject,
            "next message"
        );
        std::fs::write(root.join("file"), "two\n").unwrap();
        command(&editor.git, &root, &["add", "--", "file"]);
        std::fs::write(
            root.join(".git/hooks/pre-commit"),
            "#!/bin/sh\necho fixture-failure >&2\nexit 1\n",
        )
        .unwrap();
        editor.refresh(&ctx);
        settle(&mut editor, &ctx, |e| !e.loading);
        let stamp = editor.context.as_ref().unwrap().index_stamp.clone();
        editor.activate();
        editor.request_commit(&ctx);
        assert!(!editor.open);
        settle(&mut editor, &ctx, |e| !e.busy);
        assert!(editor.failed_commit);
        assert!(editor.output.contains("fixture-failure"));
        assert_eq!(editor.draft.subject, "next message");
        assert_eq!(editor.context.as_ref().unwrap().index_stamp, stamp);
        editor.activate();
        assert!(editor.open);
    }
}
