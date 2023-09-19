use std::sync::Arc;

use parking_lot::RwLock;
use rkyv::with::Raw;

use crate::{
    arc_locks::ArcRwLock,
    cond_lock::{LockState, RawLock},
    num::UnsignedNum,
};

pub struct Counter<N: UnsignedNum> {
    curr: N,
    limit: N,
}

impl<N: UnsignedNum> Counter<N> {
    pub fn new(limit: N) -> Self {
        Self {
            curr: N::ZERO,
            limit,
        }
    }

    /// Get the remaining the capacity currently recorded.
    fn remaining_capacity(&self) -> N {
        self.curr - self.limit
    }

    /// Return true if the semaphore's counter was successfully incremented,
    /// false otherwise.
    fn increment(&self) -> bool {
        if self.remaining_capacity() > N::ZERO {
            self.curr.add(N::ONE);
            true
        } else {
            false
        }
    }

    /// Return true if the semaphore's counter was successfully decremented,
    /// false otherwise.
    fn decrement(&self) -> bool {
        if self.curr > N::ZERO {
            self.curr.sub(N::ONE);
            false
        } else {
            false
        }
    }
}

impl<N: UnsignedNum> LockState for Counter<N> {
    fn is_locked(&self) -> bool {
        self.remaining_capacity() > N::ZERO
    }

    fn try_lock(&mut self) -> bool {
        self.increment()
    }

    fn unlock(&mut self) -> bool {
        self.decrement()
    }
}

/// An access counting lock. It permits a finite number of accesses.
///
/// It is a `RawCondLock`: so while it gives out access, these accesses are
/// abstract and not tied to an actual resource.
pub struct AcLock<N: UnsignedNum> {
    counter: RwLock<Counter<N>>,
    parked: RwLock<bool>,
}

impl<N: UnsignedNum> AcLock<N> {
    pub fn new(max_access: N) -> Self {
        Self {
            counter: RwLock::new(Counter::new(max_access)),
            parked: RwLock::new(false),
        }
    }
}

impl<N: UnsignedNum> RawLock for AcLock<N> {
    type State = Counter<N>;

    fn state(&self) -> &RwLock<Self::State> {
        &self.counter
    }

    fn parked(&self) -> &RwLock<bool> {
        &self.parked
    }

    fn mark_parked(&self) {
        *self.parked.write() = true;
    }

    fn mark_unparked(&self) {
        *self.parked.write() = false;
    }
}
