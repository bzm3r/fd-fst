use std::sync::Arc;

use parking_lot::RwLock;
use rkyv::with::Raw;

use crate::{
    arc_locks::ArcRwLock,
    conditional_lock::{Lock, LockState},
    num::UnsignedNum,
};

pub trait CountingLockState {
    type N: UnsignedNum;

    fn curr(&mut self) -> Self::N;
    fn limit(&mut self) -> Self::N;

    /// Get the remaining the capacity currently recorded.
    fn remaining_capacity(&self) -> Self::N {
        self.curr() - self.limit()
    }

    /// Return true if the semaphore's counter was successfully incremented,
    /// false otherwise.
    fn increment(&self) -> bool {
        if self.remaining_capacity() > Self::N {
            self.curr().add(Self::N);
            true
        } else {
            false
        }
    }

    /// Return true if the semaphore's counter was successfully decremented,
    /// false otherwise.
    fn decrement(&self) -> bool {
        if self.curr() > Self::N {
            self.curr.sub(Self::N);
            false
        } else {
            false
        }
    }
}

impl<S: CountingLockState> LockState for S {
    fn locked(&self) -> bool {
        self.remaining_capacity() == 0
    }

    fn lock(&mut self) -> bool {
        self.increment()
    }

    fn unlock(&mut self) -> bool {
        self.decrement()
    }
}

#[derive(Debug)]
pub struct CountState<N: UnsignedNum> {
    limit: N,
    curr: N,
}

impl<N: UnsignedNum> CountState<N> {
    pub fn new(limit: N) -> Self {
        CountState {
            limit,
            curr: N::ZERO,
        }
    }
}

impl<N: UnsignedNum> CountingLockState for CountState<N> {
    fn curr(&self) -> N {
        self.curr
    }

    fn limit(&self) -> N {
        self.limit
    }
}

pub struct CountingLock<N: UnsignedNum> {
    state: ArcRwLock<CountState<N>>,
    parked: bool,
}

impl<N: UnsignedNum> CountingLock<N> {
    pub fn new(limit: N) -> Self {
        Self {
            state: CountState::new(limit).into(),
            parked: false,
        }
    }
}

impl<N: UnsignedNum> Lock for CountingLock<N> {
    type State = CountState<N>;

    fn lock_state(&self) -> &std::sync::Arc<parking_lot::RwLock<Self::State>> {
        &self.state
    }

    fn parked(&self) -> bool {
        self.parked
    }

    fn mark_parked(&mut self) {
        self.parked = true;
    }

    fn mark_unparked(&mut self) {
        self.parked = false;
    }
}
