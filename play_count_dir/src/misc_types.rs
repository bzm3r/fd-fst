use crossbeam::atomic::AtomicCell;
use parking_lot::{
    deadlock,
    lock_api::{GuardNoSend, RawRwLock},
    Condvar, Mutex, MutexGuard, RawMutex as UnitMutex, RawRwLock as UnitRwLock, RwLock,
    WaitTimeoutResult,
};
use parking_lot_core::{ParkResult, UnparkToken, DEFAULT_PARK_TOKEN};
use std::sync::Arc;
use std::{cell::Cell, char::MAX, sync::atomic::AtomicU8};
use std::{
    fmt::Debug,
    ops::{Add, Sub},
};
use std::{
    sync::atomic::{AtomicPtr, Ordering},
    time::Instant,
};

pub struct CellOpt<T: Clone + Debug> {
    slot: Cell<Option<T>>,
}

impl<T: Clone + Debug> Default for CellOpt<T> {
    fn default() -> Self {
        CellOpt {
            slot: Cell::new(None),
        }
    }
}

impl<T: Clone + Debug> Clone for CellOpt<T> {
    fn clone(&self) -> Self {
        self.apply_then_restore(|inner| CellOpt::new(inner.clone()))
            .unwrap_or_default()
    }
}

impl<T: Clone + Debug> Debug for CellOpt<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.apply_then_restore(|inner| write!(f, "{}", format!("Option::Some({:?})", inner)))
            .unwrap_or_else(|| write!(f, "None"))
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ValueError {
    Occupied,
    Empty,
}

pub struct InsertErr<T> {
    to_insert: T,
    err: ValueError,
}

impl<T: Clone + Debug> CellOpt<T> {
    #[inline]
    pub fn new(value: T) -> Self {
        Self {
            slot: Cell::new(value.into()),
        }
    }

    #[inline]
    pub fn apply_then_restore<U, F: Fn(T) -> U>(&self, f: F) -> Option<U> {
        self.take()
            .map(|t| {
                let u = f(t);
                self.overwrite(t);
                u
            })
            .ok()
    }

    #[inline]
    pub fn apply_and_update<F: Fn(T) -> T>(&self, f: F) {
        if let Ok(t) = self.take() {
            self.overwrite(f(t));
        }
    }

    #[inline]
    pub fn insert(&self, value: T) -> Result<(), InsertErr<T>> {
        if self.is_occupied() {
            Err(InsertErr {
                to_insert: value,
                err: ValueError::Occupied,
            })
        } else {
            self.overwrite(value);
            Ok(())
        }
    }

    #[inline]
    pub fn force_take(&self) -> T {
        self.take().unwrap()
    }

    #[inline]
    pub fn take(&self) -> Result<T, ValueError> {
        self.slot.take().ok_or(ValueError::Empty)
    }

    #[inline]
    pub fn is_occupied(&self) -> bool {
        if let Ok(value) = self.take() {
            self.overwrite(value);
            true
        } else {
            false
        }
    }

    #[inline]
    pub fn overwrite(&self, value: impl Into<Option<T>>) {
        self.slot.replace(value.into());
    }
}
