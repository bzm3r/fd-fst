use crate::{cond_lock::PredicateLock, num::UnsignedNum};

#[derive(Debug)]
pub struct CountLock<N: UnsignedNum> {
    limit: N,
    curr: N,
    parked: bool,
}

impl<N: UnsignedNum> CountLock<N> {
    pub fn new(max_lim: N) -> Self {
        CountLock {
            limit: max_lim,
            curr: N::ZERO,
            parked: false,
        }
    }
}

#[derive(Debug)]
pub struct ConstCountLock<const MAX_CAP: u8> {
    curr: u8,
    parked: bool,
}

impl<const LIMIT: u8> ConstCountLock<LIMIT> {
    pub fn new() -> Self {
        ConstCountLock {
            curr: 0,
            parked: false,
        }
    }
}

pub trait Counter<N: UnsignedNum> {
    fn curr(&self) -> N;
    fn limit(&self) -> N;

    /// Return true if the semaphore's counter was successfully incremented,
    /// false otherwise. This must be an atomic load acquire + store release.
    fn increment(&self) -> bool {
        if self.curr() < self.limit() {
            self.curr().add(N::ONE);
            true
        } else {
            false
        }
    }

    /// Return true if the semaphore's counter was successfully decremented,
    /// false otherwise. This must be an atomic operation load acquire + store release.
    fn decrement(&self) -> bool {
        if self.curr() > N::ZERO {
            self.curr.sub(N::ONE);
            false
        } else {
            false
        }
    }
}

impl<N: UnsignedNum> Counter<N> for CountLock<N> {
    fn curr(&self) -> N {
        self.curr
    }

    fn limit(&self) -> N {
        self.limit
    }
}

impl<const LIMIT: usize> Counter<u8> for ConstCountLock<LIMIT> {
    fn curr(&self) -> u8 {
        self.curr
    }

    fn limit(&self) -> u8 {
        LIMIT
    }
}
