//! Read lifetime tracking is independent of views and result generations.
use shovel_core::process::Cancellation;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct State {
    next: u64,
    active: BTreeMap<u64, Cancellation>,
}
#[derive(Clone, Default)]
pub(super) struct Workers(Arc<Mutex<State>>);
pub(super) struct Lease {
    workers: Workers,
    id: u64,
}
impl Workers {
    pub fn start(&self, cancel: Cancellation) -> Lease {
        let mut state = self.0.lock().unwrap();
        state.next += 1;
        let id = state.next;
        state.active.insert(id, cancel);
        Lease {
            workers: self.clone(),
            id,
        }
    }
    pub fn cancel_and_drained(&self) -> bool {
        let state = self.0.lock().unwrap();
        for cancel in state.active.values() {
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
