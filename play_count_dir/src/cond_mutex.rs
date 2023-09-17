use parking_lot::{Condvar, Mutex, MutexGuard};

#[derive(Debug)]
pub struct CondMutex<T> {
    mutex: Mutex<T>,
    cv: Condvar,
}

impl<T: Default> Default for CondMutex<T> {
    fn default() -> Self {
        Self {
            mutex: Mutex::new(T::default()),
            cv: Condvar::new(),
        }
    }
}

impl<T> CondMutex<T> {
    pub fn lock<'a>(&self) -> MutexGuard<'a, T>
    where
        T: 'a,
    {
        self.mutex.lock()
    }

    pub fn wait_for_cond<'a, P: Fn(&mut T) -> bool>(&self, cond: P) -> MutexGuard<'a, T>
    where
        T: 'a,
    {
        let mut guard = self.lock();
        if !cond(&mut guard) {
            self.cv.wait_while(&mut guard, cond);
        }
        guard
    }

    #[inline]
    pub fn notify_all(&self) {
        self.cv.notify_all();
    }
}
