/// Parking refers to suspending the thread while simultaneously enqueuing it on
/// a queue keyed by some address. Unparking refers to dequeuing a thread from a
/// queue keyed by some address and resuming it.
use std::{
    fmt::Debug,
    ops::Deref,
    sync::{atomic::Ordering, Arc},
    time::Instant,
};

use crossbeam::atomic::AtomicCell;
use parking_lot::{RawRwLock, RwLock, RwLockReadGuard, RwLockWriteGuard};
use parking_lot_core::{RequeueOp, UnparkResult, UnparkToken, DEFAULT_PARK_TOKEN};

use crate::{arc_locks::ArcRwLock, num_check::FiniteTest};

pub const UNPARK_NORMAL: UnparkToken = UnparkToken(0);

pub trait RawPredicateLock
where
    Self: Sized + Debug,
{
    /// Apply a lock operation to this binary lock, if possible. This action has
    /// to be thread safe, as multiple threads could be attempting it.
    fn lock(&mut self);

    /// Apply an unlock operation to this binary lock. /// This action has to be
    /// thread safe, as multiple threads could be attempting it.
    fn unlock(&mut self);

    /// Check if this condition is in such a state that it is locked.
    ///
    /// Lock state only makes sense if the thread also has parked threads.
    ///
    /// This action has to be thread safe, as multiple threads could be
    /// attempting it.
    #[inline]
    fn has_parked_then_is_locked(self: &Arc<Self>) -> Option<bool> {
        self.has_parked().then(|| !(self.is_locked()))
    }

    /// Check if this lock has been marked as 'parked'. This action has to be
    /// thread safe, as multiple threads could be attempting it.
    fn has_parked(self: &Arc<Self>) -> bool;

    /// Mark lock as having thread that are parked due to its condition. This
    /// action has to be thread safe, as multiple threads could be attempting
    /// it.
    fn mark_parked(self: &mut Arc<Self>);

    /// Mark lock as having thread that are parked due to its condition. This
    /// action has to be thread safe, as multiple threads could be attempting
    /// it.
    fn mark_unparked(self: &mut Arc<Self>);

    /// Check if this binary lock is currently locked
    fn is_locked(&self) -> bool;
}

// /// Access to the lock is released when it is dropped. #[derive(Debug)] pub
// struct ConditionalLockGuard<C: RawConditionalLock>(Arc<C>);

// impl<C: RawConditionalLock> ConditionalLockGuard<C> { pub fn new(c: Arc<C>)
//     -> Self { Self(c.clone()) } }

// impl<C: RawConditionalLock> Drop for ConditionalLockGuard<C> { fn drop(&mut
//     self) { self.0.unlock(); } }

pub trait PredicateLockGuard {
    type BinaryLock: RawPredicateLock;
    /// Create a new guard around the provided raw binary lock
    fn new(raw: Arc<RwLock<Self::BinaryLock>>) -> Self;
}

pub trait PredicateLockReadGuard<'a, T>: PredicateLockGuard {
    fn read(&self) -> &'a T;
}

pub trait PredicateLockWriteGuard<'a, T>: PredicateLockGuard {
    fn write(&self) -> &'a mut T;
}

impl<Guard: PredicateLockGuard> Drop for Guard {
    fn drop(&mut self) {
        self.write_raw().unlock()
    }
}

/// Returns the result of waiting on a [`CondLock`].
#[derive(Debug)]
pub enum WaitOutcome<Guard: PredicateLockGuard> {
    Success(Guard),
    Failure,
    Requeued,
    TimedOut,
}

impl<Guard: PredicateLockGuard> WaitOutcome<Guard> {
    /// Check if the result indicates a timeout.
    pub fn timed_out(&self) -> bool {
        match self {
            Self::TimedOut => true,
            _ => false,
        }
    }

    /// Check if result indicates successful gain of access.
    pub fn acquired_access(&self) -> bool {
        match self {
            Self::Success(_) => true,
            _ => false,
        }
    }
}

pub trait Addressed
where
    Self: Sized,
{
    /// The address used to look up the queue of threads parked due this being
    /// locked.
    #[inline]
    fn addr(&self) -> usize {
        self as *const _ as usize
    }
}

impl<RawLock: RawPredicateLock, Lock: PredicateLock<RawLock = RawLock>> Addressed for Lock {}

pub trait PredicateLock: Debug {
    type RawLock: RawPredicateLock;

    fn raw(&self) -> &Arc<RwLock<Self::RawLock>>;

    /// Wakes up one blocked thread.
    ///
    /// Returns whether a thread was woken up.
    ///
    /// If there is a blocked thread on this condition variable, then it will be
    /// woken up from its call to `wait` or `wait_timeout`. Calls to
    /// `notify_one` are not buffered in any way.
    ///
    /// To wake up all threads, see `notify_all()`.
    #[inline]
    fn notify_one(&self) -> bool {
        if <Self::RawLock as RawPredicateLock>::has_parked(self.raw().read()) {
            self.notify_one_slow()
        } else {
            false
        }
    }

    /// Notify one parked thread. TODO: figure out if performance is improved if
    /// this is marked #[cold]
    fn notify_one_slow(&self) -> bool {
        let queue_addr = self.addr();
        let validate = || {
            // Unpark one thread if the underlying lock is unlocked, otherwise
            // just requeue everything to the lock.
            match self.raw.read().has_parked_then_is_locked() {
                Some(true) => {
                    self.raw.write().mark_parked();
                    RequeueOp::RequeueOne
                }
                Some(false) => RequeueOp::UnparkOne,
                None => RequeueOp::Abort,
            }
        };
        let callback = |_op, result: UnparkResult| {
            // Clear our state if there are no more waiting threads
            if !result.have_more_threads {
                self.raw.write().mark_unparked();
            }
            UNPARK_NORMAL
        };
        // we unqueue from and requeue to the same queue address, hence the
        // repeated arguments below
        let res =
            unsafe { parking_lot_core::unpark_requeue(queue_addr, queue_addr, validate, callback) };

        // Why do we return that unparked_threads + requeued_threads != 0?
        res.unparked_threads + res.requeued_threads != 0
    }

    /// Wakes up all blocked threads on this condvar.
    ///
    /// Returns the number of threads woken up.
    ///
    /// This method will ensure that any current waiters on the condition
    /// variable are awoken. Calls to `notify_all()` are not buffered in any
    /// way.
    ///
    /// To wake up only one thread, see `notify_one()`.
    ///
    /// TODO: figure out if performance is )proved if this is marked #[cold]
    #[inline]
    fn notify_all(&self) -> usize {
        if self.raw().read().has_parked() {
            self.notify_all_slow()
        } else {
            // Nothing to do if there are no waiting threads
            0
        }
    }

    fn notify_all_slow(&self) -> usize {
        let addr = self.addr();

        let validate = || {
            // Unpark one thread if the mutex is unlocked, otherwise just
            // requeue everything to the mutex.
            let requeue_op = self
                .raw
                .read()
                .has_parked_then_is_locked()
                .map(|is_locked| {
                    if is_locked {
                        RequeueOp::RequeueAll
                    } else {
                        RequeueOp::UnparkOneRequeueRest
                    }
                });

            // Clear our state since we are going to unpark or requeue all
            // threads.
            self.raw.write().mark_unparked();
            requeue_op.unwrap_or(RequeueOp::Abort)
        };
        let callback = |op, result: UnparkResult| {
            // If we requeued threads to the mutex, mark it as having parked
            // threads. The RequeueAll case is already handled above.
            if op == RequeueOp::UnparkOneRequeueRest && result.requeued_threads != 0 {
                self.raw.write().mark_parked();
            }
            UNPARK_NORMAL
        };
        let res = unsafe { parking_lot_core::unpark_requeue(addr, addr, validate, callback) };

        res.unparked_threads + res.requeued_threads
    }

    /// Blocks a thread until it acquires access, unless timeout is not passed.
    /// Uses [`park`] to cause thread to wait until it times out, or gets access
    /// from the binary lock.
    fn wait_until_access(&self, timeout: Option<Instant>) -> WaitOutcome<Self::RawLock> {
        let result;
        let mut requeued = false;
        let addr = self.addr();
        {
            result = unsafe {
                parking_lot_core::park(
                    addr,
                    // If validate returns false, then the thread owning this
                    // lock will not be added to the internal parking_lot wait
                    // queue. We always validate to true for now.
                    || true,
                    // For now, there is nothing special that we have to do
                    // before going to sleep.
                    || (),
                    // We just need to check whether or not we were requeued.
                    |k, _last_thread| requeued = k != addr,
                    DEFAULT_PARK_TOKEN,
                    timeout,
                )
            };
        }
        if requeued {
            WaitOutcome::Requeued
        } else if result.is_unparked() {
            self.raw
                .write()
                .lock()
                .map_or(WaitOutcome::Failure, |access| WaitOutcome::Success(access))
        } else {
            WaitOutcome::TimedOut
        }
    }
}

impl<RawLock, Lock: PredicateLock<RawLock = RawLock>> From<RawLock> for Lock {
    fn from(raw: RawLock) -> Self {
        Self::new(raw)
    }
}
