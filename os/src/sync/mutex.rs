//! Mutex (spin-like and blocking(sleep))

use super::{Resource, UPSafeCell};
use crate::task::TaskControlBlock;
use crate::task::{block_current_and_run_next, suspend_current_and_run_next};
use crate::task::{current_task, wakeup_task};
use alloc::{collections::VecDeque, sync::Arc};

/// Mutex trait
pub trait Mutex: Sync + Send {
    /// Resource accounting associated with this mutex.
    fn resource(&self) -> &Resource;
    /// Lock the mutex
    fn lock(&self);
    /// Unlock the mutex
    fn unlock(&self);
}

/// Spinlock Mutex struct
pub struct MutexSpin {
    locked: UPSafeCell<bool>,
    resource: Resource,
}

impl MutexSpin {
    /// Create a new spinlock mutex
    pub fn new(resource: Resource) -> Self {
        Self {
            locked: unsafe { UPSafeCell::new(false) },
            resource,
        }
    }
}

impl Mutex for MutexSpin {
    fn resource(&self) -> &Resource {
        &self.resource
    }

    /// Lock the spinlock mutex
    fn lock(&self) {
        trace!("kernel: MutexSpin::lock");
        self.resource.wait();
        loop {
            let mut locked = self.locked.exclusive_access();
            if !*locked {
                *locked = true;
                self.resource.acquire(&current_task().unwrap());
                return;
            }
            drop(locked);
            suspend_current_and_run_next();
        }
    }

    fn unlock(&self) {
        trace!("kernel: MutexSpin::unlock");
        self.resource.release();
        *self.locked.exclusive_access() = false;
    }
}

/// Blocking Mutex struct
pub struct MutexBlocking {
    inner: UPSafeCell<MutexBlockingInner>,
    resource: Resource,
}

pub struct MutexBlockingInner {
    locked: bool,
    wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl MutexBlocking {
    /// Create a new blocking mutex
    pub fn new(resource: Resource) -> Self {
        trace!("kernel: MutexBlocking::new");
        Self {
            resource,
            inner: unsafe {
                UPSafeCell::new(MutexBlockingInner {
                    locked: false,
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }
}

impl Mutex for MutexBlocking {
    fn resource(&self) -> &Resource {
        &self.resource
    }

    /// lock the blocking mutex
    fn lock(&self) {
        trace!("kernel: MutexBlocking::lock");
        self.resource.wait();
        let mut inner = self.inner.exclusive_access();
        let task = current_task().unwrap();
        if !inner.locked {
            inner.locked = true;
            self.resource.acquire(&task);
        } else {
            inner.wait_queue.push_back(task);
            drop(inner);
            block_current_and_run_next();
        }
    }

    /// unlock the blocking mutex
    fn unlock(&self) {
        trace!("kernel: MutexBlocking::unlock");
        let mut inner = self.inner.exclusive_access();
        assert!(inner.locked);
        self.resource.release();
        if let Some(task) = inner.wait_queue.pop_front() {
            self.resource.acquire(&task);
            drop(inner);
            wakeup_task(task);
        } else {
            inner.locked = false;
        }
    }
}
