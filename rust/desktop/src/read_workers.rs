//! Read lifetime tracking is independent of views and result generations.
use shovel_core::process::Cancellation;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct State {
    next: u64,
    stopping: bool,
    active: BTreeMap<u64, Option<Cancellation>>,
}
#[derive(Clone, Default)]
pub(super) struct Workers(Arc<Mutex<State>>);
pub(super) struct Lease {
    workers: Workers,
    id: u64,
}
impl Workers {
    pub fn start(&self, cancel: Cancellation) -> Lease {
        self.register(Some(cancel))
    }
    // Draft restoration supplies the revision needed for safe subsequent writes.
    // Wait for it without cancelling or discarding the recovery information.
    pub fn start_required(&self) -> Lease {
        self.register(None)
    }
    fn register(&self, cancel: Option<Cancellation>) -> Lease {
        let mut state = self.0.lock().unwrap();
        state.next += 1;
        let id = state.next;
        state.active.insert(id, cancel);
        Lease {
            workers: self.clone(),
            id,
        }
    }
    pub fn stopping(&self) -> bool {
        self.0.lock().unwrap().stopping
    }
    pub fn idle(&self) -> bool {
        self.0.lock().unwrap().active.is_empty()
    }
    pub fn resume(&self) {
        self.0.lock().unwrap().stopping = false;
    }
    pub fn cancel_and_drained(&self) -> bool {
        let mut state = self.0.lock().unwrap();
        state.stopping = true;
        for cancel in state.active.values().flatten() {
            cancel.cancel();
        }
        state.active.is_empty()
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.workers.0.lock().unwrap().active.remove(&self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_does_not_count_as_completion_and_old_leases_remain_owned() {
        let workers = Workers::default();
        let first = Cancellation::default();
        let second = Cancellation::default();
        let old = workers.start(first.clone());
        let new = workers.start(second.clone());
        assert!(!workers.cancel_and_drained());
        assert!(first.is_cancelled() && second.is_cancelled());
        drop(new);
        assert!(
            !workers.cancel_and_drained(),
            "superseding a read cannot forget the old worker"
        );
        drop(old);
        assert!(workers.cancel_and_drained());
    }
}
