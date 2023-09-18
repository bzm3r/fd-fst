use crate::{
    arc_locks::{ArcMutex, ArcMutexFrom, ArcRwLockFrom},
    qbuf::QBuf,
    run_info::ThreadMetrics,
    semaphore::RwSemaphore,
};
use chrono::format::Item;
use core::num;
use fst::{raw, Error};
use parking_lot::{Condvar, Mutex, MutexGuard};
use rand::{rngs::SmallRng, seq::SliceRandom, thread_rng, SeedableRng};
use std::{
    borrow::Cow,
    cell::{Cell, UnsafeCell},
    collections::HashMap,
    fs::{read_dir, DirEntry, Metadata},
    hash::Hash,
    io::{Error as IoError, Result as IoResult},
    iter::once,
    ops::{AddAssign, Deref, DerefMut},
    path::{Path, PathBuf, Prefix},
    rc::Rc,
    slice::SliceIndex,
    str::FromStr,
    sync::{atomic::AtomicU8, Arc},
    time::Duration,
};

pub type DiskGuard<'a> = MutexGuard<'a, ()>;

pub type DiskMutex = Mutex<()>;

#[derive(Default, Debug)]
pub struct TaskCountVar {
    counter: Mutex<usize>,
    cv: Condvar,
}

impl TaskCountVar {
    fn notify_all(&self) {
        *self.counter.lock() += 1;
        self.cv.notify_all();
    }

    fn wait_for_ticket(&self) {
        let mut guarded_counter = self.counter.lock();
        if *guarded_counter == 0 {
            self.cv
                .wait_while(&mut guarded_counter, |counter| *counter > 0);
        }
        *guarded_counter -= 1;
    }
}

#[derive(Default)]
pub struct DiskRegistry {
    map_repr: HashMap<u8, Rc<Disk>>,
    vec_repr: Vec<(DiskPressure, Rc<Disk>)>,
}

impl DiskRegistry {
    pub fn new(map_repr: HashMap<u8, Rc<Disk>>, vec_repr: Vec<Rc<Disk>>) -> Self {
        let vec_repr = vec_repr
            .into_iter()
            .map(|disk| (disk.pressure(), disk))
            .collect();
        Self { map_repr, vec_repr }
    }
    pub fn get(&self, disk: u8) -> Option<Rc<Disk>> {
        self.map_repr.get(&disk).cloned()
    }

    fn exists_updated_disk(&self) -> bool {
        self.vec_repr.iter().any(|(_, disk)| disk.has_changed())
    }

    pub fn disks_by_pressure(&mut self) -> &[(usize, Rc<Disk>)] {
        if self.exists_updated_disk() {
            self.sort_by_pressure();
        }
        self.vec_repr.as_slice()
    }

    fn sort_by_pressure(&self) {
        self.vec_repr
            .sort_unstable_by(|a, b| a.pressure().cmp(&b.pressure()));
    }
}

impl From<HashMap<u8, Vec<PathBuf>>> for DiskRegistry {
    fn from(mut disks_and_paths: HashMap<u8, Vec<PathBuf>>) -> Self {
        let map_repr = disks_and_paths
            .into_iter()
            .map(|(disk, paths)| (disk, Rc::new(Disk::new(disk, paths))))
            .collect::<HashMap<u8, Rc<Disk>>>();
        let vec_repr = map_repr.values().cloned().collect();
        Self { map_repr, vec_repr }
    }
}

/// Assumed: greater the pressure on a disk, the greater the benefit of assigning
/// a thread to read from it.
pub type DiskPressure = usize;

pub struct Disks {
    registry: DiskRegistry,
    disks_by_pressure: Vec<(usize, Rc<Disk>)>,
    task_counter: TaskCountVar,
    rng: SmallRng,
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
        let registry: DiskRegistry = m.into();

        let disks_by_pressure = registry.disks_by_pressure();

        Disks {
            registry,
            rng: SmallRng::from_entropy(),
            task_counter: TaskCountVar::default(),
            disks_by_pressure,
        }
    }

    fn get_random_reader_from_high_pressure(
        &self,
        at_most: Option<usize>,
        metrics: Box<ThreadMetrics>,
    ) -> Option<TaskPacket> {
        let high_pressure_disks = self.registry.disks_by_pressure();
        for disk in high_pressure_disks
            .choose_multiple(self.rng.lock().deref_mut(), high_pressure_disks.len())
        {
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

#[derive(Debug, Clone)]
pub struct ErrorDir {
    #[allow(unused)]
    err: String,
    #[allow(unused)]
    path: PathBuf,
}

impl ErrorDir {
    pub fn new(path: PathBuf, err: IoError) -> Self {
        Self {
            err: err.to_string(),
            path,
        }
    }
}

#[derive()]
pub struct DiskEntry {
    path: PathBuf,
    metadata: Metadata,
}

pub type DiskEntries = QBuf<DiskEntry>;

#[derive()]
pub struct DiskContents {
    paths: DiskEntries,
    entries: Vec<DirEntry>,
    errors: Mutex<Vec<ErrorDir>>,
}

#[derive(Debug, Clone)]
pub struct NonDir(PathBuf);

#[derive(Debug, Clone)]
pub enum DiskContent {
    Complete {
        path: PathBuf,
        metadata: Metadata,
        contents: NonDir,
    },
    Partial {
        path: PathBuf,
        metadata: Metadata,
    },
    Error(ErrorDir),
    Path(PathBuf),
}

#[derive(Debug)]
pub struct Disk {
    disk: u8,
    contents: QBuf<DiskContent>,
    changed: bool,
}

pub struct TaskPacket<'a> {
    pub disk: Rc<Disk>,
    pub tasks: TaskSlice<'a>,
}

pub struct DirContents {
    sub_dirs: Vec<IoResult<DirEntry>>,
}

impl FromIterator<IoResult<DirEntry>> for DirContents {
    fn from_iter<Iterable: IntoIterator<Item = IoResult<DirEntry>>>(iter: Iterable) -> Self {
        Self {
            sub_dirs: iter.into_iter().collect(),
        }
    }
}

impl<'a> TaskPacket<'a> {
    fn new(parent: Rc<Disk>, tasks: TaskSlice) -> Self {
        Self {
            disk: parent,
            tasks,
        }
    }

    pub fn disk_read(&self) -> Vec<IoResult<DirContents>> {
        let disk_guard = self.disk.io_semaphore.read();
        // Here the assumption is that `read_dir` is the iterator object that
        // actually "causes" reads to be executed on the physical disk
        self.tasks
            .iter()
            .map(|p| read_dir(p).map(|iter| iter.collect()))
            .collect()
    }

    pub fn record_errors(&self, errors: Vec<ErrorDir>) {
        self.disk.errors.lock().extend(errors);
    }

    pub fn record_new_tasks(&self, dirs: FoundTasks) {
        self.disk.dirs.write(dirs)
    }

    pub fn record_results(&self, errors: Vec<ErrorDir>, found: FoundTasks) {
        self.disk.errors.lock()
    }
}

impl Disk {
    fn new(disk: u8, tasks: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            disk,
            contents: QBuf::from_iter(tasks),
            changed: false,
        }
    }

    fn pressure(&self) -> usize {
        (*self.contents.read(None).len() == 0)
            .then(|| self.dirs.reads_available())
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
        self.dirs.read(at_most)
    }

    fn push(&self, tasks: FoundTasks) {
        self.dirs.push(tasks)
    }
}
