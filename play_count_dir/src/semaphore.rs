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
    conditional_lock::{
        AbstractReadGuard, AbstractWriteGuard, Lock, LockGuard, LockState, WaitResult,
    },
    counting_lock::{CountState, CountingLock},
    num::UnsignedNum,
};

#[derive(Debug, Clone)]
pub struct RwSemaphore<N: UnsignedNum, T> {
    read_counter: Arc<CountingLock<N>>,
    write_counter: Arc<CountingLock<N>>,
    resource: T,
}

impl<N: UnsignedNum, T> RwSemaphore<N, T> {
    pub fn new(max_reads: N, max_writes: N, resource: T) -> Self {
        Self {
            read_counter: Arc::new(CountingLock::new(max_reads)),
            write_counter: Arc::new(CountingLock::new(max_writes)),
            resource,
        }
    }
}

pub struct SemaphoreReadGuard<'a, N: UnsignedNum, T> {
    resource: &'a T,
    lock: Arc<CountingLock<N>>,
}

impl<'a, N: UnsignedNum, T> AbstractReadGuard<'a> for SemaphoreReadGuard<'a, N, T> {
    type Data = T;
}

pub struct SemaphoreWriteGuard<'a, N: UnsignedNum, T> {
    resource: &'a T,
    lock: Arc<CountingLock<N>>,
}

impl<'a, N: UnsignedNum, T> AbstractWriteGuard<'a> for SemaphoreWriteGuard<'a, N, T> {
    type Data = T;
}
