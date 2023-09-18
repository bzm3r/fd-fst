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
    count_lock::{ConstCountLock, Counter, CountingLock, RawCountingLock},
    num::UnsignedNum,
    predicate_lock::{PredicateLock, PredicateLockGuard, RawPredicateLock, WaitOutcome},
};

pub struct SemaphoreLock<C: RawCountingLock> {
    read_counter: C,
    write_counter: C,
}

#[derive(Debug, Clone)]
pub struct Semaphore<N: UnsignedNum, C: Counter<N>>(Arc<SemaphoreLock<N, C>>);

#[derive(Debug, Clone)]
pub struct ConstSemaphore<N: UnsignedNum>(Arc<SemaphoreLock<N>>);

impl<N: UnsignedNum> Semaphore<N> {
    pub fn new(max_capacity: N) -> Self {
        Self(Arc::new(CountingLock::new(max_capacity).into()))
    }
}

impl<N: UnsignedNum> Deref for Semaphore<N> {
    type Target = Arc<SemaphoreLock<N>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub trait SemaphoreReadGuard<N: UnsignedNum>: PredicateLockGuard {
    fn new(raw: Arc<RwLock<CountingLock<N>>>) -> Self;

    fn read_raw(&self) -> parking_lot::RwLockReadGuard<N>;
}

pub trait SemaphoreWriteGuard<N: UnsignedNum>: PredicateLockGuard {
    fn new(raw: Arc<RwLock<CountingLock<N>>>) -> Self;

    fn write_raw(&self) -> parking_lot::RwLockWriteGuard<N>;
}

impl<N: UnsignedNum, T: SemaphoreReadGuard<N>> PredicateLockGuard for T {
    type BinaryLock = SemaphoreLock<N>;

    fn new(raw: Arc<RwLock<Self::BinaryLock>>) -> Self {
        todo!()
    }
}

impl<N: UnsignedNum, T: SemaphoreWriteGuard<N>> PredicateLockGuard for T {
    type BinaryLock = SemaphoreLock<N>;

    fn new(raw: Arc<RwLock<Self::BinaryLock>>) -> Self {
        todo!()
    }
}

pub struct CountGuard<C> {}

impl<C: C> PredicateLockGuard for CountGuard<C> {}

impl<N: UnsignedNum, T> RawPredicateLock for CountingLock<N> {
    #[inline]
    fn lock(self: &mut Arc<Self>) -> Option<SemaphoreReadGuard<N>> {
        self.increment()
            .then(|| PredicateLockGuard::new(self.clone()))
    }

    #[inline]
    fn unlock(self: &mut Arc<Self>) {
        self.decrement();
    }

    fn has_parked(self: &Arc<Self>) -> bool {
        self.parked
    }

    fn mark_parked(self: &mut Arc<Self>) {
        self.parked = true;
    }

    fn mark_unparked(self: &mut Arc<Self>) {
        self.parked = false;
    }

    #[inline]
    fn is_locked(&self) -> bool {
        self.curr < self.limit
    }
}

impl<N: UnsignedNum> Semaphore<N> {
    fn create_access(&self) -> PredicateLockGuard<CountingLock<N>> {
        PredicateLockGuard::new(self.0.raw())
    }

    /// Wait on the semaphore until a token can be provided.
    #[inline]
    fn access(&self) -> PredicateLockGuard<CountingLock<N>> {
        let mut result: Option<WaitOutcome<CountingLock<N>>> = None;

        while !result.is_some_and(|result| result.timed_out()) {
            result.replace(self.0.wait_until_access(None));
        }

        match result.unwrap() {
            WaitOutcome::Success(access) => access,
            _ => panic!("unexpected loop break, even though access has not been acquired"),
        }
    }
}

pub struct RwSem<N: UnsignedNum, T> {
    read_sem: Semaphore<N>,
    write_sem: Semaphore<N>,
    protected: T,
}

impl<N: UnsignedNum, T> RwSem<N, T> {
    pub fn new(max_read: N, max_write: N) -> Self {
        todo!()
    }

    pub fn read(&self) -> Self {}
}
