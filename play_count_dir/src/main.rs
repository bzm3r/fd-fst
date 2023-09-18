// #![cfg(feature = "debug_macros")]
#![allow(unused)]
#![feature(trace_macros)]
trace_macros!(false);

mod adp_num;
mod arc_locks;
mod atomic_counter;
mod conditional_lock;
mod cond_mutex;
mod disk;
mod flat_buf;
mod hist_defs;
mod history;
mod intervals;
mod misc_types;
mod num;
mod num_absf64;
mod num_check;
mod num_conv;
mod num_display;
mod num_duration;
mod num_f64;
mod num_hist;
mod num_isize;
mod num_u32;
mod num_u8;
mod num_usize;
mod qbuf;
mod run_info;
mod semaphore;
mod sig_figs;
mod task_results;
mod work;
mod counting_lock;

use crossbeam::channel::{bounded, Receiver, SendError, Sender, TryRecvError};
use disk::{DiskRegistry, ErrorDir, FoundTasks, TaskPacket};
use hist_defs::{ProcessingRate, TimeSpan};
use history::{AvgInfoBundle, HistoryVec};
use num_conv::IntoNum;
use num_hist::{HistData, HistoryNum};
use parking_lot::{RwLock, RwLockReadGuard};
use paste::paste;
use run_info::ThreadMetrics;
use std::fmt::Debug;
use std::fs::{read_dir, DirEntry};
use std::io::Error as IoErr;
use std::io::Result as IoResult;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::thread::{self, sleep};
use std::time::{Duration, Instant};
use work::WorkError;
use work::WorkResult;

use crate::disk::Disks;
use crate::history::AvgInfoWithSummaries;
use crate::num_display::NumDisplay;

pub const NUM_THREADS: usize = 3;
pub const DEFAULT_EXECUTE_LOOP_SLEEP: Duration = Duration::from_micros(10);
pub const UPDATE_PRINT_DELAY: Duration = Duration::from_secs(5);
pub const DEFAULT_WORK_CHUNK_SIZE: usize = 64;
pub const MAX_HISTORY: usize = 10;

#[derive(PartialEq, Eq, Copy, Clone)]
enum Status {
    Busy,
    Idle,
}

struct BufferedSender<T> {
    buffer: Vec<T>,
    sender: Sender<Vec<T>>,
}

impl<T> BufferedSender<T> {
    fn new(sender: Sender<Vec<T>>, initial_capacity: usize) -> Self {
        BufferedSender {
            buffer: Vec::with_capacity(initial_capacity),
            sender,
        }
    }
    fn push(&mut self, value: T) {
        self.buffer.push(value)
    }

    fn flush_send(&mut self) -> Result<(), SendError<Vec<T>>> {
        self.sender.send(self.buffer.drain(0..).collect())
    }
}

#[derive(Default)]
struct Printer(Option<BufferedSender<String>>);

impl Printer {
    fn new(sender: Option<Sender<Vec<String>>>) -> Self {
        if let Some(sender) = sender {
            Printer(Some(BufferedSender::new(sender, 20)))
        } else {
            Printer(None)
        }
    }

    fn push(&mut self, lazy_value: impl FnOnce() -> String) {
        self.0
            .as_mut()
            .map(|buf_sender| buf_sender.push(lazy_value()))
            .unwrap_or(());
    }

    fn flush_send(&mut self) -> Result<(), SendError<Vec<String>>> {
        let result = self
            .0
            .as_mut()
            .map(|buf_sender| buf_sender.flush_send())
            .unwrap_or(Ok(()));
        result
    }
}

macro_rules! create_paired_comm {
    (
        $name:ident$(($($params:tt)+))? ;
        LHS: $(($fid:ident, $fty:ty)),+ ;
        RHS: $(($gid:ident, $gty:ty)),+
    ) => {
        paste! {
            struct $name$(<$($params:ident),+>)? {
                $([< $fid _sender >]: Sender<$fty>,)+
                $([< $gid _receiver >]: Receiver<$gty>,)+
            }
        }
    }
}

macro_rules! create_paired_comms {
    (
        [ $lhs_snake_name:ident ; $lhs_struct_id:ident$(($($lhs_params:tt)+))? ; $(($fid:ident, $fty:ty)),+ ] <->
        [ $rhs_snake_name:ident ; $rhs_struct_id:ident$(($($rhs_params:tt)+))? ; $(($gid:ident, $gty:ty)),+ ]
    ) => {
        create_paired_comm!(
            $lhs_struct_id$(($($lhs_params)+))? ;
            LHS: $(($fid, $fty)),+ ;
            RHS: $(($gid, $gty)),+
        );
        create_paired_comm!(
            $rhs_struct_id$(($($rhs_params)+))? ;
            LHS: $(($gid, $gty)),+ ;
            RHS: $(($fid, $fty)),+
        );

        paste! {
            fn [< new_ $lhs_snake_name _to_ $rhs_snake_name _comms>]() ->
                ($lhs_struct_id, $rhs_struct_id)
            {
                $(let ([< $fid _sender>], [< $fid _receiver>]) = bounded(100);)+
                $(let ([< $gid _sender>], [< $gid _receiver>]) = bounded(100);)+

                (
                    $lhs_struct_id {
                        $([< $fid _sender >],)+
                        $([< $gid _receiver >],)+
                    },
                    $rhs_struct_id {
                        $([< $gid _sender >],)+
                        $([< $fid _receiver >],)+
                    }
                )
            }
        }
    }
}

create_paired_comms!(
    [handle ;  ThreadHandleComms ; (new_work, Vec<FoundTasks>) ] <->
    [thread ; ThreadComms ; (status, Status), (read_results, WorkResult) ]
);

struct Thread {
    id: usize,
    disks: Arc<Disks>,
    comms: ThreadComms,
    printer: Printer,
    status: Status,
    metrics: ThreadMetrics,
    max_dir_size: usize,
    shared_from_others: Vec<usize>,
    in_flight: usize,
}

struct ThreadHandle {
    comms: ThreadHandleComms,
    status: Status,
    worker_thread: JoinHandle<()>,
    in_flight: usize,
    new_dirs_processed: usize,
    avg_info_bundle: AvgInfoBundle,
}

struct Executor {
    verbose: bool,
    print_receiver: Option<Receiver<Vec<String>>>,
    handles: Vec<ThreadHandle>,
    max_dir_size: usize,
    last_status_print: Option<Instant>,
    start_time: Instant,
    processed: usize,
    orders_submitted: usize,
    is_finished: bool,
    registry: Arc<Disks>,
}

macro_rules! thread_print {
    ($self:ident, $str_lit:literal$(, $($args:tt)+)?) => {
        $self.printer.push(|| format!("{}: {}", $self.id, format!($str_lit$(, $($args)+)?)));
    };
}

macro_rules! main_print {
    ($verbose:expr, $str_lit:literal$(, $($args:tt)+)?) => {
        if $verbose {
            println!("main: {}", format!($str_lit$(, $($args)+)?));
        }
    }
}

macro_rules! printer_print {
    ($printer:ident, $str_lit:literal$(, $($args:tt)+)?) => {
        $printer.push(|| format!("{}", format!($str_lit$(, $($args)+)?)));
    };
}

impl Thread {
    fn new(
        id: usize,
        max_history: usize,
        disks: Arc<Disks>,
        comms: ThreadComms,
        print_sender: Option<Sender<Vec<String>>>,
    ) -> Self {
        comms.status_sender.send(Status::Idle).unwrap();
        let mut printer = Printer::new(print_sender);
        Self {
            id,
            comms,
            printer,
            status: Status::Idle,
            metrics: ThreadMetrics::new(max_history),
            max_dir_size: 0,
            shared_from_others: vec![],
            in_flight: 0,
            disks,
        }
    }

    fn send_status(&self) -> Result<(), WorkError> {
        self.comms.status_sender.send(self.status)?;
        Ok(())
    }

    fn change_status(&mut self, new_status: Status) -> Result<(), WorkError> {
        self.status = new_status;
        self.send_status()
    }

    fn finished_work(&mut self, tasks: FoundTasks) {
        self.metrics.began_post_order(tasks.len());
    }

    fn process_tasks(mut self, tasks: TaskPacket) -> WorkResult {
        let mut errors = Vec::new();
        let read_results = tasks.disk_read();
        let mut new_tasks = Vec::new();
        read_results
            .into_iter()
            .filter_map(|dir_entry| match dir_entry {
                Ok(dir_entry) => match dir_entry.metadata() {
                    Ok(metadata) => metadata.is_dir().then_some(Ok(dir_entry.path())),
                    Err(err) => Err(ErrorDir::new(dir_entry.path(), err)).into(),
                },
                Err((path, err)) => Err(ErrorDir::new(path, err)).into(),
            })
            .for_each(|res| match res {
                Ok(path) => new_tasks.push(path),
                Err(err) => errors.push(err),
            });
        let work_result = WorkResult::summarize(&errors, &new_tasks);
        self.record_results(errors, new_tasks);
        work_result
    }

    fn start(mut self) {
        let _start_timer = self
            .metrics
            .begin_event(run_info::HistoryEvent::ThreadStart);
        loop {
            unimplemented!()
        }
    }
}

impl ThreadHandle {
    fn new(
        id: usize,
        max_history: usize,
        registry: Arc<Disks>,
        print_sender: Option<Sender<Vec<String>>>,
    ) -> Self {
        let (handle_comms, process_thread_comms) = new_handle_to_thread_comms();
        let worker: Thread = Thread::new(
            id,
            max_history,
            registry,
            process_thread_comms,
            print_sender,
        );

        ThreadHandle {
            comms: handle_comms,
            status: Status::Idle,
            worker_thread: thread::spawn(move || worker.start()),
            in_flight: 0,
            new_dirs_processed: 0,
            avg_info_bundle: AvgInfoBundle::default(),
        }
    }

    fn get_avg_info(&self) -> AvgInfoBundle {
        self.avg_info_bundle
    }

    fn update_dirs_processed(&mut self, newly_processed: usize) {
        unimplemented!()
    }

    fn in_flight(&self) -> usize {
        self.in_flight
    }

    fn update_status(&mut self) {
        self.status = self
            .comms
            .status_receiver
            .try_iter()
            .last()
            .unwrap_or(self.status);
    }

    fn is_idle(&mut self) -> bool {
        self.update_status();
        self.status == Status::Idle
    }

    fn finish(self) {
        self.worker_thread.join().unwrap();
    }
}

enum RedistributeResult {
    SurplusRequestsSent,
    SurplusesDistributed,
    NoDistributionRequired,
    NoPathsInFlight,
}

impl Executor {
    fn new(mut seed: Vec<PathBuf>, max_history: usize, verbosity: Verbosity) -> Self {
        main_print!(verbosity.main, "{}", "Creating new executor.");
        let registry = Arc::new(Disks::new(seed.clone()));
        let (print_sender, print_receiver) = if verbosity.thread {
            println!("creating thread printer");
            let (print_sender, print_receiver) = bounded(100);
            (Some(print_sender), Some(print_receiver))
        } else {
            (None, None)
        };
        let handles = (0..NUM_THREADS)
            .map(|id| ThreadHandle::new(id, max_history, registry.clone(), print_sender.clone()))
            .collect();

        Self {
            verbose: verbosity.main,
            print_receiver,
            handles,
            max_dir_size: 0,
            last_status_print: None,
            start_time: Instant::now(),
            processed: 0,
            orders_submitted: 0,
            is_finished: false,
            registry,
        }
    }

    fn get_total_surplus(&self) -> usize {
        main_print!(self.verbose, "{}", "Getting total available surplus.");
        self.available_surplus
            .iter()
            .map(|surplus_vec| {
                surplus_vec
                    .iter()
                    .map(|slice: &FoundTasks| slice.len())
                    .sum::<usize>()
            })
            .sum()
    }

    fn update_available_surplus(&mut self) {
        main_print!(self.verbose, "{}", "Checking for surplus from threads.");
        self.handles
            .iter()
            .zip(self.available_surplus.iter_mut())
            .for_each(|(h, surplus_slot)| {
                surplus_slot.extend(h.comms.surplus_fulfill_receiver.try_iter());
            });
    }

    fn get_surplus_of_size(&mut self, size: usize) -> Vec<FoundTasks> {
        main_print!(self.verbose, "{}", "Carving out suprlus of size: {size}");
        let mut unfulfilled = size;
        let mut result = vec![];
        self.available_surplus = self
            .available_surplus
            .drain(0..)
            .map(|mut surplus_from_thread| {
                surplus_from_thread = surplus_from_thread
                    .into_iter()
                    .filter_map(|surplus| {
                        let (will_give, remaining) = if unfulfilled < surplus.len() {
                            surplus.split(unfulfilled)
                        } else {
                            (surplus, None)
                        };
                        unfulfilled -= will_give.len();
                        result.push(will_give);
                        remaining
                    })
                    .collect::<Vec<FoundTasks>>();
                surplus_from_thread
            })
            .collect();
        result
    }

    fn update_loop_sleep_time(&mut self) {
        main_print!(self.verbose, "{}", "Updating loop sleep time.");
        self.loop_sleep_time = DEFAULT_EXECUTE_LOOP_SLEEP;
    }

    fn redistribute_work(&self) -> Result<RedistributeResult, WorkError> {
        Ok(RedistributeResult::NoDistributionRequired)
    }

    fn print_handle_avg_info(&self) {
        main_print!(self.verbose, "{}", "Printing handle avg_info.");
        let avg_infos = self
            .handles
            .iter()
            .map(|h| h.get_avg_info())
            .collect::<Vec<AvgInfoBundle>>();
        let info_with_summaries: AvgInfoWithSummaries = avg_infos.clone().into();

        println!(
            "processing rates:  (max: {}, min: {}, total: {})",
            info_with_summaries
                .summary_processing_rates
                .max
                .num_display(),
            info_with_summaries
                .summary_processing_rates
                .min
                .num_display(),
            info_with_summaries
                .summary_processing_rates
                .total
                .num_display(),
        );

        println!(
            "in flight: {:?}",
            self.handles
                .iter()
                .map(|h| h.in_flight())
                .collect::<Vec<usize>>()
        );

        println!(
            "task times:  (max: {}, min: {}, total:{})",
            info_with_summaries.summary_task_times.max.num_display(),
            info_with_summaries.summary_task_times.min.num_display(),
            info_with_summaries.summary_task_times.total.num_display(),
        );

        println!(
            "idle times:  (max: {}, min: {}, total: {})",
            info_with_summaries.summary_idle_times.max.num_display(),
            info_with_summaries.summary_idle_times.min.num_display(),
            info_with_summaries.summary_idle_times.total.num_display(),
        );

        for (ix, avg_info_bundle) in avg_infos.iter().enumerate() {
            println!("{ix}: {avg_info_bundle}");
        }
    }

    fn print_status(&mut self) {
        main_print!(self.verbose, "{}", "Printing status.");
        if self
            .last_status_print
            .map(|t| (Instant::now() - t) > UPDATE_PRINT_DELAY)
            .unwrap_or(true)
        {
            let now = Instant::now();
            let run_time = self.start_time.elapsed();
            let minutes = run_time.as_secs() / 60;
            let seconds = run_time.as_secs() % 60;

            self.last_status_print = now.into();
            let processing_rate = ((self.processed as f64) / run_time.as_secs_f64()).round();
            println!(
                "{} directories visited. {}/{} idle. Loop wait time: {}, Running for: {}:{}. Overall rate: {}. Expected remaining: {:?}",
                self.processed,
                self.handles.iter_mut().filter_map(|p| p.is_idle().then_some(1)).sum::<usize>(),
                self.handles.len(),
                self.loop_sleep_time.custom_display(),
                minutes,
                seconds,
                processing_rate,
                Duration::from_secs_f64((1.0 / processing_rate) * (1620723723 - self.processed) as f64)
            );

            self.print_handle_avg_info();

            println!(
                "sleep: {}",
                self.loop_sleep_time_history
                    .iter()
                    .map(|d| format!("{}", d.custom_display()))
                    .collect::<Vec<String>>()
                    .join(", ")
            );

            self.orders_submitted = 0;
        }
        main_print!(self.verbose, "{}", "Finished printing status.");
    }

    fn handle_print_requests(&self) {
        main_print!(self.verbose, "{}", "Handling print requests from thread.");
        if let Some(print_receiver) = self.print_receiver.as_ref() {
            match print_receiver.try_recv() {
                Ok(print_requests) => {
                    for print_request in print_requests {
                        println!("{print_request}");
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    panic!("{:?}", WorkError::PrintSenderDisconnected)
                }
            }
        }
    }

    fn process_results(&mut self) {
        main_print!(self.verbose, "{}", "Processing results.");
        for (max_dir_size, new_dirs_processed) in self.handles.iter_mut().filter_map(|p| {
            p.drain_results()
                .map(|max_dir_size| (max_dir_size, p.new_dirs_processed))
        }) {
            // println!("Got some new work!");
            if max_dir_size > self.max_dir_size {
                self.max_dir_size = max_dir_size;
                println!("Found a directory with {} entries.", self.max_dir_size);
            }
            self.processed += new_dirs_processed;
        }
    }

    fn execute(mut self) -> Result<usize, WorkError> {
        main_print!(self.verbose, "{}", "Starting execute loop!");
        self.start_time = Instant::now();
        // Initial short sleep to ensure everyone is initialized and ready;
        // TODO: replace this with a status check on each handle
        sleep(Duration::from_millis(1));
        loop {
            self.handle_print_requests();

            self.process_results();
            match self.redistribute_work()? {
                RedistributeResult::SurplusRequestsSent => {}
                RedistributeResult::SurplusesDistributed => {}
                RedistributeResult::NoDistributionRequired => {}
                RedistributeResult::NoPathsInFlight => {
                    self.is_finished = self.handles.iter_mut().all(|h| {
                        h.update_status();
                        h.is_idle()
                    });
                }
            }
            self.print_status();

            if self.is_finished {
                let run_time = self.start_time.elapsed();
                let minutes = run_time.as_secs() / 60;
                let seconds = run_time.as_secs() % 60;
                println!(
                    "Done! {} directories visited. Ran for: {}:{}",
                    self.processed, minutes, seconds,
                );
                break;
            } else {
                self.update_loop_sleep_time();
                sleep(self.loop_sleep_time);
            }
        }
        main_print!(self.verbose, "{}", "Joining all threads.");
        for worker in self.handles.into_iter() {
            worker.finish()
        }
        Ok(self.max_dir_size)
    }
}

struct Verbosity {
    main: bool,
    thread: bool,
}

fn main() {
    let start = Instant::now();
    let manager = Executor::new(
        vec!["C:\\".into(), "A:\\".into(), "B:\\".into()],
        MAX_HISTORY,
        Verbosity {
            main: false,
            thread: false,
        },
    );
    let result = manager.execute().unwrap();
    println!("Final max dir entry count: {}", result);
    println!("Took {}.", start.elapsed().num_display());
}
