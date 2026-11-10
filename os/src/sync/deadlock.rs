//! Deadlock detection and resource accounting.

use super::UPSafeCell;
use crate::task::{current_task, TaskControlBlock};
use alloc::{sync::Arc, vec, vec::Vec};

/// Resource allocation and pending requests for one resource family.
#[derive(Default)]
pub struct DeadlockDetector {
    /// Whether unsafe requests should be rejected.
    pub enabled: bool,
    /// Currently available units, indexed by resource ID.
    available: Vec<usize>,
    /// Units held by each thread.
    allocation: Vec<Vec<usize>>,
    /// One pending resource unit per thread.
    need: Vec<Option<usize>>,
}

impl DeadlockDetector {
    /// Determine whether every recorded thread can finish in some order.
    pub fn is_safe(&self) -> bool {
        let mut work = self.available.clone();
        let mut finished = vec![false; self.need.len()];
        let mut remaining = self.need.len();
        while remaining > 0 {
            let mut progressed = false;
            for (tid, done) in finished.iter_mut().enumerate() {
                if *done {
                    continue;
                }
                if let Some(id) = self.need[tid] {
                    if work[id] == 0 {
                        continue;
                    }
                }
                for (available, held) in work.iter_mut().zip(&self.allocation[tid]) {
                    *available += *held;
                }
                *done = true;
                remaining -= 1;
                progressed = true;
            }
            if !progressed {
                return false;
            }
        }
        true
    }

    fn ensure_thread(&mut self, tid: usize) {
        self.need.resize(self.need.len().max(tid + 1), None);
        self.allocation
            .resize_with(self.need.len(), || vec![0; self.available.len()]);
    }

    /// Remove accounting associated with a terminated thread.
    pub fn remove_thread(&mut self, tid: usize) {
        if tid < self.need.len() {
            self.need[tid] = None;
            self.allocation[tid].fill(0);
        }
    }
}

/// A synchronization resource registered in a detector.
pub struct Resource {
    id: usize,
    detector: Arc<UPSafeCell<DeadlockDetector>>,
}

impl Resource {
    /// Register a resource type with an initial number of units.
    pub fn new(detector: Arc<UPSafeCell<DeadlockDetector>>, count: usize) -> Self {
        let mut state = detector.exclusive_access();
        let id = state.available.len();
        state.available.push(count);
        for row in &mut state.allocation {
            row.push(0);
        }
        drop(state);
        Self { id, detector }
    }

    /// Record that the current thread is waiting for this resource.
    pub fn wait(&self) -> usize {
        let tid = current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid;
        let mut state = self.detector.exclusive_access();
        state.ensure_thread(tid);
        state.need[tid] = Some(self.id);
        tid
    }

    /// Record and validate a resource request.
    pub fn request(&self) -> bool {
        let tid = self.wait();
        let mut state = self.detector.exclusive_access();
        if state.enabled && !state.is_safe() {
            state.need[tid] = None;
            return false;
        }
        true
    }

    /// Assign one unit to a specific thread.
    pub fn acquire(&self, task: &TaskControlBlock) {
        let tid = task.inner_exclusive_access().res.as_ref().unwrap().tid;
        let mut state = self.detector.exclusive_access();
        state.ensure_thread(tid);
        state.available[self.id] -= 1;
        state.allocation[tid][self.id] += 1;
        state.need[tid] = None;
    }

    /// Return one unit from the current thread.
    pub fn release(&self) {
        let tid = current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid;
        let mut state = self.detector.exclusive_access();
        state.ensure_thread(tid);
        state.allocation[tid][self.id] = state.allocation[tid][self.id].saturating_sub(1);
        state.available[self.id] += 1;
    }
}
