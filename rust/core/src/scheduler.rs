//! Application-owned accepted work. Views hold IDs, never command cancellation.
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
    thread,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    Read,
    Fetch,
    WorkingCopy,
    Exclusive,
}
fn conflicts(a: Lane, b: Lane) -> bool {
    if a == Lane::Read || b == Lane::Read {
        false
    } else if a == Lane::Exclusive || b == Lane::Exclusive {
        true
    } else {
        a == b
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    Failed,
    Conflict,
    Removed,
    StoppedAfterFailure,
}
#[derive(Clone, Debug)]
pub struct Record {
    pub id: u64,
    pub repository: String,
    pub common: String,
    pub key: String,
    pub lane: Lane,
    pub running: bool,
    pub outcome: Option<Outcome>,
}
struct Work {
    record: Record,
    run: Option<Box<dyn FnOnce() -> Outcome + Send>>,
    discard: Box<dyn FnOnce(Outcome) + Send>,
}
struct State {
    next: u64,
    waiting: VecDeque<Work>,
    running: BTreeMap<u64, Record>,
    completed: VecDeque<Record>,
}
impl State {
    fn retain_completed(&mut self, record: Record) {
        self.completed.push_back(record);
        while self.completed.len() > 128 {
            self.completed.pop_front();
        }
    }
}
#[derive(Clone)]
pub struct Scheduler {
    state: Arc<Mutex<State>>,
}
impl Default for Scheduler {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                next: 1,
                waiting: VecDeque::new(),
                running: BTreeMap::new(),
                completed: VecDeque::new(),
            })),
        }
    }
}
impl Scheduler {
    /// Duplicate action keys share an accepted request. Conflicting mutations
    /// preserve submission order even across linked worktrees; reads bypass it.
    pub fn submit(
        &self,
        repository: String,
        common: String,
        key: String,
        lane: Lane,
        run: impl FnOnce() -> Outcome + Send + 'static,
        discard: impl FnOnce(Outcome) + Send + 'static,
    ) -> (u64, bool) {
        self.enqueue(
            repository,
            common,
            key,
            lane,
            Some(Box::new(run)),
            Box::new(discard),
        )
    }

    /// Reserve a real waiting request before repository initialization resolves.
    /// It participates in exit guards, FIFO, deduplication and individual removal,
    /// but no worker may start until the owning controller supplies verified state.
    pub fn reserve(
        &self,
        repository: String,
        common: String,
        key: String,
        lane: Lane,
        discard: impl FnOnce(Outcome) + Send + 'static,
    ) -> (u64, bool) {
        self.enqueue(repository, common, key, lane, None, Box::new(discard))
    }

    fn enqueue(
        &self,
        repository: String,
        common: String,
        key: String,
        lane: Lane,
        run: Option<Box<dyn FnOnce() -> Outcome + Send>>,
        discard: Box<dyn FnOnce(Outcome) + Send>,
    ) -> (u64, bool) {
        let id;
        {
            let mut state = self.state.lock().unwrap();
            if let Some(record) = state
                .running
                .values()
                .chain(state.waiting.iter().map(|w| &w.record))
                .find(|r| r.repository == repository && r.key == key)
            {
                return (record.id, false);
            }
            id = state.next;
            state.next += 1;
            state.waiting.push_back(Work {
                record: Record {
                    id,
                    repository,
                    common,
                    key,
                    lane,
                    running: false,
                    outcome: None,
                },
                run,
                discard,
            });
        }
        self.dispatch();
        (id, true)
    }
    /// Bind the whole pending owner before activating any request. Failure of
    /// the first action can then stop all its remaining requests consistently.
    pub fn bind_repository(&self, pending: &str, repository: &str, common: &str) {
        let mut state = self.state.lock().unwrap();
        for work in &mut state.waiting {
            if work.record.repository == pending && work.run.is_none() {
                work.record.repository = repository.to_owned();
                work.record.common = common.to_owned();
            }
        }
    }

    pub fn activate(&self, id: u64, run: impl FnOnce() -> Outcome + Send + 'static) -> bool {
        {
            let mut state = self.state.lock().unwrap();
            let Some(work) = state
                .waiting
                .iter_mut()
                .find(|w| w.record.id == id && w.run.is_none())
            else {
                return false;
            };
            work.run = Some(Box::new(run));
        }
        self.dispatch();
        true
    }

    pub fn fail_reserved(&self, id: u64) -> bool {
        let discarded = {
            let mut state = self.state.lock().unwrap();
            let Some(repository) = state
                .waiting
                .iter()
                .find(|w| w.record.id == id && w.run.is_none())
                .map(|w| w.record.repository.clone())
            else {
                return false;
            };
            let mut discarded = Vec::new();
            let mut index = 0;
            while index < state.waiting.len() {
                if state.waiting[index].record.repository == repository {
                    let mut work = state.waiting.remove(index).unwrap();
                    work.record.outcome = Some(if work.record.id == id {
                        Outcome::Failed
                    } else {
                        Outcome::StoppedAfterFailure
                    });
                    state.retain_completed(work.record.clone());
                    discarded.push(work);
                } else {
                    index += 1;
                }
            }
            discarded
        };
        for work in discarded {
            (work.discard)(work.record.outcome.unwrap());
        }
        self.dispatch();
        true
    }

    pub fn records(&self) -> Vec<Record> {
        let state = self.state.lock().unwrap();
        state
            .completed
            .iter()
            .chain(state.running.values())
            .chain(state.waiting.iter().map(|w| &w.record))
            .cloned()
            .collect()
    }
    pub fn has_accepted_work(&self) -> bool {
        let state = self.state.lock().unwrap();
        !state.waiting.is_empty() || !state.running.is_empty()
    }
    pub fn remove_waiting(&self, id: u64) -> bool {
        let work = {
            let mut state = self.state.lock().unwrap();
            let Some(index) = state.waiting.iter().position(|w| w.record.id == id) else {
                return false;
            };
            let mut work = state.waiting.remove(index).unwrap();
            work.record.outcome = Some(Outcome::Removed);
            state.retain_completed(work.record.clone());
            work
        };
        (work.discard)(Outcome::Removed);
        self.dispatch();
        true
    }
    /// A queued options dialog already owns its place. Its submitted command
    /// must inherit that place before the dialog releases the conflicting lane.
    /// Independent repositories retain their ordering; running work is untouched.
    pub fn inherit_dialog_order(&self, dialog: u64, command: u64) -> bool {
        let mut state = self.state.lock().unwrap();
        let Some(owner) = state.running.get(&dialog) else {
            return false;
        };
        if !owner.key.starts_with("dialog:") {
            return false;
        }
        let Some(index) = state.waiting.iter().position(|w| {
            w.record.id == command
                && w.record.repository == owner.repository
                && w.record.common == owner.common
                && w.record.lane == owner.lane
        }) else {
            return false;
        };
        let work = state.waiting.remove(index).unwrap();
        let first = state
            .waiting
            .iter()
            .position(|other| {
                other.record.common == work.record.common
                    && conflicts(other.record.lane, work.record.lane)
            })
            .unwrap_or(state.waiting.len());
        state.waiting.insert(first, work);
        true
    }

    fn dispatch(&self) {
        let ready = {
            let mut state = self.state.lock().unwrap();
            let mut ready = Vec::new();
            let mut index = 0;
            while index < state.waiting.len() {
                let candidate = &state.waiting[index].record;
                let blocked = state.waiting[index].run.is_none()
                    || state
                        .running
                        .values()
                        .any(|r| r.common == candidate.common && conflicts(r.lane, candidate.lane))
                    || state.waiting.iter().take(index).any(|w| {
                        w.record.common == candidate.common
                            && conflicts(w.record.lane, candidate.lane)
                    });
                if blocked {
                    index += 1;
                } else {
                    let mut work = state.waiting.remove(index).unwrap();
                    work.record.running = true;
                    state.running.insert(work.record.id, work.record.clone());
                    ready.push(work);
                }
            }
            ready
        };
        for mut work in ready {
            let scheduler = self.clone();
            thread::spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                    work.run.take().expect("ready request"),
                ))
                .unwrap_or(Outcome::Failed);
                scheduler.finish(work.record.id, outcome);
            });
        }
    }
    fn finish(&self, id: u64, outcome: Outcome) {
        let stopped = {
            let mut state = self.state.lock().unwrap();
            let Some(mut record) = state.running.remove(&id) else {
                return;
            };
            record.running = false;
            record.outcome = Some(outcome);
            let mut stopped = Vec::new();
            if matches!(outcome, Outcome::Failed | Outcome::Conflict) {
                let mut index = 0;
                while index < state.waiting.len() {
                    if state.waiting[index].record.repository == record.repository {
                        let mut work = state.waiting.remove(index).unwrap();
                        work.record.outcome = Some(Outcome::StoppedAfterFailure);
                        state.retain_completed(work.record.clone());
                        stopped.push(work);
                    } else {
                        index += 1;
                    }
                }
            }
            state.retain_completed(record);
            stopped
        };
        for work in stopped {
            (work.discard)(Outcome::StoppedAfterFailure);
        }
        self.dispatch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;
    #[test]
    fn dialog_submission_inherits_order_without_reordering_other_owners() {
        let scheduler = Scheduler::default();
        let (release, held) = mpsc::channel();
        let (entered, started) = mpsc::channel();
        let (order, received) = mpsc::channel();
        let (dialog, _) = scheduler.submit(
            "repo".into(),
            "common".into(),
            "dialog:NewBranch".into(),
            Lane::Exclusive,
            move || {
                entered.send(()).unwrap();
                held.recv_timeout(Duration::from_secs(5)).unwrap();
                Outcome::Succeeded
            },
            |_| panic!("dialog discarded"),
        );
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let later = order.clone();
        scheduler.submit(
            "repo".into(),
            "common".into(),
            "later".into(),
            Lane::Exclusive,
            move || {
                later.send("later").unwrap();
                Outcome::Succeeded
            },
            |_| {},
        );
        let (other, _) = scheduler.reserve(
            "other".into(),
            "common".into(),
            "other".into(),
            Lane::Exclusive,
            |_| {},
        );
        assert!(!scheduler.inherit_dialog_order(dialog, other));
        let (command, _) = scheduler.submit(
            "repo".into(),
            "common".into(),
            "create".into(),
            Lane::Exclusive,
            move || {
                order.send("create").unwrap();
                Outcome::Succeeded
            },
            |_| {},
        );
        assert!(scheduler.inherit_dialog_order(dialog, command));
        release.send(()).unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            "create"
        );
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            "later"
        );
        assert!(scheduler.remove_waiting(other));
    }

    #[test]
    fn reserved_initialization_coalesces_removes_and_orders_bound_linked_work() {
        let scheduler = Scheduler::default();
        let (a, accepted) = scheduler.reserve(
            "pending".into(),
            "pending".into(),
            "push".into(),
            Lane::Exclusive,
            |_| {},
        );
        assert!(accepted);
        let duplicate = scheduler.reserve(
            "pending".into(),
            "pending".into(),
            "push".into(),
            Lane::Exclusive,
            |_| panic!("duplicate"),
        );
        assert_eq!(duplicate, (a, false));
        let (b, _) = scheduler.reserve(
            "pending".into(),
            "pending".into(),
            "pull".into(),
            Lane::Exclusive,
            |_| {},
        );
        let (removed, _) = scheduler.reserve(
            "pending".into(),
            "pending".into(),
            "fetch".into(),
            Lane::Fetch,
            |_| {},
        );
        assert!(scheduler.remove_waiting(removed));
        assert!(!scheduler.activate(removed, || panic!("removed reservation started")));
        assert!(scheduler.has_accepted_work());
        assert!(scheduler.records().iter().all(|r| !r.running));
        scheduler.bind_repository("pending", "worktree-A", "common");
        let (sent, received) = mpsc::channel();
        let tx = sent.clone();
        // Activating a later request cannot overtake an earlier unresolved one.
        assert!(scheduler.activate(b, move || {
            tx.send("second").unwrap();
            Outcome::Succeeded
        }));
        assert!(received.recv_timeout(Duration::from_millis(40)).is_err());
        let tx = sent.clone();
        scheduler.submit(
            "worktree-B".into(),
            "common".into(),
            "linked".into(),
            Lane::Exclusive,
            move || {
                tx.send("linked").unwrap();
                Outcome::Succeeded
            },
            |_| {},
        );
        let tx = sent.clone();
        scheduler.submit(
            "other".into(),
            "other".into(),
            "independent".into(),
            Lane::Exclusive,
            move || {
                tx.send("independent").unwrap();
                Outcome::Succeeded
            },
            |_| {},
        );
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            "independent"
        );
        assert!(scheduler.activate(a, move || {
            sent.send("first").unwrap();
            Outcome::Succeeded
        }));
        for expected in ["first", "second", "linked"] {
            assert_eq!(
                received.recv_timeout(Duration::from_secs(5)).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn failed_initialization_stops_only_its_waiting_requests_without_running_them() {
        let scheduler = Scheduler::default();
        let (tx, rx) = mpsc::channel();
        let sent = tx.clone();
        let (first, _) = scheduler.reserve(
            "a".into(),
            "a".into(),
            "push".into(),
            Lane::Exclusive,
            move |outcome| {
                sent.send(outcome).unwrap();
            },
        );
        let (second, _) = scheduler.reserve(
            "a".into(),
            "a".into(),
            "pull".into(),
            Lane::Exclusive,
            move |outcome| {
                tx.send(outcome).unwrap();
            },
        );
        let (other, _) =
            scheduler.reserve("b".into(), "b".into(), "fetch".into(), Lane::Fetch, |_| {});
        assert!(scheduler.fail_reserved(first));
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            Outcome::Failed
        );
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            Outcome::StoppedAfterFailure
        );
        assert!(!scheduler.activate(second, || panic!("failed initialization ran")));
        assert!(
            scheduler
                .records()
                .iter()
                .any(|r| r.id == other && r.outcome.is_none())
        );
        assert!(scheduler.remove_waiting(other));
        assert!(!scheduler.has_accepted_work());
    }

    #[test]
    fn waiting_removals_bound_completed_history_without_evicting_running_work() {
        let scheduler = Scheduler::default();
        let (release, held) = mpsc::channel();
        let (started, running) = mpsc::channel();
        let (active, _) = scheduler.submit(
            "repo".into(),
            "common".into(),
            "held".into(),
            Lane::Exclusive,
            move || {
                started.send(()).unwrap();
                held.recv_timeout(Duration::from_secs(10)).unwrap();
                Outcome::Succeeded
            },
            |_| panic!("running work must never be discarded"),
        );
        running.recv_timeout(Duration::from_secs(5)).unwrap();
        let (discarded, received) = mpsc::channel();
        let mut latest = 0;
        for n in 0..160 {
            let discarded = discarded.clone();
            let (id, accepted) = scheduler.submit(
                "repo".into(),
                "common".into(),
                format!("waiting-{n}"),
                Lane::WorkingCopy,
                || panic!("removed request must not start"),
                move |outcome| {
                    discarded.send(outcome).unwrap();
                },
            );
            assert!(accepted);
            assert!(scheduler.remove_waiting(id));
            assert_eq!(
                received.recv_timeout(Duration::from_secs(1)).unwrap(),
                Outcome::Removed
            );
            latest = id;
            let records = scheduler.records();
            assert!(records.iter().filter(|r| r.outcome.is_some()).count() <= 128);
            assert!(records.iter().any(|r| r.id == active && r.running));
            assert!(scheduler.has_accepted_work());
        }
        assert!(!scheduler.remove_waiting(active));
        let records = scheduler.records();
        assert_eq!(records.len(), 129);
        assert!(
            records
                .iter()
                .any(|r| r.id == latest && r.outcome == Some(Outcome::Removed))
        );
        assert!(!records.iter().any(|r| r.id == active + 1));
        release.send(()).unwrap();
        let start = std::time::Instant::now();
        while scheduler.has_accepted_work() {
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(5));
        }
        let records = scheduler.records();
        assert_eq!(records.len(), 128);
        assert!(
            records
                .iter()
                .any(|r| r.id == active && r.outcome == Some(Outcome::Succeeded))
        );
    }

    #[test]
    fn fetch_overlaps_working_copy_but_conflicting_jobs_wait_and_coalesce() {
        let scheduler = Scheduler::default();
        let (started_tx, started) = mpsc::channel();
        let (release_tx, release) = mpsc::channel();
        let (id, _) = scheduler.submit(
            "a".into(),
            "common".into(),
            "stage".into(),
            Lane::WorkingCopy,
            move || {
                started_tx.send(()).unwrap();
                release.recv().unwrap();
                Outcome::Succeeded
            },
            |_| {},
        );
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            scheduler.submit(
                "a".into(),
                "common".into(),
                "stage".into(),
                Lane::WorkingCopy,
                || panic!("duplicate ran"),
                |_| {}
            ),
            (id, false)
        );
        let (fetch_tx, fetch) = mpsc::channel();
        scheduler.submit(
            "a".into(),
            "common".into(),
            "fetch".into(),
            Lane::Fetch,
            move || {
                fetch_tx.send(()).unwrap();
                Outcome::Succeeded
            },
            |_| {},
        );
        fetch.recv_timeout(Duration::from_secs(2)).unwrap();
        let (next_tx, next) = mpsc::channel();
        scheduler.submit(
            "linked".into(),
            "common".into(),
            "commit".into(),
            Lane::WorkingCopy,
            move || {
                next_tx.send(()).unwrap();
                Outcome::Succeeded
            },
            |_| {},
        );
        assert!(next.try_recv().is_err());
        assert!(!scheduler.remove_waiting(id));
        release_tx.send(()).unwrap();
        next.recv_timeout(Duration::from_secs(2)).unwrap();
    }
    #[test]
    fn remove_one_waiting_and_stop_own_queue_on_failure() {
        let scheduler = Scheduler::default();
        let (start_tx, start) = mpsc::channel();
        let (release_tx, release) = mpsc::channel();
        scheduler.submit(
            "a".into(),
            "common".into(),
            "held".into(),
            Lane::Exclusive,
            move || {
                start_tx.send(()).unwrap();
                release.recv().unwrap();
                Outcome::Failed
            },
            |_| {},
        );
        start.recv_timeout(Duration::from_secs(2)).unwrap();
        let (discard_tx, discard) = mpsc::channel();
        let removed = discard_tx.clone();
        let (waiting, _) = scheduler.submit(
            "a".into(),
            "common".into(),
            "remove".into(),
            Lane::WorkingCopy,
            || panic!("removed ran"),
            move |o| {
                removed.send(o).unwrap();
            },
        );
        scheduler.submit(
            "a".into(),
            "common".into(),
            "stop".into(),
            Lane::WorkingCopy,
            || panic!("stopped ran"),
            move |o| {
                discard_tx.send(o).unwrap();
            },
        );
        let (other_tx, other) = mpsc::channel();
        scheduler.submit(
            "other".into(),
            "other common".into(),
            "commit".into(),
            Lane::WorkingCopy,
            move || {
                other_tx.send(()).unwrap();
                Outcome::Succeeded
            },
            |_| {},
        );
        other.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(scheduler.remove_waiting(waiting));
        assert_eq!(
            discard.recv_timeout(Duration::from_secs(2)).unwrap(),
            Outcome::Removed
        );
        release_tx.send(()).unwrap();
        assert_eq!(
            discard.recv_timeout(Duration::from_secs(2)).unwrap(),
            Outcome::StoppedAfterFailure
        );
    }
}
