// #![cfg(feature = "debug_macros")]
#![feature(trace_macros)]
trace_macros!(false);

#[macro_use]
mod num_macros;
mod adp_num;
mod display;
mod hist_defs;
mod hist_num;
mod history;
mod num;
mod num_check;
mod num_conv;
mod sig_figs;
mod signed_num;

use hist_defs::{ProcessingRate, TimeSpan};
use hist_num::{HistData, HistoryNum};
use history::{AvgInfoBundle, HistoryVec};
use num_conv::{IntoNum, TryIntoNum};
use paste::paste;
use std::cmp::Ordering;
use std::fmt::Debug;
use std::fs::{read_dir, DirEntry};
use std::io::Error as IoErr;
use std::path::PathBuf;
use std::sync::mpsc::{self, TryRecvError};
use std::sync::mpsc::{Receiver, SendError, Sender};
use std::sync::{Arc, RwLock, RwLockReadGuard};
use std::thread::JoinHandle;
use std::thread::{self, sleep};
use std::time::{Duration, Instant};

use crate::display::CustomDisplay;
use crate::history::AvgInfoWithSummaries;

pub const NUM_THREADS: usize = 1;
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
                $(let ([< $fid _sender>], [< $fid _receiver>]) = mpsc::channel();)+
                $(let ([< $gid _sender>], [< $gid _receiver>]) = mpsc::channel();)+

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

#[derive(Debug)]
enum WorkError {
    StatusRequestSendError,
    PrintSenderDisconnected,
}

impl<T> From<SendError<T>> for WorkError {
    fn from(_: SendError<T>) -> Self {
        Self::StatusRequestSendError
    }
}

#[derive(Clone, Debug, Default)]
struct Timer {
    start: Option<Instant>,
    history: HistoryVec<TimeSpan, MAX_HISTORY>,
    total: Duration,
}

impl Timer {
    #[inline]
    fn begin(&mut self) {
        self.start.replace(Instant::now());
    }

    fn end(&mut self) -> Option<Duration> {
        self.start.take().map(|instant| {
            let elapsed = instant.elapsed();
            self.total += elapsed;
            self.history.push(elapsed);
            elapsed
        })
    }

    #[inline]
    fn last(&self) -> Option<TimeSpan> {
        self.history.last()
    }
}

#[derive(Clone, Debug)]
struct WorkResults {
    avg_t_order: TimeSpan,
    avg_t_idle: TimeSpan,
    avg_processing_rate: ProcessingRate,
    newly_processed: usize,
    max_dir_size: usize,
    in_flight: usize,
}

impl Default for WorkResults {
    fn default() -> Self {
        WorkResults {
            avg_t_order: TimeSpan::default(),
            avg_t_idle: TimeSpan::default(),
            avg_processing_rate: 0.0.into_num(),
            newly_processed: 0,
            in_flight: 0,
            max_dir_size: 0,
        }
    }
}

impl WorkResults {
    fn merge(mut self, next: WorkResults) -> Self {
        let WorkResults {
            avg_t_order: avg_task_time,
            avg_t_idle: avg_idle_time,
            avg_processing_rate,
            newly_processed: dirs_processed,
            max_dir_size,
            in_flight,
        } = next;
        self.avg_t_order = avg_task_time;
        self.avg_t_idle = avg_idle_time;
        self.avg_processing_rate = avg_processing_rate;
        self.newly_processed = self.newly_processed + dirs_processed;
        self.max_dir_size = self.max_dir_size.max(max_dir_size);
        self.in_flight += in_flight;
        self
    }
}

impl std::iter::Sum for WorkResults {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.reduce(|a, b| a.merge(b)).unwrap_or_default()
    }
}

#[derive(Default)]
struct ThreadHistory<const MAX_HISTORY: usize> {
    dirs_processed: HistoryVec<HistData<usize, isize>, MAX_HISTORY>,
    processing_rates: HistoryVec<ProcessingRate, MAX_HISTORY>,
    work_request_sizes: HistoryVec<HistData<usize, isize>, MAX_HISTORY>,
    t_order: Timer,
    t_idle: Timer,
    t_post_order: Timer,
    start_t: Timer,
    total_processed: usize,
}

// struct InfoBundle {
//     processing_rates: Option<ProcessingRate>,
//     t_order: Option<TimeSpan>,
//     t_idle: Option<TimeSpan>,
//     t_post_order: Option<TimeSpan>,
// }

impl<const MAX_HISTORY: usize> ThreadHistory<MAX_HISTORY> {
    fn began_process(&mut self) {
        self.start_t.begin();
    }

    fn began_idling(&mut self) {
        self.t_post_order.end();
        self.t_idle.begin();
    }

    fn began_order(&mut self) {
        self.t_idle.end();
        self.t_post_order.end();
        self.t_order.begin();
    }

    fn began_post_order(&mut self, newly_processed_count: usize) {
        if let Some(elapsed) = self.t_order.end() {
            self.processing_rates
                .push((newly_processed_count as f64 / elapsed.as_secs_f64()).into_num());
            self.dirs_processed.push(newly_processed_count);
            self.total_processed += newly_processed_count;
        }
        self.t_post_order.begin();
    }

    fn avg_processing_rate(&self) -> ProcessingRate {
        self.processing_rates.average
    }

    fn avg_t_idle(&self) -> TimeSpan {
        self.t_idle.history.average
    }

    fn avg_t_order(&self) -> TimeSpan {
        self.t_order.history.average
    }

    // fn avg_t_post_order(&self) -> TimeSpan {
    //     self.t_post_order.history.average
    // }

    // fn last(&self) -> InfoBundle {
    //     InfoBundle {
    //         processing_rates: self.processing_rates.last(),
    //         t_order: self.t_order.history.last(),
    //         t_idle: self.t_idle.history.last(),
    //         t_post_order: self.t_post_order.history.last(),
    //     }
    // }
}

// create_paired_comms!(
//     [handle ;  ThreadHandleComms ; (new_work, Vec<WorkSlice>), (surplus_request, usize)] <->
//     [thread ; ThreadComms ; (status, Status), (result, WorkResults), (out_of_work, usize), (surplus_fulfill, WorkSlice) ]
// );

create_paired_comms!(
    [handle ;  ThreadHandleComms ; (new_work, Vec<WorkSlice>), (surplus_request, usize)] <->
    [thread ; ThreadComms ; (status, Status), (result, WorkResults), (surplus_fulfill, WorkSlice) ]
);

type LockedPathBuf = Arc<RwLock<Vec<PathBuf>>>;

#[derive(Default, Debug, Clone)]
struct WorkBuf {
    buf: LockedPathBuf,
    pending_from: usize,
    shared_with_others: Vec<(usize, usize)>,
}

impl From<Vec<PathBuf>> for WorkBuf {
    fn from(seed_work: Vec<PathBuf>) -> Self {
        WorkBuf {
            buf: Arc::new(RwLock::new(seed_work)),
            pending_from: 0,
            shared_with_others: vec![],
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum WorkSource {
    Local,
    Shared,
}

#[derive(Clone, Debug)]
struct WorkSlice {
    source: WorkSource,
    buf: LockedPathBuf,
    start: usize,
    end: usize,
    cursor: usize,
}

impl WorkSlice {
    fn new(source: WorkSource, buf: LockedPathBuf, start: usize, end: usize) -> WorkSlice {
        WorkSlice {
            source,
            buf,
            start,
            end,
            cursor: start,
        }
    }

    fn buf(&self) -> RwLockReadGuard<'_, Vec<PathBuf>> {
        self.buf.read().unwrap()
    }

    fn len(&self) -> usize {
        self.end - self.start
    }

    fn split(self, split_size: usize) -> (WorkSlice, Option<WorkSlice>) {
        if split_size > self.len() {
            (self, None)
        } else {
            (
                WorkSlice::new(
                    self.source,
                    self.buf.clone(),
                    self.start,
                    self.start + split_size,
                ),
                Some(WorkSlice::new(
                    self.source,
                    self.buf.clone(),
                    self.start + split_size,
                    self.end,
                )),
            )
        }
    }
}

#[allow(unused)]
// TODO: handle error dirs
struct ErrorDir {
    err: IoErr,
    path: PathBuf,
}

impl ErrorDir {
    fn new(path: PathBuf, err: IoErr) -> Self {
        Self { err, path }
    }
}

type DirEntryResult = Result<DirEntry, ErrorDir>;
type WorkSliceIterItem = Result<Vec<DirEntryResult>, ErrorDir>;

impl Iterator for WorkSlice {
    type Item = WorkSliceIterItem;

    fn next(&mut self) -> Option<WorkSliceIterItem> {
        if self.start <= self.cursor && self.cursor < self.end {
            let result = {
                let path = &self.buf()[self.cursor];
                read_dir(path)
                    .map(|read_iter| {
                        read_iter
                            .map(|dir_entry_result| {
                                dir_entry_result.map_err(|err| ErrorDir::new(path.clone(), err))
                            })
                            .collect::<Vec<DirEntryResult>>()
                    })
                    .map_err(|err| ErrorDir::new(path.clone(), err))
            };
            self.cursor += 1;
            Some(result)
        } else {
            None
        }
    }
}

impl WorkBuf {
    #[inline]
    fn len(&self) -> usize {
        self.buf.read().unwrap().len()
    }

    #[inline]
    fn empty_pending(&self) -> bool {
        self.pending_from == self.len()
    }

    #[inline]
    fn next_pending_from(&self, size: usize) -> usize {
        (self.pending_from + size).min(self.len())
    }

    #[inline]
    fn get_work_slice(&mut self, source: WorkSource, size: usize) -> WorkSlice {
        let start = self.pending_from;
        self.pending_from = self.next_pending_from(size);
        WorkSlice::new(source, self.buf.clone(), start, self.pending_from)
    }

    #[inline]
    fn get_work_for_local(&mut self, size: usize) -> WorkSlice {
        self.get_work_slice(WorkSource::Local, size)
    }

    #[inline]
    fn get_work_for_sharing(&mut self, size: usize) -> WorkSlice {
        let slice = self.get_work_slice(WorkSource::Shared, size);
        self.shared_with_others.push((slice.start, slice.end));
        slice
    }

    fn total_pending(&self) -> usize {
        self.len() - self.pending_from
    }

    fn extend(&self, paths: impl Iterator<Item = PathBuf>) {
        // println!("attempting to extend workbuf!");
        self.buf.write().unwrap().extend(paths);
        // println!("finished extending workbuf!");
    }
}

struct Thread<const MAX_HISTORY: usize> {
    id: usize,
    comms: ThreadComms,
    printer: Printer,
    status: Status,
    history: ThreadHistory<MAX_HISTORY>,
    max_dir_size: usize,
    shared_from_others: Vec<usize>,
    work_buf: WorkBuf,
    errored: Vec<ErrorDir>,
}

struct ThreadHandle {
    comms: ThreadHandleComms,
    status: Status,
    worker_thread: JoinHandle<()>,
    in_flight: usize,
    new_dirs_processed: usize,
    avg_info_bundle: AvgInfoBundle,
    // work_request_size: usize,
    // shared_work: Vec<PathBuf>,
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
    loop_sleep_time: Duration,
    loop_sleep_time_history: HistoryVec<TimeSpan, MAX_HISTORY>,
    is_finished: bool,
    // unfulfilled_requests: [usize; NUM_THREADS],
    available_surplus: Vec<Vec<WorkSlice>>,
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

impl<const MAX_HISTORY: usize> Thread<MAX_HISTORY> {
    fn new(
        id: usize,
        seed_work: Vec<PathBuf>,
        comms: ThreadComms,
        print_sender: Option<Sender<Vec<String>>>,
    ) -> Self {
        comms.status_sender.send(Status::Idle).unwrap();
        let mut printer = Printer::new(print_sender);
        printer_print!(printer, "{}: Beginning with seed work: {:?}", id, seed_work);
        Self {
            id,
            comms,
            printer,
            status: Status::Idle,
            history: ThreadHistory::default(),
            max_dir_size: 0,
            work_buf: WorkBuf::from(seed_work),
            shared_from_others: vec![],
            errored: vec![],
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

    fn get_local_work(&mut self) -> WorkSlice {
        let work = self.work_buf.get_work_for_local(DEFAULT_WORK_CHUNK_SIZE);
        self.history.began_order();
        work
    }

    fn get_work(&mut self) -> Vec<WorkSlice> {
        if self.work_buf.empty_pending() {
            self.get_shared_work()
        } else {
            vec![self.get_local_work()]
        }
    }

    fn finished_work(&mut self, work_slice: WorkSlice) {
        self.history.began_post_order(work_slice.len());
    }

    fn get_share_request_size(&self) -> usize {
        let work_request_size = ((self.history.avg_processing_rate().adaptor()
            * self.history.avg_t_order().adaptor())
        .round() as usize)
            .max(1);
        work_request_size
    }

    fn get_shared_work(&mut self) -> Vec<WorkSlice> {
        self.history.began_idling();
        thread_print!(self, "Began idling!");
        self.comms.status_sender.send(Status::Idle).unwrap();
        let request_size = self.get_share_request_size();
        thread_print!(self, "share request size: {}", request_size);
        self.history.work_request_sizes.push(request_size);
        // self.comms.out_of_work_sender.send(request_size).unwrap();
        let shared_work = self.comms.new_work_receiver.recv().unwrap();
        self.shared_from_others.push(shared_work.len());
        thread_print!(
            self,
            "Done idling, took: {:?}",
            self.history.t_idle.last().map(|t| t.absolute())
        );
        shared_work
    }

    fn share_work(&mut self, surplus_request: usize) -> Result<(), WorkError> {
        let shared_work_slice = self.work_buf.get_work_for_sharing(
            surplus_request.min(self.work_buf.total_pending()), // if surplus_request > 0 { 1 } else { 0 },
        );
        thread_print!(
            self,
            "sending work for sharing: {:?}",
            shared_work_slice.len()
        );
        if shared_work_slice.len() > 0 {
            self.comms.surplus_fulfill_sender.send(shared_work_slice)?;
        }
        Ok(())
    }

    fn in_flight(&self) -> usize {
        self.work_buf.total_pending()
    }

    fn start(mut self) {
        self.history.began_process();
        loop {
            self.printer.flush_send().unwrap();

            let work_slices = self.get_work();
            self.history.began_order();
            self.change_status(Status::Busy).unwrap();

            let mut newly_processed = 0;
            // println!("Non printer: beginning task...");
            for mut work_slice in work_slices {
                // println!("Non printer: beginning inner loop");
                'inner: loop {
                    if let Some(dir_entries) = work_slice.next() {
                        if let Err(err_dir) = dir_entries.map(|entry_vec| {
                            if entry_vec.len() > 0 {
                                let mut this_dir_size = 0;
                                self.work_buf.extend(entry_vec.into_iter().filter_map(
                                    |dir_entry_result| {
                                        dir_entry_result
                                            .map(|entry| {
                                                // dir_entry.metadata().ok().and_then(|meta| meta.is_dir().then_some(dir_entry.path()))
                                                match entry.metadata() {
                                                    Ok(meta) => meta.is_dir().then_some({
                                                        this_dir_size += 1;
                                                        entry.path()
                                                    }),
                                                    Err(err) => {
                                                        self.errored.push(ErrorDir {
                                                            err,
                                                            path: entry.path(),
                                                        });
                                                        None
                                                    }
                                                }
                                            })
                                            .map_err(|err_dir| self.errored.push(err_dir))
                                            .ok()
                                            .flatten()
                                    },
                                ));
                                self.max_dir_size = self.max_dir_size.max(this_dir_size);
                            }
                        }) {
                            self.errored.push(err_dir)
                        }
                    } else {
                        break 'inner;
                    }
                }
                // println!("Non printer: finished inner loop");
                newly_processed += work_slice.len();
                self.finished_work(work_slice);
            }
            // println!("Non printer: finished task...");
            self.history.began_post_order(newly_processed);
            if let Some(surplus_request) = self.comms.surplus_request_receiver.try_iter().last() {
                thread_print!(
                    self,
                    "found something in the surplus requests: {surplus_request}"
                );
                self.share_work(surplus_request).unwrap();
            }
            self.comms
                .result_sender
                .send(WorkResults {
                    avg_t_order: self.history.avg_t_order(),
                    avg_t_idle: self.history.avg_t_idle(),
                    max_dir_size: self.max_dir_size,
                    avg_processing_rate: self.history.avg_processing_rate(),
                    newly_processed,
                    in_flight: self.in_flight(),
                })
                .unwrap();
        }
    }
}

impl ThreadHandle {
    fn new(id: usize, seed_work: Vec<PathBuf>, print_sender: Option<Sender<Vec<String>>>) -> Self {
        let (handle_comms, process_thread_comms) = new_handle_to_thread_comms();
        let worker: Thread<MAX_HISTORY> =
            Thread::new(id, seed_work, process_thread_comms, print_sender);

        ThreadHandle {
            comms: handle_comms,
            status: Status::Idle,
            worker_thread: thread::spawn(move || worker.start()),
            in_flight: 0,
            new_dirs_processed: 0,
            avg_info_bundle: AvgInfoBundle::default(),
        }
    }

    // fn get_avg_task_time(&self) -> Duration {
    //     self.avg_info_bundle.task_time.data
    // }

    // fn get_avg_idle_time(&self) -> Duration {
    //     self.avg_info_bundle
    //         .idle_time
    //         .data
    //         .try_adaptor_as_absolute()
    //         .unwrap()
    // }

    // fn get_avg_processing_rate(&self) -> f64 {
    //     self.avg_info_bundle.processing_rate.data
    // }

    fn get_avg_info(&self) -> AvgInfoBundle {
        self.avg_info_bundle
    }

    // fn drain_orders(&mut self) -> Vec<PathBuf> {
    //     let order: Vec<PathBuf> = self.orders.drain(0..).collect();
    //     self.in_flight += order.len();
    //     order
    // }

    // fn queue_orders(&mut self, orders: Vec<PathBuf>) {
    //     self.orders.extend(orders.into_iter());
    // }

    fn dispatch_surplus(&self, surplus: Vec<WorkSlice>) -> Result<(), WorkError> {
        // if self.is_idle() {
        //     self.in_flight += orders.len();
        //     self.comms.order_sender.send(orders)?;
        //     let drained_orders = self.drain_orders();
        //     self.comms.order_sender.send(drained_orders)?;
        // } else {
        //     self.queue_orders(orders);
        // }
        self.comms.new_work_sender.send(surplus)?;
        // let drained_orders = self.drain_orders();
        // self.comms.order_sender.send(drained_orders)?;
        Ok(())
    }

    fn dispatch_surplus_request(&self, request_size: usize) -> Result<(), WorkError> {
        // println!("sending surplus request of size: {request_size}");
        self.comms.surplus_request_sender.send(request_size)?;
        Ok(())
    }

    // fn push_orders(&mut self, orders: Vec<PathBuf>) -> Result<(), WorkError> {
    //     self.dispatch_orders(orders)?;
    //     // if self.is_idle() {
    //     //     self.dispatch_orders(orders)?;
    //     // } else {
    //     //     self.queue_orders(orders);
    //     // }
    //     Ok(())
    // }

    fn update_dirs_processed(&mut self, newly_processed: usize) {
        self.new_dirs_processed += newly_processed;
        if self.in_flight < newly_processed {
            self.in_flight = 0;
        } else {
            self.in_flight -= newly_processed;
        }
    }

    fn drain_results(&mut self) -> Option<usize> {
        let results: Vec<WorkResults> = self.comms.result_receiver.try_iter().collect();
        if results.len() > 0 {
            let WorkResults {
                avg_t_order: avg_task_time,
                avg_t_idle: avg_idle_time,
                avg_processing_rate,
                newly_processed,
                max_dir_size,
                in_flight,
            } = results.into_iter().sum();
            self.avg_info_bundle
                .update(avg_processing_rate, avg_task_time, avg_idle_time);
            self.in_flight = in_flight;
            self.update_dirs_processed(newly_processed);
            Some(max_dir_size)
        } else {
            None
        }
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

// fn task_time_score(avg_task: f64, max_avg_task_time: f64) -> f64 {
//     1.0 - (avg_task / max_avg_task_time)
// }

// fn idle_time_score(avg_idle: f64, max_avg_idle_time: f64) -> f64 {
//     avg_idle / max_avg_idle_time
// }

// // fn in_flight_penalty(in_flight: usize) -> f64 {
// //     (1.0 - (in_flight as f64 / MAX_IN_FLIGHT as f64))
// //         .min(0.0)
// //         .max(1.0)
// // }

// fn processing_rate_score(processing_rate: f64, max_processing_rate: f64) -> f64 {
//     processing_rate / max_processing_rate
// }

enum RedistributeResult {
    SurplusRequestsSent,
    SurplusesDistributed,
    NoDistributionRequired,
    #[allow(unused)]
    NoPathsInFlight,
}

impl Executor {
    fn new(mut seed: Vec<PathBuf>, verbose: bool) -> Self {
        main_print!(verbose, "{}", "Creating new executor.");
        let (print_sender, print_receiver) = if verbose {
            let (print_sender, print_receiver) = mpsc::channel();
            (Some(print_sender), Some(print_receiver))
        } else {
            (None, None)
        };

        let seed_work_size = ((seed.len() as f64 / NUM_THREADS as f64).round() as usize).max(1);
        let handles = (0..NUM_THREADS)
            .map(|id| {
                let seed_work = seed
                    .drain(0..seed_work_size.min(seed.len()))
                    .collect::<Vec<PathBuf>>();
                ThreadHandle::new(id, seed_work, print_sender.clone())
            })
            .collect();

        Self {
            verbose,
            print_receiver,
            handles,
            max_dir_size: 0,
            last_status_print: None,
            start_time: Instant::now(),
            processed: 0,
            orders_submitted: 0,
            loop_sleep_time: DEFAULT_EXECUTE_LOOP_SLEEP,
            loop_sleep_time_history: HistoryVec::default(),
            is_finished: false,
            // unfulfilled_requests: [0; NUM_THREADS],
            available_surplus: vec![Vec::default(); NUM_THREADS],
        }
    }

    // fn fetch_stats<T>(&self, f: fn(&ThreadHandle) -> T) -> Vec<T>
    // where
    //     T: PartialOrd + Copy + Default,
    // {
    //     self.handles.iter().map(f).collect()
    // }

    // fn update_unfulfilled_requests(&mut self) {
    //     self.handles
    //         .iter()
    //         .zip(self.unfulfilled_requests.iter_mut())
    //         .for_each(|(h, slot)| {
    //             *slot = *slot + h.comms.out_of_work_receiver.try_iter().last().unwrap_or(0);
    //         });
    // }

    fn get_total_surplus(&self) -> usize {
        main_print!(self.verbose, "{}", "Getting total available surplus.");
        self.available_surplus
            .iter()
            .map(|surplus_vec| {
                surplus_vec
                    .iter()
                    .map(|slice: &WorkSlice| slice.len())
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
                // let initial_slot_len = surplus_slot.len();
                surplus_slot.extend(h.comms.surplus_fulfill_receiver.try_iter());
                // let final_slot_len = surplus_slot.len();
                // if unfulfilled > 0 && surplus_slot.len() > 0 {
                //     panic!("surplus slot is non-empty, but unfulfilled request also exists!")
                // }
            });
    }

    fn get_surplus_of_size(&mut self, size: usize) -> Vec<WorkSlice> {
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
                    .collect::<Vec<WorkSlice>>();
                surplus_from_thread
            })
            .collect();
        result
    }

    #[allow(unused)]
    fn send_surplus_requests(
        &self,
        total_required: usize,
    ) -> Result<RedistributeResult, WorkError> {
        main_print!(
            self.verbose,
            "{}",
            "Sending out surplus request of total: {total_required}"
        );
        let in_flights: Vec<usize> = self.handles.iter().map(|h| h.in_flight()).collect();
        // println!("in flights: {in_flights:?}");
        let max_in_flight: usize = in_flights.iter().max().copied().unwrap_or(0);
        // println!("max in flight: {max_in_flight:?}");
        if max_in_flight > 0 && total_required > 0 {
            let mut remaining = total_required;
            let mut handle_info: Vec<(usize, usize, f64)> = self
                .handles
                .iter()
                .enumerate()
                .zip(in_flights.into_iter())
                .map(|((ix, h), in_flight)| {
                    (
                        ix,
                        in_flight,
                        h.get_avg_info().processing_rate.data.adaptor(),
                    )
                })
                .collect();
            handle_info.sort_unstable_by(|p, q| {
                (p.1 as f64 / p.2)
                    .partial_cmp(&(q.1 as f64 / q.2))
                    .unwrap_or(Ordering::Less)
            });
            // println!("sorted handles with pr: {handle_info:?}");
            for (ix, in_flight, _) in handle_info.into_iter() {
                if remaining == 0 {
                    break;
                } else {
                    if in_flight > 0 {
                        let request_size = (((in_flight as f64 / max_in_flight as f64)
                            * total_required as f64)
                            .round() as usize)
                            .min(remaining);
                        self.handles[ix].dispatch_surplus_request(request_size)?;
                        remaining -= request_size;
                    }
                }
            }
            Ok(RedistributeResult::SurplusRequestsSent)
        } else {
            Ok(RedistributeResult::NoPathsInFlight)
        }
    }

    fn update_loop_sleep_time(&mut self) {
        main_print!(self.verbose, "{}", "Updating loop sleep time.");
        // let idle_times = self.fetch_stats(|h| h.get_avg_idle_time());
        // let min_idle_time = idle_times
        //     .iter()
        //     .copied()
        //     .min()
        //     .unwrap_or(DEFAULT_EXECUTE_LOOP_SLEEP);
        // self.loop_sleep_time = (min_idle_time / NUM_THREADS as u32).min(DEFAULT_EXECUTE_LOOP_SLEEP);
        // self.loop_sleep_time_history.push(self.loop_sleep_time);
        self.loop_sleep_time = DEFAULT_EXECUTE_LOOP_SLEEP;
    }

    // fn redistribute_work(&mut self) -> Result<RedistributeResult, WorkError> {
    //     // println!("initial unfulfilled: {:?}", self.unfulfilled_requests);
    //     self.update_unfulfilled_requests();
    //     // println!("updated unfulfilled: {:?}", self.unfulfilled_requests);
    //     let unfulfilled_total = self.unfulfilled_requests.iter().sum();
    //     if unfulfilled_total > 0 {
    //         self.update_available_surplus();
    //         let initial_surplus_total = self.surplus_total();
    //         // println!("available surplus: {:?}", initial_surplus_total);
    //         if initial_surplus_total == 0 {
    //             // println!("no surplus found, sending another request");
    //             return self.send_surplus_requests(unfulfilled_total);
    //         } else {
    //             // println!("dispatching surplus");
    //             let mut current_surplus_total = initial_surplus_total;
    //             let will_takes = self
    //                 .unfulfilled_requests
    //                 .iter_mut()
    //                 .enumerate()
    //                 .filter_map(|(ix, unfulfilled)| {
    //                     (*unfulfilled > 0).then_some({
    //                         let will_take = (((*unfulfilled as f64 / unfulfilled_total as f64)
    //                             * (initial_surplus_total as f64))
    //                             .round() as usize)
    //                             .min(current_surplus_total)
    //                             .min(*unfulfilled)
    //                             .max(1);
    //                         current_surplus_total -= will_take;
    //                         (ix, will_take)
    //                     })
    //                 })
    //                 .collect::<Vec<(usize, usize)>>();
    //             // println!("{will_takes:?}");
    //             for (ix, will_take) in will_takes {
    //                 let surplus = self.get_surplus_of_size(will_take);
    //                 self.unfulfilled_requests[ix] -= surplus.len();
    //                 // println!(
    //                 //     "dispatching surplus of size: {} to thread {}",
    //                 //     surplus.len(),
    //                 //     ix
    //                 // );
    //                 self.handles[ix].dispatch_surplus(surplus)?;
    //             }
    //             return Ok(RedistributeResult::SurplusesDistributed);
    //         }
    //     } else {
    //         // println!("no re-distribution was required!");
    //         Ok(RedistributeResult::NoDistributionRequired)
    //     }
    // }

    fn redistribute_work(&mut self) -> Result<RedistributeResult, WorkError> {
        main_print!(self.verbose, "{}", "Redistributing work.");
        let in_flights = self
            .handles
            .iter()
            .map(|h| h.in_flight())
            .collect::<Vec<usize>>();
        let max_in_flights = in_flights.iter().copied().max().unwrap_or(0);
        if max_in_flights > 0 {
            Ok(RedistributeResult::NoDistributionRequired)
        } else {
            self.update_available_surplus();
            let each_thread_should_have = in_flights.iter().sum::<usize>() / NUM_THREADS;
            let mut total_surplus = self.get_total_surplus();
            if total_surplus > 0 {
                for (ix, in_flight) in in_flights.into_iter().enumerate() {
                    if in_flight > each_thread_should_have {
                        // self.handles[ix]
                        //     .dispatch_surplus_request(in_flight - each_thread_should_have)?;
                    } else {
                        let surplus = self.get_surplus_of_size(each_thread_should_have);
                        total_surplus -= surplus.len().min(total_surplus);
                        self.handles[ix].dispatch_surplus(surplus)?;
                    }
                    if total_surplus == 0 {
                        break;
                    }
                }
                Ok(RedistributeResult::SurplusesDistributed)
            } else {
                for (ix, in_flight) in in_flights.into_iter().enumerate() {
                    if in_flight > each_thread_should_have {
                        self.handles[ix]
                            .dispatch_surplus_request(in_flight - each_thread_should_have)?;
                    }
                }
                Ok(RedistributeResult::SurplusRequestsSent)
            }
        }
    }

    // fn distribute_work(&mut self) -> Result<(), WorkError> {
    //     let (_, _) = self.fetch_stats_and_max(|h| h.avg_task_time.as_secs_f64(), 0.0);
    //     let (idle_times, max_idle_time) =
    //         self.fetch_stats_and_max(|h| h.avg_idle_time.as_secs_f64(), 0.0);
    //     let (processing_rates, max_processing_rate) =
    //         self.fetch_stats_and_max(|h| h.avg_processing_rate, 0.0);
    //     let currently_submitted = self.fetch_stats(|h| h.in_flight());
    //     let max_per_thread =
    //         (self.work_q.len() as f64 / self.handles.len() as f64).floor() as usize;
    //     let dispatch_sizes: Vec<usize> = if max_idle_time > 0.0 && max_processing_rate > 0.0 {
    //         // bias distribution
    //         // let ratings = task_times
    //         //     .iter()
    //         //     .zip(idle_times.iter())
    //         //     .zip(processing_rates.iter())
    //         //     .map(|((&task_time, &idle_time), _)| {
    //         //         (max_task_time / max_idle_time).min(1.0)
    //         //             * task_time_score(task_time, max_task_time)
    //         //             + idle_time_score(idle_time, max_idle_time)
    //         //     })
    //         //     .collect::<Vec<f64>>();
    //         // let normalizer: f64 = *ratings.find_max(&1.0);
    //         // self.ratings = ratings.iter().map(|r| r / normalizer).collect();
    //         // ratings
    //         //     .into_iter()
    //         //     .map(|r| (r / normalizer))
    //         //     .zip(currently_submitted.iter())
    //         //     .map(|(r, in_flight)| {
    //         //         ((r * max_per_thread as f64).round() as usize)
    //         //             .min(MAX_IN_FLIGHT - in_flight)
    //         //     })
    //         //     .collect()
    //         let requested_dispatches: Vec<usize> = processing_rates
    //             .iter()
    //             .zip(idle_times.iter())
    //             .zip(currently_submitted.iter())
    //             .map(|((&processing_rate, &idle_time), &in_flight)| {
    //                 let filled_idle_time = in_flight as f64 / processing_rate;
    //                 let unfilled_idle_time = (idle_time > filled_idle_time)
    //                     .then(|| idle_time - filled_idle_time)
    //                     .unwrap_or(0.0);
    //                 let would_like = ((processing_rate * unfilled_idle_time).round() as usize)
    //                     .max(if in_flight == 0 { 1 } else { 0 });
    //                 // let would_like = if would_like + in_flight > MAX_IN_FLIGHT {
    //                 //     (would_like + in_flight) - MAX_IN_FLIGHT
    //                 // } else {
    //                 //     would_like
    //                 // };
    //                 would_like
    //             })
    //             .collect();
    //         // println!(
    //         //     "({}, {}) requested_dispatches: {requested_dispatches:?}",
    //         //     processing_rates.len(),
    //         //     idle_times.len()
    //         // );
    //         let max_dispatch_total = self.work_q.len().min(MAX_TOTAL_DISPATCH_SIZE);
    //         if requested_dispatches.iter().sum::<usize>() > max_dispatch_total {
    //             let max_dispatch_request = requested_dispatches
    //                 .iter()
    //                 .max()
    //                 .copied()
    //                 .unwrap_or(1_usize) as f64;
    //             requested_dispatches
    //                 .into_iter()
    //                 .map(|x| {
    //                     ((x as f64 / max_dispatch_request) * max_dispatch_total as f64).round()
    //                         as usize
    //                 })
    //                 .collect()
    //         } else {
    //             requested_dispatches
    //         }
    //     } else {
    //         // distribute equally
    //         let max_per_thread = max_per_thread.max(1);
    //         vec![max_per_thread; self.handles.len()]
    //     };
    //     // println!("dispatches: {dispatch_sizes:?}");
    //     dispatch_sizes
    //         .into_iter()
    //         .zip(self.handles.iter_mut())
    //         .zip(currently_submitted.iter())
    //         .map(|((dispatch_size, handle), &_)| {
    //             let final_dispatch_size = dispatch_size.min(self.work_q.len());
    //             let orders: Vec<PathBuf> = self.work_q.drain(0..final_dispatch_size).collect();
    //             let dispatch_size = orders.len();
    //             if dispatch_size > 0 {
    //                 handle.push_orders(orders).unwrap();
    //                 self.orders_submitted += dispatch_size;
    //             }
    //             final_dispatch_size
    //         })
    //         .zip(self.total_submitted.iter_mut())
    //         .for_each(|(final_dispatch, current_total)| {
    //             *current_total = *current_total + final_dispatch
    //         });
    //     self.loop_sleep_time = Duration::from_secs_f64(
    //         idle_times
    //             .iter()
    //             .min_by(|p, q| p.partial_cmp(q).unwrap_or(Ordering::Less))
    //             .copied()
    //             .map(|d| d / (5.0 * NUM_THREADS as f64))
    //             .unwrap_or(DEFAULT_EXECUTE_LOOP_SLEEP.as_secs_f64()),
    //     );
    //     self.loop_sleep_time_history.push(self.loop_sleep_time);
    //     sleep(self.loop_sleep_time);
    //     Ok(())
    // }

    fn print_handle_avg_info(&self) {
        main_print!(self.verbose, "{}", "Printing handle avg_info.");
        let avg_infos = self
            .handles
            .iter()
            .map(|h| h.get_avg_info())
            .collect::<Vec<AvgInfoBundle>>();
        let info_with_summaries: AvgInfoWithSummaries = avg_infos.clone().into();

        println!(
            "{} (max: {}, min: {}, total: {})",
            "processing rates: ",
            info_with_summaries
                .summary_processing_rates
                .max
                .custom_display(),
            info_with_summaries
                .summary_processing_rates
                .min
                .custom_display(),
            info_with_summaries
                .summary_processing_rates
                .total
                .custom_display(),
        );

        println!(
            "in flight: {:?}",
            self.handles
                .iter()
                .map(|h| h.in_flight())
                .collect::<Vec<usize>>()
        );

        println!(
            "{} (max: {}, min: {}, total:{})",
            "task times: ",
            info_with_summaries.summary_task_times.max.custom_display(),
            info_with_summaries.summary_task_times.min.custom_display(),
            info_with_summaries
                .summary_task_times
                .total
                .custom_display(),
        );

        println!(
            "{} (max: {}, min: {}, total: {})",
            "idle times: ",
            info_with_summaries.summary_idle_times.max.custom_display(),
            info_with_summaries.summary_idle_times.min.custom_display(),
            info_with_summaries
                .summary_idle_times
                .total
                .custom_display(),
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
            println!(
                "{} directories visited. {}/{} idle. Loop wait time: {}, Running for: {}:{}. Overall rate: {}",
                self.processed,
                self.handles.iter_mut().filter_map(|p| p.is_idle().then_some(1)).sum::<usize>(),
                self.handles.len(),
                self.loop_sleep_time.custom_display(),
                minutes,
                seconds,
                ((self.processed as f64) / run_time.as_secs_f64()).round()
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
                Err(TryRecvError::Disconnected) => Err(WorkError::PrintSenderDisconnected).unwrap(),
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

// fn map_io_error(path: &Path, io_err: IoErr) -> String {
//     format!("Could not open path: {path:?} due to error {io_err}")
// }

fn main() {
    let start = Instant::now();
    let manager = Executor::new(vec!["C:\\".into(), "A:\\".into(), "B:\\".into()], true);
    let result = manager.execute().unwrap();
    println!("Final max dir entry count: {}", result);
    println!("Took {}.", start.elapsed().custom_display());
}
