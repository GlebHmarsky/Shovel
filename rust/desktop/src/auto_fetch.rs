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
pub struct Finished {
    pub path: PathBuf,
    pub succeeded: bool,
    pub status: &'static str,
    pub output: String,
}

pub struct Controller {
    due: HashMap<PathBuf, Instant>,
    interval: Option<Duration>,
    active: HashMap<u64, Cancellation>,
    tx: mpsc::Sender<(u64, Finished)>,
    rx: mpsc::Receiver<(u64, Finished)>,
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
    ) -> Vec<Finished> {
        let mut completed = vec![];
        let now = Instant::now();
        let interval = Duration::from_secs(seconds);
        while let Ok((id, result)) = self.rx.try_recv() {
            self.active.remove(&id);
            if let Some(due) = self.due.get_mut(&result.path) {
                *due = now + interval;
            }
            completed.push(result);
        }
        if !enabled {
            self.interval = None;
            self.due.clear();
            self.cancel();
            return completed;
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
            return completed;
        }
        let Some(git) = git else { return completed };
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
                    let (succeeded, output) = match result {
                        Ok(result) => {
                            let succeeded = result.succeeded() && !result.has_conflicts;
                            let mut details = result.output;
                            for key in [result.message, result.warning].into_iter().flatten() {
                                details.push_str(&format!("\n{}", crate::strings::text(&key)));
                            }
                            if result.output_truncated {
                                details.push_str(&format!(
                                    "\n{}",
                                    crate::strings::text("ActivityOutputLimit")
                                ));
                            }
                            if let Err(error) = result.updated {
                                details.push_str(&format!(
                                    "\n{}",
                                    crate::operations::error_log(&error)
                                ));
                            }
                            (succeeded, details)
                        }
                        Err(error) => (false, crate::operations::error_log(&error)),
                    };
                    let status = if succeeded {
                        "RustAutoFetchDone"
                    } else if token.is_cancelled() {
                        "ActionStoppedShort"
                    } else {
                        "RustAutoFetchFailed"
                    };
                    let _ = tx.send((
                        id,
                        Finished {
                            path,
                            succeeded,
                            status,
                            output,
                        },
                    ));
                    ctx.request_repaint();
                    // A background failure must not clear queued user work.
                    Outcome::Succeeded
                },
                move |_| {
                    let _ = discard.send((
                        id,
                        Finished {
                            path: dp,
                            succeeded: false,
                            status: "ActionStoppedShort",
                            output: crate::strings::text("ActionResultCancelled"),
                        },
                    ));
                    dc.request_repaint();
                },
            );
            if accepted {
                self.active.insert(id, cancel);
            }
        }
        completed
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
            .send((
                1,
                Finished {
                    path: path.clone(),
                    succeeded: true,
                    status: "RustAutoFetchDone",
                    output: String::new(),
                },
            ))
            .unwrap();
        let completed = controller.tick(
            &ctx,
            true,
            10,
            std::iter::empty(),
            Some(git.clone()),
            &scheduler,
        );
        assert_eq!(completed.len(), 1);
        assert_eq!(&completed[0].path, path);
        assert!(completed[0].succeeded);
        assert!(controller.due.is_empty());
    }
}
