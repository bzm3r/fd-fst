use std::sync::Arc;

use parking_lot::{RwLock, Mutex};


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
