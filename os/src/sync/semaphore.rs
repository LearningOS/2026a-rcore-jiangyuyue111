//! Semaphore

use crate::sync::{Resource, UPSafeCell};
use crate::task::{block_current_and_run_next, current_task, wakeup_task, TaskControlBlock};
use alloc::{collections::VecDeque, sync::Arc};

/// semaphore structure
pub struct Semaphore {
    /// semaphore inner
    pub inner: UPSafeCell<SemaphoreInner>,
    resource: Resource,
}

pub struct SemaphoreInner {
    pub count: isize,
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl Semaphore {
    /// Create a new semaphore
    pub fn new(res_count: usize, resource: Resource) -> Self {
        trace!("kernel: Semaphore::new");
        Self {
            resource,
            inner: unsafe {
                UPSafeCell::new(SemaphoreInner {
                    count: res_count as isize,
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }

    /// up operation of semaphore
    pub fn up(&self) {
        trace!("kernel: Semaphore::up");
        self.resource.release();
        let mut inner = self.inner.exclusive_access();
        inner.count += 1;
        if inner.count <= 0 {
            let task = inner.wait_queue.pop_front().unwrap();
            self.resource.acquire(&task);
            drop(inner);
            wakeup_task(task);
        }
    }

    /// down operation of semaphore
    pub fn down(&self) -> bool {
        trace!("kernel: Semaphore::down");
        if !self.resource.request() {
            return false;
        }
        let mut inner = self.inner.exclusive_access();
        inner.count -= 1;
        let task = current_task().unwrap();
        if inner.count >= 0 {
            self.resource.acquire(&task);
        } else {
            inner.wait_queue.push_back(task);
            drop(inner);
            block_current_and_run_next();
        }
        true
    }
}
