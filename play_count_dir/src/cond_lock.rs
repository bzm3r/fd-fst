/// Parking refers to suspending the thread while simultaneously enqueuing it on
/// a queue keyed by some address. Unparking refers to dequeuing a thread from a
/// queue keyed by some address and resuming it.
use std::{fmt::Debug, sync::Arc, time::Instant};

use parking_lot::RwLock;
use parking_lot_core::{RequeueOp, UnparkResult, UnparkToken, DEFAULT_PARK_TOKEN};

use crate::guards::{AccessError, AccessResult, LockGuard, RawLockGuard};

pub const UNPARK_NORMAL: UnparkToken = UnparkToken(0);

pub trait AccessKind: Copy + Clone + Debug {}

pub trait LockState
where
    Self: Sized,
{
    type SupportedAccesses: AccessKind;
    /// Determine if this lock should be considered locked.
    ///
    /// This predicate is fundamental to governing the behaviour of RawLock
    fn is_locked(&self) -> bool;

    /// Attempt to perform a lock operation on this state, and return whether
    /// it succeeded.
    fn try_lock<'lock, 'guard, Guard: RawLockGuard<'lock, 'guard, Self>>(&mut self) -> bool;

    /// Perform an unlock operation on this state.
    fn unlock<'lock, 'guard, Guard: RawLockGuard<'lock, 'guard, Self>>(&mut self) -> bool;
}

pub trait RawLock<'lock, 'guard: 'lock, S: LockState>
where
    Self: 'lock + Sized,
{
    /// The address used to look up the queue of threads parked due this being
    /// locked.
    #[inline]
    fn addr(&self) -> usize {
        self as *const _ as usize
    }

    /// Get the underlying condition governing this lock.
    fn state(&self) -> &RwLock<S>;

    /// Check if this lock has parked (suspended) threads that have not yet
    /// been woken up
    fn parked(&self) -> &RwLock<bool>;

    /// Mark lock as having parked (suspended) threads
    fn mark_parked(&self);

    /// Mark lock as having thread that are parked due to its condition
    fn mark_unparked(&self);

    /// Notify one blocked thread for wake-up.
    ///
    /// Returns whether a thread was woken up.
    ///
    /// To wake up all threads, see `notify_all()`.
    #[inline]
    fn notify_one(&self) -> bool {
        if *self.parked().read() {
            self.notify_one_slow()
        } else {
            false
        }
    }

    /// Notify one blocked thread for wake-up.
    ///
    /// Returns whether a thread was woken up.
    ///
    /// TODO: figure out if performance is improved if this is marked #[cold]
    fn notify_one_slow(&self) -> bool {
        let queue_addr = self.addr();
        let validate = || {
            // Unpark one thread if the underlying lock is unlocked, otherwise
            // just requeue everything to the lock.
            match self
                .parked()
                .read()
                .then(|| !self.state().read().is_locked())
            {
                Some(true) => {
                    self.mark_parked();
                    RequeueOp::RequeueOne
                }
                Some(false) => RequeueOp::UnparkOne,
                None => RequeueOp::Abort,
            }
        };
        let callback = |_op, result: UnparkResult| {
            // Clear our state if there are no more waiting threads
            if !result.have_more_threads {
                self.mark_unparked();
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

    /// Notify all blocked threads for wake-up.
    ///
    /// Returns the number of threads woken up.
    ///
    /// To wake up only one thread, see `notify_one()`.
    #[inline]
    fn notify_all(&self) -> usize {
        if *self.parked().read() {
            self.notify_all_slow()
        } else {
            // Nothing to do if there are no waiting threads
            0
        }
    }

    /// Notify all blocked threads for wake-up.
    ///
    /// Returns the number of threads woken up.
    ///
    /// TODO: figure out if performance is )proved if this is marked #[cold]
    #[inline]
    fn notify_all_slow(&self) -> usize {
        let addr = self.addr();

        let validate = || {
            // Unpark one thread if the mutex is unlocked, otherwise just
            // requeue everything to the mutex.
            let requeue_op = self.parked().read().then(|| {
                if self.state().read().is_locked() {
                    RequeueOp::RequeueAll
                } else {
                    RequeueOp::UnparkOneRequeueRest
                }
            });

            // Clear our state since we are going to unpark or requeue all
            // threads.
            requeue_op.unwrap_or_else(|| {
                self.mark_unparked();
                RequeueOp::Abort
            })
        };
        let callback = |op, result: UnparkResult| {
            // If we requeued threads to the mutex, mark it as having parked
            // threads. The RequeueAll case is already handled above.
            if op == RequeueOp::UnparkOneRequeueRest && result.requeued_threads != 0 {
                self.mark_parked();
            }
            UNPARK_NORMAL
        };
        let res = unsafe { parking_lot_core::unpark_requeue(addr, addr, validate, callback) };

        res.unparked_threads + res.requeued_threads
    }

    /// Blocks a thread until it can either:
    /// * attempt to access through the lock,
    /// * times out.
    ///
    /// Uses [`park`] to cause thread to suspend the thread, or gets access
    /// from the binary lock.
    fn wait_for_access_until<G: RawLockGuard<'lock, 'guard, S>>(
        &self,
        timeout: Option<Instant>,
    ) -> AccessResult<'lock, 'guard, S, G> {
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
            Err(AccessError::Requeued)
        } else if result.is_unparked() {
            self.state()
                .write()
                .try_lock()
                .then(|| Self::Guard::new(self.clone()))
                .ok_or(AccessError::AttemptFailed)
        } else {
            Err(AccessError::TimedOut)
        }
    }

    /// Blocks thread until it has received access.
    fn wait_for_access<Guard: RawLockGuard<'lock, 'guard, S>>(&self) -> Guard {
        let mut result: AccessResult<Self> = Err(AccessError::default());
        while result.as_ref().err().map_or(false, |err| !err.timed_out()) {
            result = self.wait_for_access_until::<Guard>(None);
        }
        result.unwrap()
    }
}

pub trait DataLock<'data, 'guard: 'data, T, S: LockState>
where
    Self: 'data,
{
    type Raw: RawLock<'data, 'guard, S>;

    fn raw_lock(&self) -> &Arc<Self::Raw>;

    fn data(&self) -> &'data T;

    fn wait_for_access<G: LockGuard<'data, 'guard, T, S>>(&self) -> G {
        G::new(
            self.raw_lock().wait_for_access::<LockGuard::RawGuard>(),
            self.data(),
        )
    }
}
