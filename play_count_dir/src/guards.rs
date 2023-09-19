use std::ops::{Deref, DerefMut};

use crate::cond_lock::{AccessKind, DataLock, LockState, RawLock};

pub trait SharedAccess<'a, T> {
    fn as_ref(&self) -> &'a T;
}

impl<'a, T, Shared: SharedAccess<'a, T>> Deref for Shared {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<'a, T, Exclusive: ExclusiveAccess<'a, T>> DerefMut for Exclusive {
    fn deref_mut(&self) -> &Self::Target {
        self.as_ref()
    }
}

pub trait ExclusiveAccess<'a, T>: SharedAccess<'a, T> + Deref + DerefMut {}

pub trait RawLockGuard<'lock, 'guard: 'lock, S: LockState>
where
    Self: 'guard + Drop,
{
    type RawLock: RawLock<'lock, 'guard, S>;
    type Access: AccessKind;

    fn new(lock: &'lock Self::RawLock) -> Self;

    /// Get the lock held by this
    fn raw_lock_mut(&mut self) -> &mut Option<&'lock Self::RawLock>;

    /// Notify threads waiting on the lock. This will be called by
    /// [`<RawLockGuard as Drop>::drop`] other drop operations are complete.
    fn notify(lock: &'lock Self::RawLock);
}

impl<'lock, 'guard, S: LockState, RawGuard: RawLockGuard<'lock, 'guard, S>> Drop for RawGuard {
    fn drop(&mut self) {
        if let Some(lock) = self.raw_lock_mut().take() {
            lock.state().write().unlock::<RawGuard::Access>();
            Self::notify(lock);
        }
    }
}

pub trait LockGuard<'data, 'guard: 'data, T, S: LockState> {
    type RawGuard: RawLockGuard<'data, 'guard, S>;
    type Lock: DataLock<'data, 'guard, T, Self>;
    type Data: SharedAccess<'data, T>;

    fn new(lock: &'data Self::Lock) -> Self;

    /// Get the raw lock guard that helps manage
    fn raw_guard(&self) -> &Self::RawGuard;

    /// Get access to the data being protected by this lock.
    fn data(&self) -> Self::Data;
}

impl<'data, 'guard: 'data, T, S: LockState, G: LockGuard<'data, 'guard, T, S>> Deref for G {
    type Target = <G::Data as Deref>::Target;
    fn deref(&self) -> &Self::Target {
        self.data().deref()
    }
}

impl<'data, 'guard: 'data, T, S: LockState, G: LockGuard<'data, 'guard, T, S>> DerefMut for G
where
    G::Data: ExclusiveAccess<'data, T>,
{
    fn deref_mut(&self) -> &Self::Target {
        self.data().deref_mut()
    }
}

pub type AccessResult<'lock, 'guard, S: LockState, G: RawLockGuard<'lock, 'guard, S>> =
    Result<G, AccessError>;

/// Returns the result of waiting on a [`CondLock`].
#[derive(Debug, Default)]
pub enum AccessError {
    #[default]
    AttemptFailed,
    Requeued,
    TimedOut,
}

impl AccessError {
    /// Check if the result indicates a timeout.
    pub fn timed_out(&self) -> bool {
        match self {
            Self::TimedOut => true,
            _ => false,
        }
    }
}
