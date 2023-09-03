use crate::{
    hist_defs::{ProcessingRate, TimeSpan},
    misc_types::{ArcMutex, ArcMutexFrom, ArcRwLockFrom, CellSlot},
    run_info::ThreadMetrics,
    shared_buf::SharedBuf,
    task_results::WorkResult,
    IoErr, IoResult,
};
use chrono::format::Item;
use core::num;
use fst::raw;
use parking_lot::{Condvar, Mutex, MutexGuard};
use rand::{rngs::SmallRng, seq::SliceRandom, thread_rng, SeedableRng};
use std::{
    borrow::Cow,
    cell::{Cell, UnsafeCell},
    collections::HashMap,
    fs::{read_dir, DirEntry},
    hash::Hash,
    iter::once,
    ops::{AddAssign, Deref, DerefMut},
    path::{Path, PathBuf, Prefix},
    slice::SliceIndex,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

pub struct CondMutex<T> {
    mutex: Mutex<T>,
    cv: Condvar,
}

impl<T> CondMutex<T> {
    pub fn lock<'a>(&self) -> MutexGuard<'a, T>
    where
        T: 'a,
    {
        self.mutex.lock().unwrap()
    }

    pub fn wait_for_cond<'a, P: Fn(&mut T) -> bool>(&self, cond: P) -> MutexGuard<'a, T>
    where
        T: 'a,
    {
        let mut guard = self.lock();
        if !cond(guard.deref_mut()) {
            self.cv.wait_while(&mut guard, cond).unwrap();
        }
        guard
    }

    #[inline]
    pub fn notify_all(&self) {
        self.cv.notify_all();
    }
}

pub type DiskGuard<'a> = MutexGuard<'a, ()>;

pub type DiskMutex = Mutex<()>;

#[derive(Default, Debug)]
pub struct TaskCountVar {
    counter: Mutex<usize>,
    cv: Condvar,
}

impl TaskCountVar {
    fn notify_all(&self) {
        *self.counter.lock().unwrap() += 1;
        self.cv.notify_all();
    }

    fn wait_for_ticket(&self) {
        let mut guarded_counter = self.counter.lock().unwrap();
        if *guarded_counter == 0 {
            guarded_counter = self
                .cv
                .wait_while(guarded_counter, |counter| *counter > 0)
                .unwrap();
        }
        *guarded_counter -= 1;
    }
}

#[derive(Default)]
pub struct DiskRegistry {
    m: HashMap<u8, Arc<Disk>>,
    v: CellSlot<Vec<Arc<Disk>>>,
}

impl DiskRegistry {
    fn get(&self, disk: u8) -> Option<Arc<Disk>> {
        self.m.get(&disk).cloned()
    }

    fn disks_with_highest_pressures(&self) -> Vec<Arc<Disk>> {
        self.sort_by_pressure();
        self.v.apply_then_restore(|v| {
            let highest = v.last().map_or(0, |arc_disk| arc_disk.pressure());
            let mut high_pressures = Vec::new();
            for disk in v.iter() {
                if disk.pressure() == highest {
                    high_pressures.push(disk.clone());
                } else {
                    break;
                }
            }
            high_pressures
        })
    }

    fn sort_by_pressure(&self) {
        self.v.apply_and_update(|mut v| {
            v.sort_unstable_by(|a, b| a.pressure().cmp(&b.pressure()));
            v
        });
    }
}

impl From<HashMap<u8, Vec<PathBuf>>> for DiskRegistry {
    fn from(mut m: HashMap<u8, Vec<PathBuf>>) -> Self {
        let m = m
            .into_iter()
            .map(|(disk, paths)| (disk, Arc::new(Disk::new(disk, paths))))
            .collect::<HashMap<u8, Arc<Disk>>>();
        let v = CellSlot::new(m.values().cloned().collect());
        Self { m, v }
    }
}

pub struct Disks {
    registry: Arc<Mutex<DiskRegistry>>,
    task_counter: TaskCountVar,
    rng: Arc<Mutex<SmallRng>>,
}

impl Disks {
    pub fn new(seed_paths: Vec<PathBuf>) -> Self {
        let mut m: HashMap<u8, Vec<PathBuf>> = HashMap::new();

        seed_paths.into_iter().for_each(|disk_path| {
            if let Some(disk) = disk_path.components().find_map(|c| match c {
                std::path::Component::Prefix(prefix) => match prefix.kind() {
                    Prefix::Disk(disk) => Some(disk),
                    _ => None,
                },
                _ => None,
            }) {
                m.entry(disk).and_modify(|v| v.push(disk_path)).or_default();
            }
        });

        Disks {
            registry: DiskRegistry::arc_mutex_from(m),
            rng: Arc::new(Mutex::new(SmallRng::from_entropy())),
            task_counter: TaskCountVar::default(),
        }
    }

    pub fn insert_tasks(&mut self, disk: u8, tasks: FoundTasks) {
        if let Some(disk) = self.registry.lock().unwrap().get(&disk) {
            disk.extend(tasks)
        }
        self.task_counter.notify_all();
    }

    fn get_random_reader_from_high_pressure(
        &self,
        at_most: Option<usize>,
        metrics: Box<ThreadMetrics>,
    ) -> Option<TaskPacket> {
        let high_pressure_disks = self.registry.disks_with_highest_pressures();
        for disk in high_pressure_disks.choose_multiple(
            self.rng.lock().unwrap().deref_mut(),
            high_pressure_disks.len(),
        ) {
            let reader = disk.get_reader(self.clone(), at_most, metrics);
            if reader.is_some() {
                return reader;
            }
        }
        None
    }

    pub fn get_readers_for(
        &self,
        at_most: Option<usize>,
        metrics: Box<ThreadMetrics>,
    ) -> TaskPacket {
        self.get_random_reader_from_high_pressure(at_most, metrics)
            .unwrap_or_else(|| {
                self.task_counter.wait_for_ticket();
                self.get_reader(at_most, metrics)
            })
    }
}

pub type FoundTasks = Vec<PathBuf>;
pub type TaskSlice<'a> = &'a [PathBuf];

#[derive(Debug)]
struct ErrorDir {
    #[allow(unused)]
    err: IoErr,
    #[allow(unused)]
    path: PathBuf,
}

impl ErrorDir {
    fn new(path: PathBuf, err: IoErr) -> Self {
        Self { err, path }
    }
}

#[derive(Debug)]
pub struct Disk {
    disk: u8,
    task_buf: SharedBuf<PathBuf>,
    access_mutex: Mutex<()>,
    errors: Mutex<Vec<ErrorDir>>,
}

pub struct TaskPacket<'a> {
    pub disk: Arc<Disk>,
    pub tasks: TaskSlice<'a>,
}

pub struct DirContents {
    sub_dirs: Vec<IoResult<DirEntry>>,
}

pub struct DiskReadResults {
    buf: FlatBuf,
}

impl<'a> TaskPacket<'a> {
    fn new(parent: Arc<Disk>, tasks: TaskSlice) -> Self {
        Self {
            disk: parent,
            tasks,
        }
    }

    pub fn disk_read(&self) -> Vec<IoResult<DirContents>> {
        let disk_guard = self.disk.access_mutex.lock().unwrap();
        // Here the assumption is that `read_dir` is the iterator object that
        // actually "causes" reads to be executed on the physical disk
        self.tasks
            .iter()
            .map(|p| read_dir(p).map(|iter| iter.collect()))
            .collect()
    }

    pub fn record_errors(&self, errors: Vec<ErrorDir>) {
        self.disk.errors.lock().unwrap().extend(errors);
    }

    pub fn record_new_tasks(&self, dirs: FoundTasks) {
        self.disk.task_buf.write(dirs)
    }

    pub fn record_results(&self, errors: Vec<ErrorDir>, found: FoundTasks) {
        self.disk.errors.lock().unwrap()
    }
}

impl Disk {
    fn new(disk: u8, tasks: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            disk,
            task_buf: SharedBuf::new(tasks).into(),
            access_mutex: DiskMutex::default(),
            errors: Mutex::new(Vec::new()),
            active_readers: RwLock::new(0),
        }
    }

    fn pressure(&self) -> usize {
        (*self.active_readers.read().unwrap() == 0)
            .then(|| self.task_buf.reads_available())
            .flatten()
            .unwrap_or(0)
    }

    fn get_reader(
        &self,
        parent: Arc<Disks>,
        at_most: Option<usize>,
        metrics: Box<ThreadMetrics>,
    ) -> Option<TaskPacket> {
        self.get_tasks(at_most).map(|tasks| TaskPacket {
            disk: parent,
            tasks,
        })
    }

    fn get_tasks(&self, at_most: Option<usize>) -> Option<TaskSlice> {
        self.task_buf.read(at_most)
    }

    fn push(&self, tasks: FoundTasks) {
        self.task_buf.push(tasks)
    }
}
