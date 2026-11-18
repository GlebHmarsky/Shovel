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
    interval: Option<Duration>,
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
            interval: None,
            active: Default::default(),
            tx,
            rx,
            activity: vec![],
            next: 1,
        }
    }
}
impl Controller {
    #[cfg(test)]
    pub(crate) fn due_now(&mut self, path: PathBuf) {
        self.due
            .insert(path, Instant::now() - Duration::from_secs(1));
    }
    pub fn drained(&self) -> bool {
        self.active.is_empty()
    }
    pub fn forget(&mut self, path: &std::path::Path) {
        self.due
            .retain(|p, _| !crate::Workspace::same_path(p, path));
    }
    pub fn cancel(&self) {
        for cancel in self.active.values() {
            cancel.cancel();
        }
    }
    pub fn tick<'a>(
        &mut self,
        ctx: &egui::Context,
        enabled: bool,
        seconds: u64,
        snapshots: impl Iterator<Item = &'a Snapshot>,
        git: Option<Arc<Git>>,
        scheduler: &Scheduler,
    ) -> Vec<PathBuf> {
        let mut changed = vec![];
        let now = Instant::now();
        let interval = Duration::from_secs(seconds);
        while let Ok((id, path, ok, message)) = self.rx.try_recv() {
            self.active.remove(&id);
            if let Some(due) = self.due.get_mut(&path) {
                *due = now + interval;
            }
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
        if !enabled {
            self.interval = None;
            self.due.clear();
            self.cancel();
            return changed;
        }
        if self
            .interval
            .replace(interval)
            .is_some_and(|old| old != interval)
        {
            self.due.clear();
        }
        if scheduler
            .active_records()
            .iter()
            .any(|r| !r.key.starts_with("automatic-fetch:"))
        {
            self.cancel();
            return changed;
        }
        let Some(git) = git else { return changed };
        for snapshot in snapshots {
            if self.active.len() >= 2 {
                break;
            }
            if snapshot.remotes.is_empty() {
                continue;
            }
            let Some(due) = self.due.get_mut(&snapshot.worktree) else {
                self.due.insert(snapshot.worktree.clone(), now + interval);
                ctx.request_repaint_after(interval);
                continue;
            };
            if *due > now {
                ctx.request_repaint_after(*due - now);
                continue;
            }
            *due = now + interval;
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
                shovel_core::case::path_key(&snapshot.common_directory),
                format!("automatic-fetch:{}", path.display()),
                Lane::Fetch,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_the_interval_restarts_the_timer_and_tab_closure_releases_it() {
        let fixture = crate::history_view::tests::Fixture::new();
        fixture.command(&["remote", "add", "origin", "unused-local-remote"], &[]);
        let snapshot = fixture
            .git
            .inspect(&fixture.root, &Cancellation::default())
            .unwrap();
        let path = &snapshot.worktree;
        let git = Arc::new(fixture.git);
        let ctx = egui::Context::default();
        let scheduler = Scheduler::default();
        let mut controller = Controller::default();
        let tick = |controller: &mut Controller, seconds| {
            controller.tick(
                &ctx,
                true,
                seconds,
                std::iter::once(&snapshot),
                Some(git.clone()),
                &scheduler,
            );
        };
        tick(&mut controller, 60);
        let initial = controller.due[path];
        tick(&mut controller, 60);
        assert_eq!(controller.due[path], initial);
        controller.due_now(path.clone());
        let before = Instant::now();
        tick(&mut controller, 10);
        assert!(controller.due[path] >= before + Duration::from_secs(10));
        assert!(scheduler.records().is_empty());
        controller.tick(
            &ctx,
            false,
            10,
            std::iter::from_fn(|| panic!("disabled fetch visited a tab")),
            Some(git.clone()),
            &scheduler,
        );
        assert!(controller.due.is_empty());
        tick(&mut controller, 10);
        controller.forget(path);
        // A worker may finish after its tab closes. Keep its actual outcome
        // in Activity without restoring a timer for the closed repository.
        controller
            .tx
            .send((1, path.clone(), true, String::new()))
            .unwrap();
        let changed = controller.tick(
            &ctx,
            true,
            10,
            std::iter::empty(),
            Some(git.clone()),
            &scheduler,
        );
        assert_eq!(changed.as_slice(), std::slice::from_ref(path));
        assert!(controller.due.is_empty());
        assert_eq!(controller.activity.len(), 1);
    }
}
