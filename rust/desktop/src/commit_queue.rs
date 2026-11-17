//! Captured working-copy requests remain owned by the editor after its view closes.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Mutex;

#[derive(Clone)]
pub(crate) struct OwnedActivity {
    pub request: u64,
    pub label: &'static str,
    pub status: String,
    pub output: String,
    pub done: bool,
    pub failed: bool,
    pub reopen_commit: bool,
}
struct Entry {
    activity: OwnedActivity,
    stage: bool,
    dirty: bool,
    capture: Capture,
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

/// Redact complete lines, preserving independent stdout/stderr chunk boundaries.
/// Never expose an incomplete bounded line: a URL credential may span chunks.
#[derive(Default)]
pub(crate) struct Capture {
    lines: [Vec<u8>; 2],
    captured: usize,
    overflow: bool,
    pub(crate) text: String,
}
impl Capture {
    pub(crate) fn append(&mut self, stderr: bool, bytes: &[u8]) {
        let room = (1024 * 1024usize).saturating_sub(self.captured);
        self.overflow |= bytes.len() > room;
        let bytes = &bytes[..bytes.len().min(room)];
        self.captured += bytes.len();
        let pending = &mut self.lines[usize::from(stderr)];
        pending.extend_from_slice(bytes);
        if let Some(end) = pending.iter().rposition(|b| matches!(b, b'\r' | b'\n')) {
            let complete: Vec<_> = pending.drain(..=end).collect();
            self.publish(&complete);
        }
    }
    fn publish(&mut self, bytes: &[u8]) {
        let redacted = shovel_core::network::redact(&String::from_utf8_lossy(bytes));
        let room = 32768usize.saturating_sub(self.text.len());
        let mut end = redacted.len().min(room);
        while !redacted.is_char_boundary(end) {
            end -= 1;
        }
        self.text.push_str(&redacted[..end]);
    }
    pub(crate) fn finish(&mut self) {
        if !self.overflow {
            for index in 0..2 {
                let bytes = std::mem::take(&mut self.lines[index]);
                self.publish(&bytes);
            }
        }
        self.lines = Default::default();
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
        && entries.iter().all(|entry| owned.entries.contains(entry))
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
        self.context.is_some()
            && !self.working_blocked()
            && (!self.loading || self.has_working_queue())
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
        if !self.working_ready() {
            return;
        }
        let context = self.context.clone().unwrap();
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
            Lane::WorkingCopy,
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
            Action::Commit { .. } => "Commit",
            Action::Hunks { preview, .. } if preview.staged() => "CommitPhaseUnstage",
            Action::Hunks { .. } => "CommitPhaseStage",
        };
        let committing = matches!(action, Action::Commit { .. });
        let submitted = committing.then(|| self.draft.clone());
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
                    reopen_commit: false,
                },
                stage: matches!(action, Action::Stage(_)),
                dirty: true,
                capture: Capture::default(),
            },
        );
        self.busy = true;
        self.status = Some(text("OperationWaiting"));
        if committing {
            self.flush(ctx);
            self.open = false;
        }
        let git = self.git.clone();
        let tx = self.sender.clone();
        let ctx = ctx.clone();
        let owned = self.queue.owned.clone();
        self.scheduler.activate(request, move || {
            let _ = tx.send(Event::WorkingStarted(id));
            ctx.request_repaint();
            let context = adopt_owned(&context, owned.lock().unwrap().as_ref(), &action);
            let output_tx = tx.clone();
            let output_ctx = ctx.clone();
            let published = Arc::new(Mutex::new(0usize));
            let callback: OutputCallback = Arc::new(move |stderr, bytes| {
                let mut count = published.lock().unwrap();
                // One extra byte marks truncation without unbounded event retention.
                let room = (1024 * 1024 + 1usize).saturating_sub(*count);
                if room > 0 {
                    let bytes = &bytes[..room.min(bytes.len())];
                    *count += bytes.len();
                    let _ = output_tx.send(Event::WorkingOutput(id, stderr, bytes.to_vec()));
                    output_ctx.request_repaint();
                }
            });
            let result = git.execute_working(&context, &action, Some(callback));
            let outcome = if result
                .as_ref()
                .is_ok_and(|r| r.succeeded() && r.updated.is_ok())
            {
                if let Ok(completed) = &result {
                    *owned.lock().unwrap() =
                        completed.updated.as_ref().ok().map(|c| Arc::new(c.clone()));
                }
                Outcome::Succeeded
            } else {
                Outcome::Failed
            };
            let _ = tx.send(Event::WorkingCompleted(id, Box::new(result), submitted));
            ctx.request_repaint();
            outcome
        });
    }
    pub(super) fn working_started(&mut self, id: u64) {
        if let Some(entry) = self.queue.entries.get_mut(&id) {
            entry.activity.status = text(entry.activity.label);
            entry.dirty = true;
            self.status = Some(entry.activity.status.clone());
            self.output.clear();
            // Retain rows and selections, but invalidate obsolete read publication.
            self.generation += 1;
            self.diff_generation += 1;
            self.diff_cancel.cancel();
            self.loading = false;
            self.diff = None;
            self.diff_deferred = self.focused.is_some();
        }
    }
    pub(super) fn working_output(&mut self, id: u64, stderr: bool, bytes: &[u8]) {
        if let Some(entry) = self.queue.entries.get_mut(&id) {
            entry.capture.append(stderr, bytes);
            entry.activity.output.clone_from(&entry.capture.text);
            self.output.clone_from(&entry.activity.output);
            entry.dirty = true;
        }
    }
    pub(super) fn working_completed(
        &mut self,
        id: u64,
        result: Result<working::Completed, working::Error>,
        submitted: Option<drafts::Text>,
        ctx: &egui::Context,
    ) {
        self.refresh_history = true;
        if let Some(entry) = self.queue.entries.get_mut(&id) {
            entry.activity.failed = !result
                .as_ref()
                .is_ok_and(|r| r.succeeded() && r.updated.is_ok());
            entry.activity.reopen_commit =
                submitted.is_some() && !result.as_ref().is_ok_and(|r| r.succeeded());
            if let Err(working::Error::Git(shovel_core::git::Error::Command { output, .. })) =
                &result
            {
                entry.capture.append(true, output.as_bytes());
            }
        }
        match result {
            Ok(result) => {
                if result.succeeded() {
                    self.status = Some(
                        if result.command.outcome == shovel_core::process::Outcome::OutputLimit {
                            format!(
                                "{} {}",
                                text("ActionCompletedShort"),
                                text("ActivityOutputLimit")
                            )
                        } else {
                            text("ActionCompletedShort")
                        },
                    );
                    if let Some(submitted) = submitted {
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
                    Ok(context) => self.apply(context, ctx),
                    Err(error) => {
                        self.status = Some(format!(
                            "{}\n{}",
                            self.status.as_deref().unwrap_or_default(),
                            crate::operations::error_log(&error.into())
                        ))
                    }
                }
            }
            Err(error) => {
                self.status = Some(error_text(error));
                self.failed_commit |= submitted.is_some();
            }
        }
        self.finish_working(id);
    }
    pub(super) fn working_removed(&mut self, id: u64, outcome: Outcome) {
        self.status = Some(text(if outcome == Outcome::Removed {
            "QueuedRequestCancelled"
        } else {
            "ActionStoppedShort"
        }));
        self.finish_working(id);
    }
    fn finish_working(&mut self, id: u64) {
        if let Some(entry) = self.queue.entries.get_mut(&id) {
            entry.capture.finish();
            entry.activity.output.clone_from(&entry.capture.text);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streamed_activity_redacts_chunked_urls_and_never_publishes_truncated_lines() {
        let mut capture = Capture::default();
        capture.append(true, b"failure https://user:sec");
        assert!(capture.text.is_empty());
        capture.append(true, b"ret@host/path?token=hidden\n");
        assert_eq!(
            capture.text,
            "failure https://[redacted]@host/path?[redacted]\n"
        );
        capture.append(false, "completed \u{2713}".as_bytes());
        capture.finish();
        assert!(capture.text.ends_with("completed \u{2713}"));
        assert!(!capture.text.contains("secret"));
        let mut overflow = Capture::default();
        overflow.append(true, b"https://user:");
        overflow.append(true, &vec![b'x'; 1024 * 1024]);
        overflow.finish();
        assert!(overflow.text.is_empty());
        assert!(overflow.lines.iter().all(Vec::is_empty));
    }
}
