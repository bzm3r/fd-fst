use parking_lot::{Condvar, Mutex, RwLock};
use std::{
    collections::HashSet,
    sync::{atomic::AtomicU8, TryLockError},
    time::Duration,
};

use crate::{
    conditional_lock::LockGuard,
    intervals::Interval,
    num::UnsignedNum,
    semaphore::{RwSemaphore, SemaphoreReadGuard},
};

#[derive(Debug, Default)]
struct CursorBuf<T> {
    buf: Vec<T>,
    cursor: usize,
}

impl<T> CursorBuf<T> {
    #[inline]
    unsafe fn extend(&self, new_values: Vec<T>) {
        self.buf.extend(new_values)
    }

    #[inline]
    fn read<'a>(&'a mut self, at_most: Option<usize>) -> &'a [T] {
        let curr_len = self.buf.len();
        let until = at_most
            .map(|n| (self.cursor + n).min(curr_len))
            .unwrap_or(curr_len);
        let slice = &self.buf[self.cursor..until];
        self.cursor = until;
        slice
    }

    #[inline]
    fn reads_available(&self) -> usize {
        (self.buf.len() > self.cursor)
            .then_some(self.buf.len() - self.cursor)
            .unwrap_or(0)
    }
}

impl<T> FromIterator<T> for CursorBuf<T> {
    fn from_iter<Iterable: IntoIterator<Item = T>>(iter: Iterable) -> Self {
        Self {
            buf: iter.into_iter().collect(),
            cursor: 0,
        }
    }
}

struct Intervals {
    intervals: HashSet<Interval>,
}

#[derive(Debug)]
pub struct QBuf<T> {
    buf: RwSemaphore<u8, CursorBuf<T>>,
    exclusive_intervals: RwLock<Intervals>,
    shared_intervals: RwLock<Intervals>,
}

impl<T> QBuf<T> {
    pub fn new(init: impl IntoIterator<Item = T>) -> Self {
        Self {
            buf: RwSemaphore::new(u8::MAX, 1, CursorBuf::default()),
            exclusive_intervals: RwLock::new(Intervals::new()),
            shared_intervals: RwLock::new(Intervals::new()),
        }
    }

    pub fn append(&self, items: Vec<T>) {
        self.buf().write().extend(items);
        self.buf().notify_all();
    }

    pub fn reads_active(&self) -> u8 {
        self.buf.readers()
    }

    pub fn reads_available(&self) -> Option<usize> {
        match self.buf.try_lock() {
            Ok(buf_guard) => buf_guard.reads_available().into(),
            Err(TryLockError::WouldBlock) => None,
            Err(poisoned) => panic!("{}", poisoned),
        }
    }

    pub fn read<'a>(&'a self, at_most: Option<usize>) -> SemaphoreReadGuard<'a, u8, [T]> {
        let mut buf_guard = self.buf.lock();
        if buf_guard.reads_available() == 0 {
            let (buf_guard, wait_timeout) = self
                .read_ready
                .wait_timeout_while(buf_guard, Duration::from_micros(50), |buf| {
                    buf.reads_available() > 0
                })
                .unwrap();
            if wait_timeout.timed_out() {
                return None;
            }
        }
        let read_slice = buf_guard.read(at_most);
        self.read_ready.notify_all();
        read_slice.into()
    }
}
