//! Repository-owned Local Changes state shared by the workspace and native window.
use crate::strings::{self, text};
use eframe::egui;
pub(crate) use queue::OwnedActivity;
use shovel_core::{
    drafts,
    git::Git,
    identity::{self, Identity},
    network::Target,
    process::Cancellation,
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
    Loaded(u64, Box<Result<Context, working::Error>>),
    Diff(u64, Result<shovel_core::diff::FileDiff, working::Error>),
    DraftLoaded(Result<(Identity, Result<drafts::Snapshot, drafts::Error>), drafts::Error>),
    DraftSaved(u64, Result<drafts::Snapshot, String>),
    WorkingStarted(u64),
    WorkingPushing(u64),
    WorkingOutput(u64, Arc<crate::activity_output::OutputStream>),
    WorkingCompleted(u64, Box<queue::WorkingResult>),
    WorkingRemoved(u64, Outcome),
    Removed,
    FileStarted(file_menu::FileAction),
    PatchPicked(file_menu::Selection, Option<PathBuf>),
    PatchStarted,
    ShellCompleted(Result<bool, &'static str>),
    PatchCompleted(Result<usize, shovel_core::operation::Error>),
    FileCompleted(
        file_menu::FileAction,
        Box<Result<shovel_core::files::Completed, shovel_core::operation::Error>>,
    ),
}
struct CommitReview {
    context: Arc<Context>,
    draft: drafts::Text,
    push: Option<Target>,
}
enum Leave {
    Close,
    Exit,
    Menu(Box<file_menu::Menu>),
    Select {
        entry: Box<Change>,
        selection: BTreeSet<(Group, String)>,
        stage: bool,
    },
}
pub struct Editor {
    pub(super) read_workers: crate::read_workers::Workers,
    pub path: PathBuf,
    pub open: bool,
    pub inline: bool,
    pub context: Option<Arc<Context>>,
    pub failed_commit: bool,
    pub failed_operation: Option<&'static str>,
    pub status: Option<String>,
    pub output: String,
    pub busy: bool,
    queue: queue::Queue,
    pub size: egui::Vec2,
    geometry: crate::window_geometry::Geometry,
    files_width: f32,
    message_height: f32,
    pub external_merge_tool: shovel_core::conflicts::ToolSettings,
    git: Arc<Git>,
    scheduler: Scheduler,
    store: drafts::Store,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
    initialized: bool,
    loading: bool,
    scan_cancel: Cancellation,
    target_rows_current: bool,
    snapshot_error: Option<String>,
    refresh_pending: bool,
    generation: u64,
    diff_generation: u64,
    diff_cancel: Cancellation,
    selection: BTreeSet<(Group, String)>,
    focused: Option<Change>,
    diff: Option<shovel_core::diff::FileDiff>,
    diff_view: crate::diff_view::View,
    diff_options: shovel_core::diff::Options,
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
    confirm_amend: Option<CommitReview>,
    pub(crate) authentication: Option<crate::operations::ExternalAuthentication>,
    refresh_history: bool,
    conflict: Option<crate::conflict::Pane>,
    operation_output: String,
    leave: Option<Leave>,
    exit_cancelled: bool,
    file_menu: Option<file_menu::Menu>,
    file_confirmation: Option<file_menu::Confirmation>,
    diff_deferred: bool,
    files_focus: Option<egui::Id>,
    patch_picking: bool,
    shell_launcher: crate::file_shell::Launcher,
    inspector: Option<crate::file_history::Inspector>,
    patch_picker: Arc<dyn Fn(rfd::FileDialog) -> Option<PathBuf> + Send + Sync>,
}
impl Editor {
    pub fn apply_preferences(&mut self, settings: &shovel_core::settings::Settings) {
        self.diff_view.side_by_side = settings
            .get("DiffSideBySide")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        self.diff_view.font_size = settings
            .get("DiffFontSize")
            .and_then(|v| v.as_f64())
            .unwrap_or(12.) as f32;
        self.external_merge_tool = settings
            .get("ExternalMergeTool")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
    }

    pub fn new(path: PathBuf, git: Arc<Git>, scheduler: Scheduler, store: drafts::Store) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            read_workers: Default::default(),
            path,
            open: true,
            inline: false,
            context: None,
            failed_commit: false,
            failed_operation: None,
            status: None,
            output: String::new(),
            busy: false,
            queue: queue::Queue::default(),
            size: egui::vec2(1200., 800.),
            geometry: Default::default(),
            files_width: 380.,
            message_height: 220.,
            external_merge_tool: Default::default(),
            git,
            scheduler,
            store,
            sender,
            receiver,
            initialized: false,
            loading: false,
            scan_cancel: Cancellation::default(),
            target_rows_current: false,
            snapshot_error: None,
            refresh_pending: false,
            generation: 0,
            diff_generation: 0,
            diff_cancel: Cancellation::default(),
            selection: BTreeSet::new(),
            focused: None,
            diff: None,
            diff_view: crate::diff_view::View::default(),
            diff_options: shovel_core::diff::Options::default(),
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
            confirm_amend: None,
            authentication: None,
            refresh_history: false,
            conflict: None,
            operation_output: String::new(),
            leave: None,
            exit_cancelled: false,
            file_menu: None,
            file_confirmation: None,
            diff_deferred: false,
            files_focus: None,
            patch_picking: false,
            shell_launcher: Arc::new(crate::file_shell::launch),
            inspector: None,
            patch_picker: Arc::new(|dialog| dialog.save_file()),
        }
    }
    pub fn can_retire(&self) -> bool {
        !self.open
            && !self.loading
            && !self.blocks_exit()
            && (!self.initialized || self.draft_loaded)
            && self.draft_error.is_none()
            && !self.inspector.as_ref().is_some_and(|v| v.open || v.busy)
    }
    pub fn current_context(&self) -> Option<&Context> {
        self.context
            .as_deref()
            .filter(|_| self.snapshot_error.is_none())
    }
    pub fn activate(&mut self) {
        self.open = true;
    }
    pub fn hide(&mut self, ctx: &egui::Context) {
        self.leave = None;
        self.confirm_amend = None;
        self.files_focus = None;
        self.finish_leave(Leave::Close, ctx);
    }
    pub fn has_prompt(&self) -> bool {
        self.confirm_amend.is_some() || self.leave.is_some() || self.file_confirmation.is_some()
    }
    pub fn external_changed(&mut self) {
        self.refresh_pending = true;
    }
    pub fn blocks_exit(&self) -> bool {
        (self.initialized && !self.draft_loaded)
            || self.draft_dirty
            || self.draft_saving
            || self.action_busy()
            || self.unsaved_conflict()
    }
    pub fn native_picker_open(&self) -> bool {
        self.patch_picking
    }
    fn action_busy(&self) -> bool {
        self.busy || self.patch_picking || self.conflict.as_ref().is_some_and(|pane| pane.mutating)
    }
    pub fn progress(&self) -> Option<String> {
        if self.busy {
            Some(
                self.status
                    .clone()
                    .unwrap_or_else(|| text("OperationWaiting")),
            )
        } else if let Some(pane) = self.conflict.as_ref().filter(|pane| pane.mutating) {
            Some(pane.status.clone())
        } else if self.loading {
            Some(text("CommitReadingChanges"))
        } else if self.draft_saving {
            Some(text("CommitDraftSaving"))
        } else {
            None
        }
    }
    pub fn unsaved_conflict(&self) -> bool {
        self.conflict
            .as_ref()
            .is_some_and(crate::conflict::Pane::unsaved)
    }
    pub fn confirm_exit(&mut self, ctx: &egui::Context) {
        self.file_menu = None;
        self.file_confirmation = None;
        self.open |= !self.inline;
        self.request_leave(Leave::Exit, ctx);
    }
    pub fn take_exit_cancelled(&mut self) -> bool {
        std::mem::take(&mut self.exit_cancelled)
    }
    pub fn exit_prompt_open(&self) -> bool {
        matches!(self.leave, Some(Leave::Exit))
    }
    fn answer_leave(&mut self, discard: bool, ctx: &egui::Context) {
        if let Some(next) = self.leave.take() {
            if discard {
                self.conflict = None;
                self.finish_leave(next, ctx);
            } else if matches!(next, Leave::Exit) {
                self.exit_cancelled = true;
            }
        }
    }
    fn request_leave(&mut self, next: Leave, ctx: &egui::Context) {
        if self.patch_picking {
            self.status = Some(text("RustPatchChooserOpen"));
            if matches!(next, Leave::Exit) {
                self.exit_cancelled = true;
            }
            return;
        }
        if (matches!(next, Leave::Select { .. }) && self.working_blocked())
            || (matches!(next, Leave::Menu(_)) && self.action_busy())
        {
            return;
        }
        let leaving = match &next {
            Leave::Menu(menu) => self.conflict.as_ref().is_some_and(|p| {
                p.path != menu.entry.path || menu.entry.group != Group::Conflicted
            }),
            Leave::Select { entry, .. } => self
                .conflict
                .as_ref()
                .is_some_and(|p| p.path != entry.path || entry.group != Group::Conflicted),
            _ => true,
        };
        if leaving && self.unsaved_conflict() {
            self.leave = Some(next);
        } else {
            self.finish_leave(next, ctx);
        }
    }
    fn finish_leave(&mut self, next: Leave, ctx: &egui::Context) {
        match next {
            Leave::Menu(menu) => self.begin_file_menu(*menu),
            Leave::Close | Leave::Exit => {
                self.file_menu = None;
                self.file_confirmation = None;
                if let Some(pane) = &mut self.conflict {
                    pane.pause();
                }
                self.open = false;
                self.refresh_pending = true;
                self.flush(ctx);
            }
            Leave::Select {
                entry,
                selection,
                stage,
            } => {
                if self
                    .conflict
                    .as_ref()
                    .is_some_and(|p| p.path != entry.path || entry.group != Group::Conflicted)
                {
                    self.conflict = None;
                }
                self.selection = selection;
                let staged = entry.group == Group::Staged;
                self.show_diff(*entry, ctx);
                if stage {
                    self.stage(staged, false, ctx);
                }
            }
        }
    }
    fn open_conflict(&mut self) {
        if self.action_busy() || self.loading || self.snapshot_error.is_some() {
            return;
        }
        let Some(context) = self.context.clone() else {
            return;
        };
        let Some(change) = self
            .focused
            .clone()
            .filter(|c| c.group == Group::Conflicted)
        else {
            return;
        };
        if self
            .conflict
            .as_ref()
            .is_some_and(|p| p.path == change.path && !p.resolved)
        {
            return;
        }
        self.diff_cancel.cancel();
        self.diff_generation += 1;
        self.diff = None;
        self.diff_error = None;
        self.conflict = Some(
            crate::conflict::Pane::new(
                context,
                change,
                self.git.clone(),
                self.scheduler.clone(),
                self.external_merge_tool.clone(),
            )
            .with_read_workers(self.read_workers.clone()),
        );
    }
    pub fn draft_problem(&self) -> Option<&str> {
        self.draft_error.as_deref()
    }
    fn initialize(&mut self, ctx: &egui::Context) {
        if self.read_workers.stopping() {
            return;
        }
        self.initialized = true;
        self.refresh(ctx);
        let path = self.path.clone();
        let store = self.store.clone();
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        let lease = self.read_workers.start_required();
        thread::spawn(move || {
            let _lease = lease;
            #[cfg(test)]
            crate::history_cache::tests::wait_for_read(&store.directory);
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
        if self.read_workers.stopping() {
            return;
        }
        if self.loading || self.action_busy() {
            self.refresh_pending = true;
            return;
        }
        self.refresh_pending = false;
        self.loading = true;
        self.scan_cancel.cancel();
        self.scan_cancel = Cancellation::default();
        self.generation += 1;
        let generation = self.generation;
        let git = self.git.clone();
        let path = self.path.clone();
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        let cancel = self.scan_cancel.clone();
        let lease = self.read_workers.start(cancel.clone());
        thread::spawn(move || {
            let _lease = lease;
            #[cfg(test)]
            crate::history_cache::tests::wait_for_read(&path);
            let result = git.working_copy(&path, &cancel);
            let _ = tx.send(Event::Loaded(generation, Box::new(result)));
            ctx.request_repaint();
        });
    }
    fn show_diff(&mut self, entry: Change, ctx: &egui::Context) {
        if self.read_workers.stopping() {
            return;
        }
        let Some(context) = self.context.clone() else {
            return;
        };
        self.diff_cancel.cancel();
        self.diff_cancel = Cancellation::default();
        self.diff_generation += 1;
        self.focused = Some(entry.clone());
        self.diff = None;
        self.diff_view.reset();
        self.diff_error = None;
        if self.file_menu.is_some()
            || self.file_confirmation.is_some()
            || self.patch_picking
            || self.has_working_queue()
        {
            self.diff_deferred = true;
            return;
        }
        self.diff_deferred = false;
        if self
            .conflict
            .as_ref()
            .is_some_and(|p| p.path == entry.path && !p.resolved)
        {
            return;
        }
        let generation = self.diff_generation;
        let cancel = self.diff_cancel.clone();
        let git = self.git.clone();
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        let options = self.diff_options;
        let lease = self.read_workers.start(cancel.clone());
        thread::spawn(move || {
            let _lease = lease;
            let result = git.file_diff(&context, &entry, options, &cancel);
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
                    match *result {
                        Ok(context) => self.apply(context, ctx),
                        Err(error) => self.snapshot_error = Some(error_text(error)),
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
                Event::WorkingStarted(id) => self.working_started(id),
                Event::WorkingPushing(id) => self.working_pushing(id),
                Event::WorkingOutput(id, stream) => self.working_output(id, &stream.take_pending()),
                Event::WorkingCompleted(id, result) => self.working_completed(id, *result, ctx),
                Event::WorkingRemoved(id, outcome) => self.working_removed(id, outcome),
                Event::FileStarted(action) => self.status = Some(text(action.phase())),
                Event::FileCompleted(action, result) => self.file_completed(action, *result, ctx),
                Event::PatchPicked(selected, target) => self.patch_picked(selected, target, ctx),
                Event::PatchStarted => self.status = Some(text("FilePatchWriting")),
                Event::ShellCompleted(result) => self.shell_completed(result),
                Event::PatchCompleted(result) => self.patch_completed(result, ctx),
                Event::Removed => {
                    self.busy = false;
                    self.status = Some(text("ActionStoppedShort"));
                }
                _ => {}
            }
        }
        if let Some(refresh) = self.conflict.as_mut().and_then(|pane| pane.poll(ctx)) {
            self.failed_operation = refresh.failed.then_some("ConflictResolving");
            self.status = Some(refresh.status);
            self.output = refresh.output;
            self.operation_output = self.output.clone();
            self.refresh_history = true;
            if let Some(result) = refresh.context {
                match result {
                    Ok(context) => self.apply(context, ctx),
                    Err(error) => self.failed_reinspection(error),
                }
            }
        }
        if self.draft_dirty {
            ctx.request_repaint_after(Duration::from_millis(400));
            if self.draft_saved_at.elapsed() >= Duration::from_millis(400) {
                self.flush(ctx);
            }
        }
        if self.refresh_pending
            && self.open
            && self.initialized
            && !self.loading
            && !self.action_busy()
        {
            self.refresh(ctx);
        }
        if self.diff_deferred
            && self.open
            && self.file_menu.is_none()
            && self.file_confirmation.is_none()
            && self.leave.is_none()
            && !self.action_busy()
            && !self.loading
            && let Some(entry) = self.focused.clone()
        {
            self.show_diff(entry, ctx);
        }
        std::mem::take(&mut self.refresh_history)
    }
    fn failed_reinspection(&mut self, detail: String) {
        self.status = Some(match self.status.take() {
            Some(outcome) => format!("{outcome}\n{detail}"),
            None => detail.clone(),
        });
        self.snapshot_error = Some(detail);
    }
    fn apply(&mut self, context: Context, ctx: &egui::Context) {
        self.snapshot_error = None;
        self.target_rows_current = false;
        let resolved_path = self
            .conflict
            .as_ref()
            .filter(|p| p.resolved)
            .map(|p| p.path.clone());
        if resolved_path.is_some() {
            self.conflict = None;
        }
        let matching_row = |group, path: &str| {
            context.change(group, path).or_else(|| {
                [
                    Group::Conflicted,
                    Group::Unstaged,
                    Group::Untracked,
                    Group::Staged,
                ]
                .into_iter()
                .find_map(|other| context.change(other, path))
            })
        };
        self.selection = std::mem::take(&mut self.selection)
            .into_iter()
            .filter_map(|(group, path)| matching_row(group, &path).map(|entry| (entry.group, path)))
            .collect();
        let focus = self
            .focused
            .as_ref()
            .and_then(|old| matching_row(old.group, &old.path))
            .cloned();
        self.context = Some(Arc::new(context));
        // Keep edited text even when an external operation removes this conflict.
        // The captured preview cannot execute against the changed index/state.
        if let Some(pane) = &mut self.conflict {
            pane.observe(self.context.as_ref().unwrap());
            return;
        }
        if let Some(focus) = focus {
            if resolved_path.as_ref() == Some(&focus.path) {
                self.selection.insert((focus.group, focus.path.clone()));
            }
            self.show_diff(focus, ctx);
        } else {
            self.focused = None;
            self.diff = None;
            self.diff_error = None;
        }
    }
    fn stage(&mut self, unstage: bool, all: bool, ctx: &egui::Context) {
        let Some(context) = &self.context else {
            return;
        };
        let entries = if all {
            file_list::FileList::new(Some(context), unstage)
                .entries()
                .cloned()
                .collect()
        } else {
            self.selected_entries(context, unstage)
        };
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
    fn push_target(&self) -> Option<&Target> {
        let repository = &self.context.as_ref()?.repository;
        repository
            .upstream_target
            .as_ref()
            .filter(|target| repository.remotes.contains(&target.remote))
    }
    fn commit_reason(&self, push: bool) -> Option<&'static str> {
        let Some(context) = &self.context else {
            return Some("CommitWaitForRead");
        };
        if !self.working_ready() {
            Some("CommitWaitForRead")
        } else if push && self.push_target().is_none() {
            Some("CommitNoUpstream")
        } else if self.draft.subject.trim().is_empty() {
            Some("CommitMessageRequired")
        } else if self.draft.message().len() > 65536 {
            Some("CommitMessageLimit")
        } else if !context.group(Group::Conflicted).is_empty() {
            Some("CommitUnresolved")
        } else if !context.repository.operations.is_empty()
            && context.repository.operations != ["merge"]
        {
            Some("ActionSequence")
        } else if !self.amend
            && context.repository.operations != ["merge"]
            && context.group(Group::Staged).is_empty()
            && !self.queue.has_stage()
        {
            Some("CommitNothingStaged")
        } else {
            None
        }
    }
    fn request_commit(&mut self, push: bool, ctx: &egui::Context) {
        if self.has_prompt() || self.file_menu.is_some() || self.commit_reason(push).is_some() {
            return;
        }
        let review = CommitReview {
            context: self.context.clone().unwrap(),
            draft: self.draft.clone(),
            push: if push {
                self.push_target().cloned()
            } else {
                None
            },
        };
        if self.amend {
            self.confirm_amend = Some(review);
        } else {
            self.submit_commit(review, false, ctx);
        }
    }
    fn submit_commit(&mut self, review: CommitReview, amend: bool, ctx: &egui::Context) {
        let action = Action::Commit {
            message: review.draft.message(),
            amend,
        };
        self.submit_captured(action, review.context, Some(review.draft), review.push, ctx);
    }
    pub fn restore_layout(&mut self, settings: &shovel_core::settings::Settings) {
        if let Some(layout) = settings.get("CommitLayout") {
            let value = |key: &str, default: f32, min: f32, max: f32| {
                layout
                    .get(key)
                    .and_then(|v| v.as_f64())
                    .map(|v| (v as f32).clamp(min, max))
                    .unwrap_or(default)
            };
            self.size = egui::vec2(
                value("Width", 1200., 900., 7680.),
                value("Height", 800., 620., 4320.),
            );
            self.files_width = value("FilesWidth", 380., 250., 3000.);
            self.message_height = value("MessageHeight", 220., 180., 2000.);
        }
        if let Some(layout) = crate::window_geometry::saved_layout(settings, "CommitWindow") {
            self.size = egui::vec2(
                layout["Width"].as_f64().unwrap().clamp(900., 7680.) as f32,
                layout["Height"].as_f64().unwrap().clamp(620., 4320.) as f32,
            );
        }
    }
    pub fn save_layout(&self, settings: &mut shovel_core::settings::Settings) {
        let mut layout = settings
            .get("CommitLayout")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        for (key, value) in [
            ("Width", self.size.x),
            ("Height", self.size.y),
            ("FilesWidth", self.files_width),
            ("MessageHeight", self.message_height),
        ] {
            layout.insert(key.into(), serde_json::json!(value));
        }
        settings.set("CommitLayout", serde_json::Value::Object(layout));
        crate::window_geometry::save_layout(settings, "CommitWindow", self.size, false);
    }
    fn observe_geometry(&mut self, ctx: &egui::Context, class: egui::ViewportClass) {
        if class != egui::ViewportClass::EmbeddedWindow {
            self.geometry.observe(ctx);
            if let Some(size) = self.geometry.native_size {
                self.size = size;
            }
        }
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        enabled: bool,
        mut show_failure: impl FnMut(&egui::Context),
    ) {
        if let Some(inspector) = &mut self.inspector {
            inspector.show(ctx);
        }
        if !self.open || self.inline {
            return;
        }
        let id = egui::ViewportId::from_hash_of(("commit", &self.path));
        let title = format!(
            "{} · {}",
            text("LocalChanges"),
            self.path.file_name().unwrap_or_default().to_string_lossy()
        );
        let mut visible = false;
        ctx.show_viewport_immediate(
            id,
            egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size(crate::window_geometry::points(ctx, self.size))
                .with_min_inner_size(crate::window_geometry::points(ctx, egui::vec2(900., 620.)))
                .with_minimize_button(false)
                .with_maximize_button(false),
            |ui, class| {
                visible = ui.is_visible();
                let ctx = ui.ctx().clone();
                if ctx.input(|i| i.viewport().close_requested()) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    self.request_leave(Leave::Close, &ctx);
                }
                self.observe_geometry(&ctx, class);
                self.show_contents(ui, enabled);
                show_failure(&ctx);
            },
        );
        // An embedded viewport's first sizing pass is invisible; only a displayed
        // editor may begin its requested reads.
        if visible && !self.initialized {
            self.initialize(ctx);
        }
    }

    pub fn show_inline(&mut self, ui: &mut egui::Ui, enabled: bool) {
        if ui.is_visible() && !self.initialized {
            self.initialize(ui.ctx());
        }
        let id = egui::Id::unique(("inline_commit", &self.path));
        ui.push_id(id, |ui| self.show_contents(ui, enabled));
    }

    fn show_contents(&mut self, ui: &mut egui::Ui, enabled: bool) {
        let ctx = ui.ctx().clone();
        if !enabled {
            ui.disable();
        }
        if !self.inline
            && enabled
            && self.leave.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::F5))
        {
            self.refresh(&ctx);
        }
        if enabled
            && !self.has_prompt()
            && self.file_menu.is_none()
            && let Some(push) = ctx.input_mut(|input| {
                let index = input.events.iter().position(|event| {
                    matches!(event, egui::Event::Key {
                    key: egui::Key::Enter, pressed: true, modifiers, ..
                } if modifiers.ctrl && !modifiers.alt && !modifiers.mac_cmd)
                })?;
                match input.events.remove(index) {
                    egui::Event::Key { modifiers, repeat: false, .. } => Some(modifiers.shift),
                    _ => None,
                }
            })
        {
            self.request_commit(push, &ctx);
        }
        if !self.inline
            && enabled
            && self.confirm_amend.is_none()
            && self.leave.is_none()
            && self.file_menu.is_none()
            && self.file_confirmation.is_none()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.request_leave(Leave::Close, &ctx);
        }
        egui::Panel::top("commit_controls").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.loading && !self.action_busy(),
                        egui::Button::new(text("CommitRefresh")),
                    )
                    .clicked()
                {
                    self.refresh(&ctx);
                }
                if let Some(branch) = self
                    .context
                    .as_ref()
                    .and_then(|c| c.repository.branch.as_deref())
                {
                    ui.label(branch);
                }
                if self.progress().is_none()
                    && let Some(status) = &self.status
                {
                    ui.label(status);
                }
            });
            if let Some(error) = &self.snapshot_error {
                let message = if self.context.is_some() {
                    "CommitPreviousStatus"
                } else {
                    "CommitStatusUnavailable"
                };
                let title = egui::RichText::new(text(message)).color(ui.visuals().error_fg_color);
                egui::CollapsingHeader::new(title).show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(100.).show(ui, |ui| {
                        ui.add(egui::Label::new(error).selectable(true).wrap());
                    });
                });
            }
        });
        let message = egui::Panel::bottom(if self.inline {
            "inline_commit_message"
        } else {
            "commit_message"
        })
        .default_size(self.message_height)
        .min_size(140.)
        .resizable(true)
        .show(ui, |ui| {
            if let Some(error) = &self.draft_error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            let subject_label = ui.label(text("CommitSubject"));
            let subject = ui
                .add(
                    egui::TextEdit::singleline(&mut self.draft.subject)
                        .desired_width(f32::INFINITY),
                )
                .labelled_by(subject_label.id);
            let body_label = ui.label(text("CommitDescription"));
            let body = ui
                .add(
                    egui::TextEdit::multiline(&mut self.draft.description)
                        .desired_width(f32::INFINITY)
                        .desired_rows(3),
                )
                .labelled_by(body_label.id);
            if subject.changed() || body.changed() {
                self.edited();
            }
            ui.horizontal(|ui| {
                let allow_amend = self.context.as_ref().is_some_and(|c| {
                    c.repository.head.is_some() && c.repository.operations.is_empty()
                });
                if ui
                    .add_enabled(
                        allow_amend && self.working_ready(),
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
                let push = ui.input(|input| input.modifiers.shift);
                let label = if push { "CommitAndPush" } else { "Commit" };
                let target = self
                    .push_target()
                    .map(|t| strings::format("CommitPushTarget", &[&t.remote, &t.branch]));
                let reason = self.commit_reason(push);
                if ui
                    .add_enabled(reason.is_none(), egui::Button::new(text(label)))
                    .on_hover_text(if push {
                        target.clone().unwrap_or_else(|| text("CommitNoUpstream"))
                    } else {
                        text("CommitReady")
                    })
                    .on_disabled_hover_text(text(reason.unwrap_or(label)))
                    .clicked()
                {
                    self.request_commit(push, &ctx);
                }
                if push {
                    let target = target.unwrap_or_else(|| text("CommitNoUpstream"));
                    ui.add(egui::Label::new(egui::RichText::new(&target).small()).truncate())
                        .on_hover_text(target);
                }
                if let Some(progress) = self.progress() {
                    ui.add(egui::Spinner::new().size(14.));
                    ui.add(egui::Label::new(egui::RichText::new(&progress).small()).truncate())
                        .on_hover_text(progress);
                }
            });
        });
        self.message_height = message.response.rect.height();
        if !self.operation_output.is_empty() {
            egui::Panel::bottom("commit-output").show(ui, |ui| {
                egui::CollapsingHeader::new(text("ActionOutput")).show(ui, |ui| {
                    egui::ScrollArea::both().max_height(150.).show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&self.operation_output).monospace(),
                            )
                            .selectable(true),
                        );
                    });
                });
            });
        }
        let files = egui::Panel::left(if self.inline {
            "inline_commit_files"
        } else {
            "commit_files"
        })
        .default_size(self.files_width)
        .max_size((ui.available_width() - 200.).max(220.))
        .min_size(220.)
        .resizable(true)
        .show(ui, |ui| {
            let mut chosen = None;
            let snapshot = self.context.clone();
            for staged in [false, true] {
                let rows = file_list::FileList::new(snapshot.as_deref(), staged);
                let count = rows.count();
                ui.horizontal(|ui| {
                    ui.strong(strings::format(
                        if staged {
                            "CommitStagedCount"
                        } else {
                            "CommitUnstagedCount"
                        },
                        &[&count.to_string()],
                    ));
                    let selected = self
                        .selection
                        .iter()
                        .any(|(group, _)| (*group == Group::Staged) == staged);
                    let all = ui.input(|i| i.modifiers.shift);
                    if ui
                        .add_enabled(
                            self.working_ready() && (selected || all && count > 0),
                            egui::Button::new(text(if staged {
                                "CommitUnstage"
                            } else {
                                "CommitStage"
                            })),
                        )
                        .on_hover_text(text(if staged {
                            "CommitUnstageHelp"
                        } else {
                            "CommitStageHelp"
                        }))
                        .clicked()
                    {
                        self.stage(staged, all, &ctx);
                    }
                    if ui
                        .add_enabled(
                            self.working_ready() && count > 0,
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
                let height = (ui.available_height() / if staged { 1. } else { 2. } - 32.).max(65.);
                let empty_snapshot = count == 0 && snapshot.is_some();
                egui::ScrollArea::vertical()
                    .id_salt(("files", staged))
                    .auto_shrink([false, false])
                    .max_height(height)
                    .min_scrolled_height(height)
                    .show_rows(
                        ui,
                        22.,
                        rows.row_count() + usize::from(empty_snapshot),
                        |ui, range| {
                            if empty_snapshot {
                                ui.weak(text(if staged {
                                    "CommitNoStaged"
                                } else {
                                    "CommitNoUnstaged"
                                }));
                                return;
                            }
                            for index in range {
                                let entry = match rows.row(index) {
                                    Some(file_list::Row::Heading(group, count)) => {
                                        file_list::heading(ui, group, count);
                                        continue;
                                    }
                                    Some(file_list::Row::File(entry)) => entry,
                                    None => continue,
                                };
                                let key = (entry.group, entry.path.clone());
                                let selected = self.selection.contains(&key);
                                let response = ui
                                    .push_id((entry.group as u8, &entry.path), |ui| {
                                        if self.working_blocked() {
                                            ui.disable();
                                        }
                                        file_list::file_row(ui, entry, selected)
                                    })
                                    .inner;
                                if response.clicked() {
                                    response.request_focus();
                                    self.files_focus = Some(response.id);
                                    let mut selection = self.selection.clone();
                                    if !ui.input(|i| i.modifiers.ctrl) {
                                        selection.clear();
                                    }
                                    if selected && ui.input(|i| i.modifiers.ctrl) {
                                        selection.remove(&key);
                                    } else {
                                        selection.insert(key);
                                    }
                                    chosen = Some(Leave::Select {
                                        entry: Box::new(entry.clone()),
                                        selection,
                                        stage: false,
                                    });
                                }
                                if response.double_clicked() {
                                    chosen = Some(Leave::Select {
                                        entry: Box::new(entry.clone()),
                                        selection: BTreeSet::from([(
                                            entry.group,
                                            entry.path.clone(),
                                        )]),
                                        stage: true,
                                    });
                                }
                                if response.secondary_clicked() && !self.action_busy() {
                                    response.request_focus();
                                    self.files_focus = Some(response.id);
                                    if let Some(context) = self.context.clone() {
                                        let entries = if selected {
                                            self.selected_entries(&context, staged)
                                        } else {
                                            vec![entry.clone()]
                                        };
                                        chosen = Some(Leave::Menu(Box::new(file_menu::Menu {
                                            selected: file_menu::Selection { context, entries },
                                            entry: entry.clone(),
                                            position: response
                                                .interact_pointer_pos()
                                                .unwrap_or(response.rect.left_bottom()),
                                            layer: ui.layer_id(),
                                        })));
                                    }
                                }
                            }
                        },
                    );
                ui.separator();
            }
            if let Some(next) = chosen {
                self.request_leave(next, &ctx);
            }
        });
        self.files_width = files.response.rect.width();
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(pane) = &mut self.conflict {
                let ready = !self.busy && !self.loading && self.snapshot_error.is_none();
                ui.add_enabled_ui(ready, |ui| pane.show(ui));
                return;
            }
            if let Some(entry) = &self.focused {
                ui.strong(&entry.path);
                if entry.group == Group::Conflicted
                    && ui
                        .add_enabled(
                            !self.action_busy() && !self.loading && self.snapshot_error.is_none(),
                            egui::Button::new(text("ConflictOpen")),
                        )
                        .clicked()
                {
                    self.open_conflict();
                    return;
                }
            }
            let mut options_changed = false;
            ui.horizontal(|ui| {
                options_changed |= ui
                    .checkbox(
                        &mut self.diff_options.ignore_whitespace,
                        text("DiffIgnoreWhitespace"),
                    )
                    .changed();
                options_changed |= ui
                    .checkbox(&mut self.diff_options.whole_file, text("DiffWholeFile"))
                    .changed();
            });
            if options_changed && let Some(entry) = self.focused.clone() {
                self.show_diff(entry, &ctx);
            }
            if let Some(error) = &self.diff_error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            } else if let Some(diff) = &self.diff {
                let ready = !self.busy && !self.loading && self.snapshot_error.is_none();
                if let Some(ids) = self.diff_view.show(ui, diff, ready)
                    && let Some(preview) = diff.hunks.clone()
                {
                    self.submit(Action::Hunks { preview, ids }, &ctx);
                }
            } else {
                ui.label(text(if self.focused.is_some() {
                    "DiffLoading"
                } else {
                    "DiffChooseFile"
                }));
            }
        });
        if let Some(review) = &self.confirm_amend {
            let mut confirmed = false;
            let mut dismissed = false;
            egui::Modal::new(egui::Id::unique("confirm_amend")).show(&ctx, |ui| {
                ui.label(strings::format(
                    "CommitConfirmAmend",
                    &[
                        review
                            .context
                            .repository
                            .branch
                            .as_deref()
                            .unwrap_or("HEAD"),
                        review.context.repository.head.as_deref().unwrap_or(""),
                    ],
                ));
                if let Some(target) = &review.push {
                    ui.label(strings::format(
                        "CommitPushTarget",
                        &[&target.remote, &target.branch],
                    ));
                }
                ui.horizontal(|ui| {
                    confirmed = ui.button(text("CommitAmend")).clicked();
                    dismissed = ui.button(text("ActionCancel")).clicked();
                });
            });
            if confirmed && let Some(review) = self.confirm_amend.take() {
                self.submit_commit(review, true, &ctx);
            } else if dismissed {
                self.confirm_amend = None;
            }
        }
        self.show_file_menu(&ctx);
        self.show_file_confirmation(&ctx);
        if enabled {
            self.file_shortcuts(&ctx);
        }
        if !self.inline || !self.exit_prompt_open() {
            self.show_leave_prompt(&ctx);
        }
    }

    pub fn show_leave_prompt(&mut self, ctx: &egui::Context) {
        if self.leave.is_some() {
            let mut answer = None;
            let response = egui::Modal::new(egui::Id::unique("conflict_leave")).show(ctx, |ui| {
                ui.heading(text("ConflictResolve"));
                if let Some(pane) = &self.conflict {
                    ui.label(strings::format("ConflictCloseEdits", &[&pane.path]));
                }
                ui.horizontal(|ui| {
                    if ui.button(text("RustDiscardConflictText")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(text("ActionCancel")).clicked() {
                        answer = Some(false);
                    }
                });
            });
            if response.should_close() && answer.is_none() {
                answer = Some(false);
            }
            if let Some(discard) = answer {
                self.answer_leave(discard, ctx);
            }
        }
    }
}

#[cfg(test)]
#[path = "commit_conflict_tests.rs"]
mod conflict_tests;
#[path = "commit_file_list.rs"]
mod file_list;
#[path = "commit_file_menu.rs"]
mod file_menu;
#[cfg(test)]
#[path = "commit_file_menu_tests.rs"]
mod file_menu_tests;
#[path = "commit_patch.rs"]
mod patch;
#[path = "commit_queue.rs"]
mod queue;
#[cfg(test)]
#[path = "commit_queue_tests.rs"]
mod queue_tests;
#[path = "commit_shell.rs"]
mod shell;
fn error_text(error: working::Error) -> String {
    crate::operations::error_log(&error.into())
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
    fn native_window_geometry_preserves_commit_size_and_panes_across_profile_reload() {
        use crate::window_geometry::{fixture_input, points};
        let temp = tempfile::tempdir().unwrap();
        let git = Arc::new(Git::discover(None, &Cancellation::default()).unwrap());
        let new_editor = || {
            Editor::new(
                temp.path().join("unused-repository"),
                git.clone(),
                Scheduler::default(),
                drafts::Store {
                    directory: temp.path().join("drafts"),
                    legacy_directory: None,
                },
            )
        };
        for zoom in [0.7, 1.1, 2.] {
            let ctx = egui::Context::default();
            ctx.set_zoom_factor(zoom);
            let native = egui::vec2(1188., 792.);
            let mut editor = new_editor();
            editor.files_width = 413.;
            editor.message_height = 237.;
            for _ in 0..2 {
                ctx.run_ui(fixture_input(native / zoom, 1.5, false), |ui| {
                    editor.observe_geometry(ui.ctx(), egui::ViewportClass::Immediate)
                })
                .drop_without_applying_deltas();
            }
            let mut settings = shovel_core::settings::Settings::default();
            editor.save_layout(&mut settings);
            let mut store = shovel_core::settings::SettingsStore::new(
                temp.path().join(format!("profile-{zoom}")),
            );
            store.load().unwrap();
            store.save(&settings).unwrap();
            let reloaded = store.load().unwrap();
            let mut reopened = new_editor();
            reopened.restore_layout(&reloaded);
            assert!((reopened.size - native).length() < 0.01);
            assert_eq!(reopened.files_width, 413.);
            assert_eq!(reopened.message_height, 237.);
            assert!((points(&ctx, reopened.size) * zoom - native).length() < 0.01);
            // Embedded test layouts must not overwrite the native Commit geometry.
            ctx.run_ui(fixture_input(egui::vec2(400., 300.), 1.5, false), |ui| {
                reopened.observe_geometry(ui.ctx(), egui::ViewportClass::EmbeddedWindow)
            })
            .drop_without_applying_deltas();
            assert!((reopened.size - native).length() < 0.01);
        }
    }

    #[test]
    fn branch_changes_refresh_attached_and_reopened_views_without_replacing_drafts() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("repo");
        std::fs::create_dir(&root).unwrap();
        let config = temporary.path().join("isolated.config");
        std::fs::write(
            &config,
            "[user]\nname=Fixture\nemail=fixture@example.invalid\n[commit]\ngpgsign=false\n",
        )
        .unwrap();
        let mut git = Git::discover(None, &Cancellation::default()).unwrap();
        git.environment
            .insert("GIT_CONFIG_GLOBAL".into(), Some(config.into_os_string()));
        git.environment
            .insert("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()));
        command(&git, &root, &["init", "--initial-branch=main"]);
        command(&git, &root, &["commit", "--allow-empty", "-m", "base"]);
        command(&git, &root, &["branch", "target"]);
        let mut editor = Editor::new(
            root.clone(),
            Arc::new(git),
            Scheduler::default(),
            drafts::Store {
                directory: temporary.path().join("drafts"),
                legacy_directory: None,
            },
        );
        let ctx = egui::Context::default();
        editor.initialize(&ctx);
        settle(&mut editor, &ctx, |e| !e.loading && e.draft_loaded);
        editor.draft.subject = "retained text".into();
        editor.draft.description = "retained body".into();
        editor.edited();
        command(&editor.git, &root, &["switch", "target"]);
        editor.external_changed();
        settle(&mut editor, &ctx, |e| {
            !e.loading
                && e.context
                    .as_ref()
                    .is_some_and(|c| c.repository.branch.as_deref() == Some("target"))
        });
        assert_eq!(editor.draft.subject, "retained text");
        assert_eq!(editor.draft.description, "retained body");
        editor.open = false;
        command(&editor.git, &root, &["switch", "main"]);
        editor.external_changed();
        editor.poll(&ctx);
        assert!(!editor.loading);
        assert!(editor.refresh_pending);
        editor.activate();
        settle(&mut editor, &ctx, |e| {
            !e.loading
                && !e.refresh_pending
                && e.context
                    .as_ref()
                    .is_some_and(|c| c.repository.branch.as_deref() == Some("main"))
        });
        assert_eq!(editor.draft.subject, "retained text");
        settle(&mut editor, &ctx, |e| !e.draft_dirty && !e.draft_saving);
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
        editor.request_commit(false, &ctx);
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
        editor.request_commit(false, &ctx);
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
