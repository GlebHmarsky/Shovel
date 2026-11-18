//! Captured working-copy requests remain owned by the editor after its view closes.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Mutex;

pub(super) struct WorkingResult {
    command: Result<working::Completed, working::Error>,
    output: String,
    submitted: Option<(String, drafts::Text)>,
    push: Option<PushReport>,
}
struct PushReport {
    status: String,
    output: String,
    failed: bool,
}
impl PushReport {
    fn after_commit(
        git: &Git,
        context: &Context,
        completed: &mut working::Completed,
        target: &Target,
        authenticate: impl FnOnce() -> Result<
            shovel_core::authentication::Session,
            shovel_core::operation::Error,
        >,
    ) -> Self {
        let Some(commit) = &completed.commit else {
            return Self {
                status: text("CommitSucceededUnknownTip"),
                output: String::new(),
                failed: true,
            };
        };
        if completed.updated.is_err() {
            return Self::failed(commit, &text("ActionRefreshFailedShort"));
        }
        let pushed = authenticate()
            .and_then(|auth| git.push_created_commit(context, commit, target, Some(&auth)));
        // A native pre-push hook can change the index or worktree.
        completed.updated = git.refresh_working_copy(&context.repository, &Cancellation::default());
        Self::from_result(commit, pushed)
    }
    fn failed(commit: &str, reason: &str) -> Self {
        Self {
            status: strings::format("CommitCreatedPushFailed", &[commit, reason]),
            output: String::new(),
            failed: true,
        }
    }
    fn from_result(
        commit: &str,
        result: Result<shovel_core::network::Completed, shovel_core::operation::Error>,
    ) -> Self {
        let pushed = match result {
            Ok(result) => result,
            Err(error) => {
                let mut report = Self::failed(commit, &text(&error.code()));
                report.output = crate::operations::error_log(&error);
                return report;
            }
        };
        let sent = !pushed.commands.is_empty()
            && pushed
                .commands
                .iter()
                .all(shovel_core::network::Command::succeeded);
        let reason = pushed
            .authentication_failure
            .map(crate::operations::authentication_error)
            .or(pushed.message.as_deref())
            .map(text);
        let mut report = if sent {
            Self {
                status: strings::format("CommitCreatedPushed", &[commit]),
                output: String::new(),
                failed: !pushed.succeeded() || reason.is_some(),
            }
        } else {
            Self::failed(
                commit,
                &reason.clone().unwrap_or_else(|| text("ActionFailedShort")),
            )
        };
        if sent && report.failed {
            let detail = reason.unwrap_or_else(|| text("ActionRefreshFailedShort"));
            report.status.push('\n');
            report.status.push_str(&detail);
        }
        report.output = pushed.output;
        if let Err(error) = pushed.updated {
            report.output.push('\n');
            report
                .output
                .push_str(&crate::operations::error_log(&error));
        }
        if pushed.output_truncated {
            report.output.push('\n');
            report.output.push_str(&text("ActivityOutputLimit"));
        }
        report
    }
}
pub(crate) struct FailedCommit {
    pub owner: String,
    pub message: String,
}
#[derive(Clone)]
pub(crate) struct OwnedActivity {
    pub request: u64,
    pub label: &'static str,
    pub status: String,
    pub output: String,
    pub done: bool,
    pub failed: bool,
    pub reopen_commit: Option<Arc<FailedCommit>>,
}
struct Entry {
    activity: OwnedActivity,
    stage: bool,
    dirty: bool,
}
#[derive(Default)]
pub(super) struct Queue {
    next: u64,
    entries: BTreeMap<u64, Entry>,
    // Published by the worker before the scheduler admits its successor, not
    // by a view poll which may be delayed while every tab/window is closed.
    owned: Arc<Mutex<Option<Arc<Context>>>>,
}
impl Queue {
    fn pending(&self) -> bool {
        self.entries.values().any(|e| !e.activity.done)
    }
    pub(super) fn has_stage(&self) -> bool {
        self.entries.values().any(|e| !e.activity.done && e.stage)
    }
}

fn adopt_owned(
    captured: &Arc<Context>,
    owned: Option<&Arc<Context>>,
    action: &Action,
) -> Arc<Context> {
    let Some(owned) = owned else {
        return captured.clone();
    };
    let entries = match action {
        Action::Stage(entries) | Action::Unstage(entries) => entries.as_slice(),
        Action::Commit { .. } => &[],
        Action::Hunks { .. } => return captured.clone(),
    };
    if owned.index_stamp != captured.index_stamp
        && owned.repository.identity == captured.repository.identity
        && owned.repository.branch == captured.repository.branch
        && owned.repository.head == captured.repository.head
        && owned.config_stamp == captured.config_stamp
        && entries
            .iter()
            .all(|entry| owned.change(entry.group, &entry.path) == Some(entry))
    {
        owned.clone()
    } else {
        captured.clone()
    }
}

fn action_key(action: &Action) -> String {
    let mut digest = Sha256::new();
    match action {
        Action::Stage(entries) | Action::Unstage(entries) => {
            digest.update(if matches!(action, Action::Stage(_)) {
                b"stage".as_slice()
            } else {
                b"unstage".as_slice()
            });
            for entry in entries {
                digest.update(entry.path.len().to_le_bytes());
                digest.update(entry.path.as_bytes());
            }
        }
        Action::Commit { message, amend } => {
            digest.update(b"commit");
            digest.update([u8::from(*amend)]);
            digest.update(message.as_bytes());
        }
        Action::Hunks { preview, ids } => {
            digest.update(b"hunks");
            // A retained immutable preview is the hunk action identity.
            digest.update((Arc::as_ptr(preview) as usize).to_le_bytes());
            for id in ids {
                digest.update(id.to_le_bytes());
            }
        }
    }
    format!("working:{:x}", digest.finalize())
}

impl Editor {
    pub fn has_working_queue(&self) -> bool {
        self.queue.pending()
    }
    pub(super) fn working_blocked(&self) -> bool {
        (self.busy && !self.has_working_queue())
            || self.patch_picking
            || self.conflict.as_ref().is_some_and(|pane| pane.mutating)
    }
    pub(super) fn working_ready(&self) -> bool {
        self.current_context().is_some()
            && !self.working_blocked()
            && (!self.loading || self.target_rows_current || self.has_working_queue())
    }
    pub fn take_activity(&mut self) -> Vec<OwnedActivity> {
        let result = self
            .queue
            .entries
            .values_mut()
            .filter_map(|e| std::mem::take(&mut e.dirty).then(|| e.activity.clone()))
            .collect();
        self.queue.entries.retain(|_, e| !e.activity.done);
        result
    }
    pub(super) fn submit(&mut self, action: Action, ctx: &egui::Context) {
        let Some(context) = self.context.clone() else {
            return;
        };
        let submitted = matches!(action, Action::Commit { .. }).then(|| self.draft.clone());
        self.submit_captured(action, context, submitted, None, ctx);
    }
    pub(super) fn submit_captured(
        &mut self,
        action: Action,
        context: Arc<Context>,
        submitted: Option<drafts::Text>,
        push: Option<Target>,
        ctx: &egui::Context,
    ) {
        if !self.working_ready() {
            self.status = Some(text("CommitWaitForRead"));
            return;
        }
        if !self.has_working_queue() {
            // A completed batch no longer owns the current review. In particular,
            // never replace a fresh scan of external staging with an older result.
            self.queue.owned.lock().unwrap().take();
        }
        let id = self.queue.next;
        self.queue.next += 1;
        let dropped = self.sender.clone();
        let drop_ctx = ctx.clone();
        // Hash immutable options; scheduler keys never retain commit text or paths.
        let key = action_key(&action);
        let (request, accepted) = self.scheduler.reserve(
            context.repository.identity.fingerprint.clone(),
            shovel_core::case::path_key(&context.repository.common_directory),
            key,
            if push.is_some() {
                Lane::Exclusive
            } else {
                Lane::WorkingCopy
            },
            move |outcome| {
                let _ = dropped.send(Event::WorkingRemoved(id, outcome));
                drop_ctx.request_repaint();
            },
        );
        if !accepted {
            return;
        }
        let label = match &action {
            Action::Stage(_) => "CommitPhaseStage",
            Action::Unstage(_) => "CommitPhaseUnstage",
            Action::Commit { .. } if push.is_some() => "CommitAndPush",
            Action::Commit { .. } => "Commit",
            Action::Hunks { preview, .. } if preview.staged() => "CommitPhaseUnstage",
            Action::Hunks { .. } => "CommitPhaseStage",
        };
        let committing = matches!(action, Action::Commit { .. });
        self.queue.entries.insert(
            id,
            Entry {
                activity: OwnedActivity {
                    request,
                    label,
                    status: text("OperationWaiting"),
                    output: String::new(),
                    done: false,
                    failed: false,
                    reopen_commit: None,
                },
                stage: matches!(action, Action::Stage(_)),
                dirty: true,
            },
        );
        self.busy = true;
        self.status = Some(text("OperationWaiting"));
        if committing {
            self.flush(ctx);
            if !self.inline {
                self.open = false;
            }
        }
        let git = self.git.clone();
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        let owned = self.queue.owned.clone();
        let authentication = self.authentication.clone();
        self.scheduler.activate(request, move || {
            let _ = tx.send(Event::WorkingStarted(id));
            ctx.request_repaint();
            let context = adopt_owned(&context, owned.lock().unwrap().as_ref(), &action);
            let output_tx = tx.clone();
            let output_ctx = ctx.clone();
            let stream = Arc::new(crate::activity_output::OutputStream::default());
            let callback = stream.callback(move |stream| {
                let _ = output_tx.send(Event::WorkingOutput(id, stream));
                output_ctx.request_repaint();
            });
            let mut result = git.execute_working_for_editor(&context, &action, Some(callback));
            let push = if let (Ok(completed), Some(target)) = (&mut result, push)
                && completed.succeeded()
            {
                let _ = tx.send(Event::WorkingPushing(id));
                ctx.request_repaint();
                Some(PushReport::after_commit(
                    &git,
                    &context,
                    completed,
                    &target,
                    || {
                        authentication
                            .ok_or(shovel_core::operation::Error::Code("ActionAskPassMissing"))?
                            .start(
                                request,
                                context.repository.worktree.clone(),
                                "CommitAndPush",
                                &ctx,
                            )
                    },
                ))
            } else {
                None
            };
            let detail = if let Err(working::Error::Git(shovel_core::git::Error::Command {
                output,
                ..
            })) = &result
            {
                output.as_str()
            } else {
                push.as_ref().map_or("", |report| report.output.as_str())
            };
            let output = stream.finish(
                result
                    .as_ref()
                    .is_ok_and(|r| r.command.outcome == shovel_core::process::Outcome::Exited),
                detail,
            );
            let outcome = if result
                .as_ref()
                .is_ok_and(|r| r.succeeded() && r.updated.is_ok())
                && push.as_ref().is_none_or(|p| !p.failed)
            {
                if let Ok(completed) = &result {
                    *owned.lock().unwrap() =
                        completed.updated.as_ref().ok().map(|c| Arc::new(c.clone()));
                }
                Outcome::Succeeded
            } else {
                Outcome::Failed
            };
            let _ = tx.send(Event::WorkingCompleted(
                id,
                Box::new(WorkingResult {
                    command: result,
                    output,
                    submitted: submitted
                        .map(|text| (context.repository.identity.fingerprint.clone(), text)),
                    push,
                }),
            ));
            ctx.request_repaint();
            outcome
        });
    }
    pub(super) fn working_started(&mut self, id: u64) {
        if let Some(entry) = self.queue.entries.get_mut(&id) {
            entry.activity.status = text(
                if matches!(entry.activity.label, "Commit" | "CommitAndPush") {
                    "CommitRunning"
                } else {
                    entry.activity.label
                },
            );
            entry.dirty = true;
            self.status = Some(entry.activity.status.clone());
            self.output.clear();
            // Retain rows and selections, but invalidate obsolete read publication.
            self.generation += 1;
            self.scan_cancel.cancel();
            self.refresh_pending |= self.loading;
            self.diff_generation += 1;
            self.diff_cancel.cancel();
            self.loading = false;
            self.diff = None;
            self.diff_deferred = self.focused.is_some();
        }
    }
    pub(super) fn working_pushing(&mut self, id: u64) {
        if let Some(entry) = self.queue.entries.get_mut(&id) {
            entry.activity.status = text("ActionStagePushing");
            self.status = Some(entry.activity.status.clone());
            entry.dirty = true;
        }
    }
    pub(super) fn working_output(&mut self, id: u64, added: &str) {
        if let Some(entry) = self.queue.entries.get_mut(&id) {
            entry.activity.output.push_str(added);
            self.output.clone_from(&entry.activity.output);
            entry.dirty = true;
        }
    }
    pub(super) fn working_completed(
        &mut self,
        id: u64,
        completed: WorkingResult,
        ctx: &egui::Context,
    ) {
        let WorkingResult {
            command: result,
            output,
            submitted,
            push,
        } = completed;
        self.refresh_history = true;
        if let Some(entry) = self.queue.entries.get_mut(&id) {
            entry.activity.failed = push.as_ref().is_some_and(|p| p.failed)
                || !result
                    .as_ref()
                    .is_ok_and(|r| r.succeeded() && r.updated.is_ok());
            if !result.as_ref().is_ok_and(|r| r.succeeded())
                && let Some((owner, draft)) = &submitted
            {
                entry.activity.reopen_commit = Some(Arc::new(FailedCommit {
                    owner: owner.clone(),
                    message: draft.message(),
                }));
            }
        }
        match result {
            Ok(result) => {
                if result.succeeded() {
                    self.status = Some(if let Some(report) = push {
                        report.status
                    } else {
                        let summary = result.commit.as_ref().map_or_else(
                            || text("ActionCompletedShort"),
                            |commit| strings::format("CommitCreated", &[commit]),
                        );
                        if result.command.outcome == shovel_core::process::Outcome::OutputLimit {
                            format!("{} {}", summary, text("ActivityOutputLimit"))
                        } else {
                            summary
                        }
                    });
                    if let Some((_, submitted)) = submitted {
                        self.failed_commit = false;
                        if self.draft == submitted {
                            self.draft = drafts::Text::default();
                            self.edited();
                            self.amend = false;
                        }
                    }
                } else {
                    self.status = Some(text("ActionFailedShort"));
                    self.failed_commit |= submitted.is_some();
                }
                match result.updated {
                    Ok(context) => {
                        self.apply(context, ctx);
                        self.target_rows_current = result.needs_full_scan;
                        self.refresh_pending |= result.needs_full_scan;
                    }
                    Err(error) => self.failed_reinspection(error_text(error)),
                }
            }
            Err(error) => {
                if matches!(error, working::Error::Stale) {
                    self.snapshot_error = Some(text("CommitStale"));
                }
                self.status = Some(error_text(error));
                self.failed_commit |= submitted.is_some();
            }
        }
        self.finish_working(id, output);
    }
    pub(super) fn working_removed(&mut self, id: u64, outcome: Outcome) {
        let Some(entry) = self
            .queue
            .entries
            .get_mut(&id)
            .filter(|entry| !entry.activity.done)
        else {
            return;
        };
        if outcome == Outcome::WorkerFailed {
            let message = text("ActionWorkerFailed");
            self.status = Some(message.clone());
            self.snapshot_error = Some(message);
            self.refresh_pending = true;
            self.refresh_history = true;
            entry.activity.failed = true;
            let output = std::mem::take(&mut entry.activity.output);
            self.finish_working(id, output);
            return;
        }
        self.status = Some(text(if outcome == Outcome::Removed {
            "QueuedRequestCancelled"
        } else {
            "ActionStoppedShort"
        }));
        self.finish_working(id, String::new());
    }
    fn finish_working(&mut self, id: u64, output: String) {
        if let Some(entry) = self.queue.entries.get_mut(&id) {
            entry.activity.output = output;
            self.output.clone_from(&entry.activity.output);
            entry.activity.status = self.status.clone().unwrap_or_default();
            entry.activity.done = true;
            entry.dirty = true;
        }
        self.busy = self.has_working_queue();
        // Test/headless callers may not drain Activity; apply the same completed
        // history budget, exempting every accepted pending request.
        while self
            .queue
            .entries
            .values()
            .filter(|e| e.activity.done)
            .count()
            > 128
        {
            let id = *self
                .queue
                .entries
                .iter()
                .find(|(_, e)| e.activity.done)
                .unwrap()
                .0;
            self.queue.entries.remove(&id);
        }
    }
}
