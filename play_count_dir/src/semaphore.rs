use crossbeam::atomic::AtomicCell;
use parking_lot::{
    deadlock,
    lock_api::{GuardNoSend, RawRwLock},
    Condvar, Mutex, MutexGuard, RawMutex as UnitMutex, RawRwLock as UnitRwLock, RwLock,
    RwLockWriteGuard,
};
use parking_lot_core::{park, ParkResult, UnparkToken, DEFAULT_PARK_TOKEN};
use std::{
    cell::Cell,
    char::MAX,
    fmt::Debug,
    marker::PhantomData,
    ops::Deref,
    ops::{Add, Sub},
    sync::atomic::AtomicU8,
    sync::atomic::{AtomicPtr, Ordering},
    sync::Arc,
    time::Instant,
};

use crate::{
    ac_lock::{AcLock, Counter},
    cond_lock::{LockState, RawLock},
    num::UnsignedNum,
};

pub struct ReadSemaphore<'a, N: UnsignedNum, T: Debug> {
    raw_lock: Arc<AcLock<N>>,
    data: &'a T,
}

impl<'a, N: UnsignedNum, T: Debug> ReadSemaphore<'a, N, T> {
    pub fn new(max_readers: N, data: &'a T) -> Self {
        Self {
            raw_lock: Arc::new(AcLock::new(max_readers)),
            data,
        }
    }

    pub fn read(&self, on_release_notify: OnReleaseNotify) {
        let raw_guard = self.raw_lock.wait_for_access(on_release_notify);
        Guard::new(raw_guard, Shared::from(self.data))
    }
}

pub struct CondLock<L: RawLock, T> {
    raw_lock: Arc<L>,
    data: T,
}

impl<L: RawLock, T> CondLock<L, T> {
    pub fn new(raw_lock: L, data: T) -> Self {
        Self {
            raw_lock: Arc::new(raw_lock),
            data,
        }
    }

    pub fn wait_for_access(&self, on_release_notify: OnReleaseNotify) -> RawGuard<L> {
        self.raw_lock.wait_for_access(on_release_notify)
    }
}
