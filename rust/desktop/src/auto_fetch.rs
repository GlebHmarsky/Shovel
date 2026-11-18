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
    active: HashMap<u64, (PathBuf, Cancellation)>,
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
        for (_, cancel) in self.active.values() {
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
        let mut candidates = Vec::new();
        let mut wake = None;
        for snapshot in snapshots {
            if snapshot.remotes.is_empty() || !snapshot.operations.is_empty() {
                continue;
            }
            let due = self
                .due
                .get(&snapshot.worktree)
                .copied()
                .unwrap_or_else(|| {
                    let initial = now + interval;
                    self.due.insert(snapshot.worktree.clone(), initial);
                    initial
                });
            if due > now {
                wake = Some(wake.map_or(due, |old: Instant| old.min(due)));
            } else if !self
                .active
                .values()
                .any(|(path, _)| path == &snapshot.worktree)
            {
                candidates.push((due, snapshot));
            }
        }
        if let Some(next) = wake {
            ctx.request_repaint_after(next - now);
        }
        candidates.sort_by_key(|(due, _)| *due);
        for (_, snapshot) in candidates
            .into_iter()
            .take(2usize.saturating_sub(self.active.len()))
        {
            self.due.insert(snapshot.worktree.clone(), now + interval);
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
            let expected = snapshot.identity.clone();
            let (_, accepted) = scheduler.submit(
                snapshot.identity.fingerprint.clone(),
                shovel_core::case::path_key(&snapshot.common_directory),
                format!("automatic-fetch:{}", path.display()),
                Lane::Fetch,
                move || {
                    let result = (|| {
                        if shovel_core::identity::read(&path)? != expected {
                            return Err(shovel_core::network::Error::Code("ActionIdentityChanged"));
                        }
                        let context = g.prepare_network(&path, &token)?;
                        if context.repository.identity != expected {
                            return Err(shovel_core::network::Error::Code("ActionIdentityChanged"));
                        }
                        g.automatic_fetch(&context, &token)
                    })();
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
                self.active.insert(id, (snapshot.worktree.clone(), cancel));
            }
        }
        completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduled_fetch_rejects_a_replaced_worktree_and_fetches_after_reopening() {
        use crate::history_view::tests::Fixture;
        let fixture = Fixture::new();
        let remote = fixture._temp.path().join("source.git");
        fixture.command(&["clone", "--bare", ".", remote.to_str().unwrap()], &[]);
        fixture.command(&["remote", "add", "origin", remote.to_str().unwrap()], &[]);
        let git = Arc::new(fixture.git.clone());
        let observed = git
            .inspect(&fixture.root, &Cancellation::default())
            .unwrap();
        let original = fixture.root.join(".git");
        let moved = fixture._temp.path().join("original.git");
        assert!(
            original
                .canonicalize()
                .unwrap()
                .starts_with(fixture._temp.path().canonicalize().unwrap())
        );
        assert_eq!(moved.parent().unwrap(), fixture._temp.path());
        std::fs::rename(original, moved).unwrap();
        fixture.command(&["init", "--initial-branch=main"], &[]);
        fixture.command(&["remote", "add", "origin", remote.to_str().unwrap()], &[]);
        std::fs::write(fixture.root.join("local"), "replacement work").unwrap();
        let before = fixture.command(&["status", "--porcelain=v1"], &[]);
        let reopened = git
            .inspect(&fixture.root, &Cancellation::default())
            .unwrap();
        assert_ne!(observed.identity, reopened.identity);
        let mut controller = Controller::default();
        let scheduler = Scheduler::default();
        let ctx = egui::Context::default();
        let mut fetch = |snapshot: &Snapshot| {
            controller.due_now(snapshot.worktree.clone());
            let deadline = Instant::now() + Duration::from_secs(12);
            loop {
                let mut results = controller.tick(
                    &ctx,
                    true,
                    60,
                    std::iter::once(snapshot),
                    Some(git.clone()),
                    &scheduler,
                );
                if let Some(result) = results.pop() {
                    break result;
                }
                assert!(Instant::now() < deadline, "automatic fetch did not finish");
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        let rejected = fetch(&observed);
        assert!(!rejected.succeeded);
        assert!(
            rejected
                .output
                .contains(&crate::strings::text("ActionIdentityChanged"))
        );
        assert!(
            fixture
                .command(&["for-each-ref", "refs/remotes"], &[])
                .is_empty()
        );
        assert!(!fixture.root.join(".git/FETCH_HEAD").exists());
        let accepted = fetch(&reopened);
        assert!(accepted.succeeded, "{}", accepted.output);
        assert_eq!(
            fixture.command(&["rev-parse", "refs/remotes/origin/main"], &[]),
            observed.head.unwrap()
        );
        assert_eq!(fixture.command(&["status", "--porcelain=v1"], &[]), before);
        assert_eq!(
            std::fs::read(fixture.root.join("local")).unwrap(),
            b"replacement work"
        );
    }

    #[test]
    fn older_due_repositories_take_the_two_slots_and_active_sequences_are_skipped() {
        use crate::history_view::tests::Fixture;
        let fixture = Fixture::new();
        fixture.command(&["remote", "add", "origin", "."], &[]);
        let git = Arc::new(fixture.git.clone());
        let base = git
            .inspect(&fixture.root, &Cancellation::default())
            .unwrap();
        let mut snapshots = Vec::new();
        for index in 0..5 {
            let root = fixture._temp.path().join(format!("other-{index}"));
            fixture.command(&["clone", "--quiet", ".", root.to_str().unwrap()], &[]);
            snapshots.push(git.inspect(&root, &Cancellation::default()).unwrap());
        }
        snapshots[3].operations.push("merge".into());
        snapshots[4].remotes.clear();
        let now = Instant::now();
        let mut controller = Controller::default();
        for (index, snapshot) in snapshots.iter().enumerate() {
            controller.due.insert(
                snapshot.worktree.clone(),
                now - Duration::from_secs(index as u64 + 1),
            );
        }
        let scheduler = Scheduler::default();
        let ctx = egui::Context::default();
        let tick = |c: &mut Controller| {
            c.tick(
                &ctx,
                true,
                60,
                snapshots.iter().chain(std::iter::once(&base)),
                Some(git.clone()),
                &scheduler,
            )
        };
        assert!(tick(&mut controller).is_empty());
        assert_eq!(controller.active.len(), 2);
        assert!(
            controller
                .active
                .values()
                .any(|(p, _)| p == &snapshots[2].worktree)
        );
        assert!(
            controller
                .active
                .values()
                .any(|(p, _)| p == &snapshots[1].worktree)
        );
        assert!(controller.due[&base.worktree] > now);
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut results = Vec::new();
        while results.len() != 3 {
            results.extend(tick(&mut controller));
            assert!(controller.active.len() <= 2);
            assert!(
                Instant::now() < deadline,
                "expected the three eligible fetches"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(results.iter().all(|r| r.succeeded));
        assert!(controller.drained());
        assert!(
            results
                .iter()
                .all(|r| snapshots[..3].iter().any(|s| s.worktree == r.path))
        );
    }

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
