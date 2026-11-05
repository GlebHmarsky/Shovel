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
    run: Box<dyn FnOnce() -> Outcome + Send>,
    discard: Box<dyn FnOnce(Outcome) + Send>,
}
struct State {
    next: u64,
    waiting: VecDeque<Work>,
    running: BTreeMap<u64, Record>,
    completed: VecDeque<Record>,
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
                run: Box::new(run),
                discard: Box::new(discard),
            });
        }
        self.dispatch();
        (id, true)
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
            state.completed.push_back(work.record.clone());
            work
        };
        (work.discard)(Outcome::Removed);
        self.dispatch();
        true
    }
    fn dispatch(&self) {
        let ready = {
            let mut state = self.state.lock().unwrap();
            let mut ready = Vec::new();
            let mut index = 0;
            while index < state.waiting.len() {
                let candidate = &state.waiting[index].record;
                let blocked = state
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
        for work in ready {
            let scheduler = self.clone();
            thread::spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work.run))
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
                        state.completed.push_back(work.record.clone());
                        stopped.push(work);
                    } else {
                        index += 1;
                    }
                }
            }
            state.completed.push_back(record);
            while state.completed.len() > 128 {
                state.completed.pop_front();
            }
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
