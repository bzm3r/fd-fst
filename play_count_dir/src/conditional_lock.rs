/// Parking refers to suspending the thread while simultaneously enqueuing it on
/// a queue keyed by some address. Unparking refers to dequeuing a thread from a
/// queue keyed by some address and resuming it.
use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
    sync::{atomic::Ordering, Arc},
    time::Instant,
};

use crossbeam::atomic::AtomicCell;
use parking_lot::{RawRwLock, RwLock, RwLockReadGuard, RwLockWriteGuard};
use parking_lot_core::{RequeueOp, UnparkResult, UnparkToken, DEFAULT_PARK_TOKEN};

use crate::{arc_locks::ArcRwLock, num_check::FiniteTest};

pub const UNPARK_NORMAL: UnparkToken = UnparkToken(0);

pub trait LockState
where
    Self: Sized + Debug,
{
    /// Determine if this lock should be considered locked.
    ///
    /// This predicate is fundamental to governing the behaviour of RawLock
    fn locked(&self) -> bool;

    /// Apply a lock operation to this binary lock, if possible.
    ///
    /// Return whether or not the lock operation was successful.
    fn lock(&mut self) -> bool;

    /// Apply an unlock operation to this binary lock.
    ///
    /// Return whether or not the unlock operation was successful.
    fn unlock(&mut self);
}

pub trait LockGuard<'a>
where
    Self: 'a,
{
    type L: Lock;

    /// Create a new guard around the provided raw binary lock
    fn new(raw: Arc<Self::L>) -> Self;

    /// The "parent" lock this conditional is associated with.
    fn parent(&self) -> &Arc<Self::L>;
}

impl<'a, G: LockGuard<'a>> Drop for G {
    fn drop(&mut self) {
        self.parent().lock_state().write().unlock()
    }
}

pub trait AbstractReadGuard<'a>
where
    Self: LockGuard<'a> + Deref<Target = Self::Data>,
{
    type Data;
}

/// Considered "raw" because it does not refer to any resource it is ultimately
/// used to protect.
pub trait AbstractWriteGuard<'a>
where
    Self: LockGuard<'a> + Deref<Target = Self::Data> + DerefMut<Target = Self::Data>,
{
    type Data;
}

pub type WaitResult<'a, G: LockGuard<'a>> = Result<G, WaitError>;

/// Returns the result of waiting on a [`CondLock`].
#[derive(Debug)]
pub enum WaitError {
    Failure,
    Requeued,
    TimedOut,
}

impl WaitError {
    /// Check if the result indicates a timeout.
    pub fn timed_out(&self) -> bool {
        match self {
            Self::TimedOut => true,
            _ => false,
        }
    }
}

/// Considered "raw" because it abstracts away any resource that it might
/// ultimately be protecting.
pub trait Lock: Debug {
    type State: LockState;
    /// Retrieves the underlying lock state.
    fn lock_state<'a>(self: &'a Arc<Self>) -> &'a RwLock<Self::State>;

    /// Check if this lock has parked (suspended) threads that have not yet
    /// been woken up
    fn parked(&self) -> bool;

    /// Mark lock as having parked (suspended) threads
    fn mark_parked(&mut self);

    /// Mark lock as having thread that are parked due to its condition
    fn mark_unparked(&mut self);

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
        if self.parked() {
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
            match self.parked().then(|| !self.lock_state().read().locked()) {
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
        if self.lock_state().read().has_parked() {
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
    fn wait_until_access(&self, timeout: Option<Instant>) -> WaitResult<Self::State> {
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
            WaitResult::Requeued
        } else if result.is_unparked() {
            self.raw
                .write()
                .lock()
                .map_or(WaitResult::Failure, |access| WaitResult::Success(access))
        } else {
            WaitResult::TimedOut
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

impl<L: Lock> Addressed for L {}
