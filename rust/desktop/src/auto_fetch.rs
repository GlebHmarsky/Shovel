//! Background fetch owns cancellable work; accepted user commands preempt it.
use eframe::egui;
use shovel_core::{
    git::{Git, Snapshot},
    process::Cancellation,
    scheduler::{Lane, Outcome, Scheduler},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
pub struct Controller {
    due: HashMap<PathBuf, Instant>,
    active: HashMap<u64, Cancellation>,
    tx: mpsc::Sender<(u64, PathBuf, bool, String)>,
    rx: mpsc::Receiver<(u64, PathBuf, bool, String)>,
    pub activity: Vec<(PathBuf, String)>,
    next: u64,
}
impl Default for Controller {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            due: Default::default(),
            active: Default::default(),
            tx,
            rx,
            activity: vec![],
            next: 1,
        }
    }
}
impl Controller {
    pub fn cancel(&self) {
        for cancel in self.active.values() {
            cancel.cancel();
        }
    }
    pub fn tick(
        &mut self,
        ctx: &egui::Context,
        enabled: bool,
        seconds: u64,
        snapshots: Vec<Snapshot>,
        git: Option<Arc<Git>>,
        scheduler: &Scheduler,
    ) -> Vec<PathBuf> {
        let mut changed = vec![];
        while let Ok((id, path, ok, message)) = self.rx.try_recv() {
            self.active.remove(&id);
            self.due
                .insert(path.clone(), Instant::now() + Duration::from_secs(seconds));
            if ok {
                changed.push(path.clone());
            }
            let message = if message.is_empty() {
                crate::strings::text(if ok {
                    "RustAutoFetchDone"
                } else {
                    "RustAutoFetchFailed"
                })
            } else {
                message.chars().take(32768).collect()
            };
            self.activity.push((path, message));
            if self.activity.len() > 128 {
                self.activity.remove(0);
            }
        }
        let user_work = scheduler
            .records()
            .iter()
            .any(|r| r.outcome.is_none() && !r.key.starts_with("automatic-fetch:"));
        if !enabled || user_work {
            self.cancel();
            return changed;
        }
        ctx.request_repaint_after(Duration::from_secs(1));
        let Some(git) = git else { return changed };
        for snapshot in snapshots {
            if self.active.len() >= 2 {
                break;
            }
            if snapshot.remotes.is_empty() {
                continue;
            }
            let due = self
                .due
                .entry(snapshot.worktree.clone())
                .or_insert_with(|| Instant::now() + Duration::from_secs(seconds));
            if *due > Instant::now() {
                continue;
            }
            *due = Instant::now() + Duration::from_secs(seconds);
            let id = self.next;
            self.next += 1;
            let cancel = Cancellation::default();
            let token = cancel.clone();
            let tx = self.tx.clone();
            let discard = tx.clone();
            let g = git.clone();
            let ctx = ctx.clone();
            let dc = ctx.clone();
            let path = snapshot.worktree.clone();
            let dp = path.clone();
            let (_, accepted) = scheduler.submit(
                snapshot.identity.fingerprint.clone(),
                snapshot.common_directory.to_string_lossy().to_uppercase(),
                format!("automatic-fetch:{}", path.display()),
                Lane::Exclusive,
                move || {
                    let result = g
                        .prepare_network(&path, &token)
                        .and_then(|c| g.automatic_fetch(&c, &token));
                    let (ok, message) = match result {
                        Ok(r) => (r.succeeded() && !r.has_conflicts, r.output),
                        Err(e) => (false, crate::strings::text(&e.to_string())),
                    };
                    let _ = tx.send((id, path, ok, message));
                    ctx.request_repaint();
                    // A background failure must not clear queued user work.
                    Outcome::Succeeded
                },
                move |_| {
                    let _ = discard.send((
                        id,
                        dp,
                        false,
                        crate::strings::text("ActionResultCancelled"),
                    ));
                    dc.request_repaint();
                },
            );
            if accepted {
                self.active.insert(id, cancel);
            }
        }
        changed
    }
}
