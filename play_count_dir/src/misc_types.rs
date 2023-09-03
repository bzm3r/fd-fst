use parking_lot::{Mutex, RwLock};
use std::cell::Cell;
use std::fmt::Debug;
use std::sync::Arc;

#[derive(Default)]
pub struct CellSlot<T: Clone + Debug> {
    slot: Cell<Option<T>>,
}

impl<T: Clone + Debug> Clone for CellSlot<T> {
    fn clone(&self) -> Self {
        self.apply_then_restore(|inner| CellSlot::new(inner.clone()))
    }
}

impl<T: Clone + Debug> Debug for CellSlot<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.apply_then_restore(|inner| format!("{:?}", inner)),
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ValueError {
    Occupied,
    Empty,
}

impl<T: Clone + Debug> CellSlot<T> {
    #[inline]
    pub fn new(value: T) -> Self {
        Self {
            slot: Cell::new(value.into()),
        }
    }

    #[inline]
    pub fn apply_then_restore<U, F: Fn(&T) -> U>(&self, f: F) -> U {
        let slot = self.force_take();
        let u = f(&slot);
        self.overwrite(slot);
        u
    }

    #[inline]
    pub fn apply_and_update<F: Fn(T) -> T>(&self, f: F) {
        self.overwrite(f(self.force_take()));
    }

    #[inline]
    pub fn insert(&self, value: T) -> Result<(), (T, ValueError)> {
        if self.is_occupied() {
            Err((value, ValueError::Occupied))
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

pub type ArcRwLock<T> = Arc<RwLock<T>>;
pub type ArcMutex<T> = Arc<Mutex<T>>;

pub trait ArcRwLockFrom
where
    Self: Sized,
{
    fn arc_rwlock_from(value: Self) -> ArcRwLock<Self> {
        Arc::new(RwLock::new(value))
    }
}

impl<T> ArcRwLockFrom for T where T: Sized {}

pub trait ArcMutexFrom
where
    Self: Sized,
{
    fn arc_mutex_from(value: Self) -> ArcMutex<Self> {
        Arc::new(Mutex::new(value))
    }
}

impl<T> ArcMutexFrom for T where T: Sized {}
