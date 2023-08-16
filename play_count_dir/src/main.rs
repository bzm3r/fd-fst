// #![cfg(feature = "debug_macros")]
// #![feature(trace_macros)]
// trace_macros!(true);

use paste::paste;
use std::cmp::Ordering;
use std::fmt::Debug;
use std::fs::{read_dir, DirEntry};
use std::io::Error as IoError;
use std::io::Result as IoResult;
use std::iter::Sum;
use std::ops::Add;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::mpsc::{Receiver, SendError, Sender};
use std::thread::JoinHandle;
use std::thread::{self, sleep};
use std::time::{Duration, Instant};

#[macro_use]
mod macro_tools;

const NUM_THREADS: usize = 3;
const DEFAULT_EXECUTE_LOOP_SLEEP: Duration = Duration::from_micros(50);
const UPDATE_PRINT_DELAY: Duration = Duration::from_secs(5);
// const MAX_TOTAL_SUBMISSION: usize = NUM_THREADS * 500;
const MAX_HISTORY: usize = 10;
// const MIN_TARGET_SIZE: usize = 1;
// const MAX_DISPATCH_SIZE: usize = 2_usize.pow(6);
const MAX_TOTAL_DISPATCH_SIZE: usize = if (NUM_THREADS * 10_usize.pow(4)) < (6 * 10_usize.pow(4)) {
    ((NUM_THREADS * 10_usize.pow(4)) as f64 * 1e-2) as usize
} else {
    ((NUM_THREADS * 10_usize.pow(4)) as f64 * 1e-2) as usize
    //((6 * 10_usize.pow(4)) as f64 * 1e-2) as usize
};
// const MAX_IN_FLIGHT: usize = MAX_DISPATCH_SIZE * 2_usize.pow(10);
// const PESSIMISTIC_PROCESSING_RATE_ESTIMATE: f64 = 1e3;

trait IntoF64 {
    fn into_f64(&self) -> Result<f64, String>;
}

trait RoundSigFigs
where
    Self: Copy + Clone,
{
    fn from_f64(f: f64) -> Result<Self, String>;
    fn into_f64(self) -> Result<f64, String>;

    fn delta(x: f64) -> Option<i32> {
        let f = x.abs().log10().ceil();
        f.is_finite().then_some(f as i32)
    }

    fn round_sig_figs(&self, n_sig_figs: i32) -> Self {
        let x: f64 = self.into_f64().unwrap();
        Self::from_f64(if x == 0. || n_sig_figs == 0 {
            0.0_f64
        } else {
            if let Some(delta) = Self::delta(x) {
                let shift = n_sig_figs - delta;
                let shift_factor = 10_f64.powi(shift);
                (x * shift_factor).round() / shift_factor
            } else {
                0.0_f64
            }
        })
        .unwrap()
    }
}

impl RoundSigFigs for f64 {
    fn from_f64(f: f64) -> Result<Self, String> {
        Ok(f)
    }

    fn into_f64(self) -> Result<f64, String> {
        Ok(self)
    }
}

impl RoundSigFigs for Duration {
    fn from_f64(f: f64) -> Result<Self, String> {
        if f.is_finite() && f.is_sign_positive() {
            Ok(Duration::from_secs_f64(f))
        } else {
            Err(format!(
                "Cannot convert {f} into a Duration (not-finite, and/or negative)"
            ))
        }
    }

    fn into_f64(self) -> Result<f64, String> {
        Ok(self.as_secs_f64())
    }
}

impl RoundSigFigs for usize {
    fn from_f64(f: f64) -> Result<Self, String> {
        if f.is_finite() && f.is_sign_positive() {
            Ok(f.round() as usize)
        } else {
            Err(format!(
                "Cannot convert {f} into a usize (not-finite, and/or negative)"
            ))
        }
    }

    fn into_f64(self) -> Result<f64, String> {
        Ok(self as f64)
    }
}

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
        self.0
            .as_mut()
            .map(|buf_sender| buf_sender.flush_send())
            .unwrap_or(Ok(()))
    }

    // fn is_some(&self) -> bool {
    //     self.0.is_some()
    // }
}

#[derive(Debug, Clone)]
struct HistoryVec<Data>
where
    Data: Default + Clone + Copy + Averageable,
{
    capacity: usize,
    inner: Vec<Data>,
    // current_sum: T,
    average: Data,
}

impl<Data> Default for HistoryVec<Data>
where
    Data: Default + Clone + Copy + Averageable + Debug,
{
    fn default() -> Self {
        HistoryVec {
            capacity: MAX_HISTORY,
            inner: Vec::with_capacity(MAX_HISTORY),
            average: Data::default(),
        }
    }
}

trait Averageable
where
    Self: Sized + Clone + Copy,
{
    type Intermediate;
    fn sub_then_div(&self, sub_rhs: Self, div_rhs: usize) -> Self::Intermediate;

    fn add_delta(&self, delta: Self::Intermediate) -> Self;

    fn increment_existing_avg(
        self,
        existing_avg: Self,
        popped: Option<Self>,
        new_n: usize,
    ) -> Self {
        // We want to find a number delta such that delta + existing_avg is the new,
        // updated average.

        // suppose we have hit the maximum capacity in our history vector
        // therefore, we will be popping out the last element (`popped`) and then
        // adding `self` to the history vec. Let `q` be the sum of all the
        // elements in the history vector apart from the popped one.
        // So, in this case:
        //     (q + popped)/n  +  delta          = (q + self)/n
        // ==  (q + popped)/n  -  (q + self)/n   = -delta
        // ==  (q + popped)/n  -  (q + self)/n   = -delta
        // ==  (popped - self)/n                 = -delta
        // ==  (self - popped)/n                 =  delta
        // current_average + delta = (current_sum - popped + self)/len
        // which implies that
        //
        // In the case where we have not yet reached the maximum capacity of our
        // history vector, we will be appending self to the list without changing.
        // So the increment `delta` should be the solution to:
        //     q/(n - 1)  +  delta = (q + self)/n
        //
        // Instead of solving this directly, note that we have a list of (n - 1)
        // numbers, with average q/(n - 1). To this list, can we add another number
        // (the nth number), such that the new average is still q/(n - 1)?
        // Yes! We know (intuitively) that if we add a new number which is
        // exactly the existing average, then the existing average will not shift:
        //    (q + (q/n - 1))/n
        // == ((n - 1)q + q) / n(n - 1)
        // == (qn - q + q)/n(n - 1)
        // == qn / (n(n-1))
        // == q/(n - 1)
        //
        // Going back to our problem of interest, suppose that we have (n - 1)
        // numbers, and we would like to add another number to it that leaves
        // the average unchanged. We know that this number is existing_avg. Now
        // we have a list of n numbers, where the last number is existing_avg.
        // So, following what we derived in the last section: let
        // popped == existing_avg
        //
        // Then, the new average should be: (self - popped)/n
        // Now we have a list of n numbers, with the average q/(n - 1),
        // and we can use the formula derived for the preceding if statement
        // as follows to get that the increment should be (self - existing_avg)/n:
        //     (self - existing_avg)/n + existing_avg
        // ==  (self - existing_avg + n*existing_avg)/n
        // ==  (self + existing_avg * (n - 1))/n
        // ==  (self + q)/n
        // Intriguingly, this formula also works for the case (n - 1) == 0
        let popped = popped.unwrap_or(existing_avg);
        let delta = self.sub_then_div(popped, new_n);
        existing_avg.add_delta(delta)
    }
}

impl Averageable for Duration {
    type Intermediate = f64;
    fn sub_then_div(&self, sub_rhs: Duration, div_rhs: usize) -> Self::Intermediate {
        if div_rhs > 0 {
            let r = (self.as_secs_f64() - sub_rhs.as_secs_f64()) / (div_rhs as f64);
            if !r.is_finite() {
                panic!("result of subtracting ({self:?} - {sub_rhs:?})/{div_rhs} is not finite")
            }
            r
        } else {
            panic!("cannot divide by 0!");
        }
    }
    fn add_delta(&self, delta: Self::Intermediate) -> Self {
        let r = self.as_secs_f64() + delta;
        if r.is_sign_positive() {
            Duration::from_secs_f64(r)
        } else {
            panic!("cannot perform {self:?} + {delta:?}");
        }
    }

    // fn unitless_mul(&self, rhs: usize) -> Self {
    //     self.checked_mul(rhs.try_into().unwrap()).unwrap()
    // }

    // fn unitless_div(self, rhs: usize) -> Self {
    //     self.checked_div(rhs.try_into().unwrap()).unwrap()
    // }

    // fn from_sub_out(x: Self::SubOut) -> Self {
    //     Duration::from_secs_f64(x)
    // }
}

impl Averageable for usize {
    type Intermediate = i64;
    fn sub_then_div(&self, sub_rhs: usize, div_rhs: usize) -> Self::Intermediate {
        if div_rhs > 0 {
            ((if *self < sub_rhs {
                ((sub_rhs - self) as i64) * -1
            } else {
                (self - sub_rhs) as i64
            }) as f64
                / (div_rhs as f64))
                .round() as i64
        } else {
            panic!("cannot divide by 0!");
        }
    }

    fn add_delta(&self, delta: Self::Intermediate) -> Self {
        let r = (*self as i64) + delta;
        if r < 0 {
            panic!("result of adding {delta} to {self} is negative!")
        } else {
            r as usize
        }
    }

    // fn unitless_mul(&self, rhs: usize) -> Self {
    //     self * rhs
    // }

    // fn unitless_div(self, rhs: usize) -> Self {
    //     ((self as f64) / (rhs as f64)).round() as usize
    // }

    // fn from_sub_out(x: Self::SubOut) -> Self {
    //     if x < 0 {
    //         panic!("Cannot convert {x} into usize");
    //     } else {
    //         x as usize
    //     }
    // }
}

impl Averageable for f64 {
    type Intermediate = f64;
    fn sub_then_div(&self, sub_rhs: f64, div_rhs: usize) -> Self::Intermediate {
        if div_rhs > 0 {
            (self - sub_rhs) / (div_rhs as f64)
        } else {
            panic!("cannot divide by 0!");
        }
    }

    fn add_delta(&self, delta: Self::Intermediate) -> Self {
        self + (delta as Self::Intermediate)
    }

    // fn unitless_mul(&self, rhs: usize) -> Self {
    //     self * (rhs as f64)
    // }

    // fn unitless_div(self, rhs: usize) -> Self {
    //     self / (rhs as f64)
    // }

    // fn from_sub_out(x: Self::SubOut) -> Self {
    //     x
    // }
}

trait SimpleAverageable {
    fn unitless_div(self, rhs: usize) -> Self;
}

impl SimpleAverageable for Duration {
    fn unitless_div(self, rhs: usize) -> Self {
        self.checked_div(rhs.try_into().unwrap()).unwrap()
    }
}

impl SimpleAverageable for usize {
    fn unitless_div(self, rhs: usize) -> Self {
        ((self as f64) / (rhs as f64)).round() as usize
    }
}

impl SimpleAverageable for f64 {
    fn unitless_div(self, rhs: usize) -> Self {
        self / (rhs as f64)
    }
}

impl<Data> HistoryVec<Data>
where
    Data: Default + Clone + Copy + Averageable + Debug + Add<Data> + Sum<Data> + SimpleAverageable,
{
    fn push(&mut self, k: Data) {
        // self.average = if self.inner.len() == self.capacity {
        //     k.increment_existing_avg(
        //         self.average,
        //         self.inner.pop().unwrap().into(),
        //         self.inner.len(),
        //     )
        // } else {
        //     k.increment_existing_avg(self.average, None, self.inner.len() + 1)
        // };
        if self.inner.len() == self.capacity {
            self.inner.pop();
        }
        self.inner.push(k);
        self.average = (self.inner.iter().copied().sum::<Data>()).unitless_div(self.inner.len());
    }

    fn last(&self) -> Data {
        self.inner[self.inner.len() - 1]
    }

    fn iter(&self) -> std::slice::Iter<Data> {
        self.inner.iter()
    }
}

macro_rules! create_paired_comm {
    (
        $name:ident ;
        LHS: $(($fid:ident, $fty:ty)),+ ;
        RHS: $(($gid:ident, $gty:ty)),+
    ) => {
        paste! {
            struct $name {
                $([< $fid _sender >]: Sender<$fty>,)+
                $([< $gid _receiver >]: Receiver<$gty>,)+
            }
        }
    }
}

macro_rules! create_paired_comms {
    (
        [ $lhs_snake_name:ident ; $lhs_struct_id:ident ; $(($fid:ident, $fty:ty)),+ ] <->
        [ $rhs_snake_name:ident ; $rhs_struct_id:ident ; $(($gid:ident, $gty:ty)),+ ]
    ) => {
        create_paired_comm!(
            $lhs_struct_id ;
            LHS: $(($fid, $fty)),+ ;
            RHS: $(($gid, $gty)),+
        );
        create_paired_comm!(
            $rhs_struct_id ;
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

#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Eq, Ord)]
struct TimeStamp(Option<Instant>);

impl TimeStamp {
    #[inline]
    fn mark(&mut self) {
        self.0.replace(Instant::now());
    }

    fn elapsed(&self) -> Option<Duration> {
        self.0.map(|instant| instant.elapsed())
    }

    fn maybe_instant(&self) -> Option<Instant> {
        self.0.into()
    }
}

#[derive(Clone, Debug)]
struct WorkResults {
    avg_task_time: Duration,
    avg_idle_time: Duration,
    avg_processing_rate: f64,
    dirs_processed: usize,
    max_dir_size: usize,
    discovered: Vec<PathBuf>,
}

impl Default for WorkResults {
    fn default() -> Self {
        WorkResults {
            avg_task_time: Duration::ZERO,
            avg_idle_time: Duration::ZERO,
            avg_processing_rate: 0.0,
            dirs_processed: 0,
            max_dir_size: 0,
            discovered: Vec::new(),
        }
    }
}

impl WorkResults {
    fn merge(mut self, next: WorkResults) -> Self {
        self.avg_idle_time = next.avg_idle_time;
        self.avg_task_time = next.avg_task_time;
        self.avg_processing_rate = next.avg_processing_rate;
        self.dirs_processed += next.dirs_processed;
        self.max_dir_size = self.max_dir_size.max(next.max_dir_size);
        self.discovered.extend(next.discovered.into_iter());
        self
    }
}

impl std::iter::Sum for WorkResults {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.reduce(|a, b| a.merge(b)).unwrap_or_default()
    }
}

#[derive(Default, Clone, Debug)]
struct ResultsBuffer {
    max_dir_size: usize,
    discovered: Vec<PathBuf>,
}

#[derive(Default)]
struct ThreadHistory {
    task_times: HistoryVec<Duration>,
    idle_times: HistoryVec<Duration>,
    post_task_times: HistoryVec<Duration>,
    dirs_processed: HistoryVec<usize>,
    processing_rates: HistoryVec<f64>,
    t_task_started: TimeStamp,
    t_began_idling: TimeStamp,
    t_finished: TimeStamp,
}

impl ThreadHistory {
    fn mark_began_idling(&mut self) {
        if let Some(finished_instant) = self.t_finished.maybe_instant() {
            self.post_task_times.push(finished_instant.elapsed());
        }
        self.t_began_idling.mark();
    }

    fn mark_started(&mut self) {
        self.idle_times.push(self.t_began_idling.elapsed().expect("if we are marking a task start, then we must have marked the time when we began idling"));
        self.t_task_started.mark();
    }

    fn mark_finished(&mut self, dirs_processed: usize) {
        self.task_times.push(self.t_task_started.elapsed().expect(
            "if we are marking a task finish time, then we must have marked time since we started",
        ));
        self.dirs_processed.push(dirs_processed);
        self.processing_rates
            .push((dirs_processed as f64) / self.task_times.last().as_secs_f64());
    }
}

create_paired_comms!(
    [handle ;  ThreadHandleComms ; (order, Vec<PathBuf>)] <->
    [thread ; ThreadComms ; (status, Status), (result, WorkResults) ]
);

struct Thread {
    comms: ThreadComms,
    printer: Printer,
    status: Status,
    history: ThreadHistory,
    results_buffer: ResultsBuffer,
}

struct ThreadHandle {
    comms: ThreadHandleComms,
    status: Status,
    worker_thread: JoinHandle<()>,
    in_flight: usize,
    orders: Vec<PathBuf>,
    dirs_processed: usize,
    avg_task_time: Duration,
    avg_idle_time: Duration,
    avg_processing_rate: f64,
}

struct Executor {
    work_q: Vec<PathBuf>,
    print_receiver: Option<Receiver<Vec<String>>>,
    handles: Vec<ThreadHandle>,
    max_dir_size: usize,
    last_status_print: Option<Instant>,
    start_time: Instant,
    processed: usize,
    total_submitted: Vec<usize>,
    orders_submitted: usize,
    loop_sleep_time: Duration,
    loop_sleep_time_history: HistoryVec<Duration>,
}

impl Thread {
    fn new(comms: ThreadComms, print_sender: Option<Sender<Vec<String>>>) -> Self {
        comms.status_sender.send(Status::Idle).unwrap();
        Self {
            comms,
            printer: Printer::new(print_sender),
            status: Status::Idle,
            history: ThreadHistory::default(),
            results_buffer: ResultsBuffer::default(),
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

    fn start(mut self) {
        // self.history.mark_began_idling();
        loop {
            self.printer.flush_send().unwrap();

            self.history.mark_began_idling();
            let work_order = self.comms.order_receiver.recv().unwrap();

            self.history.mark_started();
            self.change_status(Status::Busy).unwrap();

            for path in work_order.iter() {
                let dir_entries = read_dir(path)
                    .and_then(|read_iter| read_iter.collect::<IoResult<Vec<DirEntry>>>())
                    .unwrap_or_else(|err| {
                        self.printer.push(|| map_io_error(path, err));
                        vec![]
                    });

                if dir_entries.len() > 0 {
                    self.results_buffer.max_dir_size = dir_entries.len().max(dir_entries.len());
                    self.results_buffer
                        .discovered
                        .extend(dir_entries.iter().filter_map(|entry| {
                            entry
                                .metadata()
                                .ok()
                                .and_then(|meta| meta.is_dir().then_some(entry.path()))
                        }));
                    self.printer
                        .push(|| format!("Processed {:?}", &path.as_os_str(),));
                }
            }
            self.history.mark_finished(work_order.len());
            self.comms
                .result_sender
                .send(WorkResults {
                    avg_task_time: self.history.task_times.average,
                    avg_idle_time: self.history.idle_times.average,
                    max_dir_size: self.results_buffer.max_dir_size,
                    discovered: self.results_buffer.discovered.drain(0..).collect(),
                    avg_processing_rate: self.history.processing_rates.average,
                    dirs_processed: self.history.dirs_processed.last(),
                })
                .unwrap();
            self.comms.status_sender.send(Status::Idle).unwrap();
        }
    }
}

impl ThreadHandle {
    fn new(print_sender: Option<Sender<Vec<String>>>) -> Self {
        let (handle_comms, process_thread_comms) = new_handle_to_thread_comms();
        let worker = Thread::new(process_thread_comms, print_sender);

        ThreadHandle {
            comms: handle_comms,
            status: Status::Idle,
            worker_thread: thread::spawn(move || worker.start()),
            in_flight: 0,
            dirs_processed: 0,
            avg_task_time: Duration::ZERO,
            avg_idle_time: Duration::ZERO,
            avg_processing_rate: f64::default(),
            orders: vec![],
        }
    }

    fn get_avg_task_time(&self) -> Duration {
        self.avg_task_time
    }

    fn get_avg_idle_time(&self) -> Duration {
        self.avg_idle_time
    }

    fn get_avg_processing_rate(&self) -> f64 {
        self.avg_processing_rate
    }

    // fn drain_orders(&mut self) -> Vec<PathBuf> {
    //     let order: Vec<PathBuf> = self.orders.drain(0..).collect();
    //     self.in_flight += order.len();
    //     order
    // }

    // fn queue_orders(&mut self, orders: Vec<PathBuf>) {
    //     self.orders.extend(orders.into_iter());
    // }

    fn dispatch_orders(&mut self, orders: Vec<PathBuf>) -> Result<(), WorkError> {
        // if self.is_idle() {
        //     self.in_flight += orders.len();
        //     self.comms.order_sender.send(orders)?;
        //     let drained_orders = self.drain_orders();
        //     self.comms.order_sender.send(drained_orders)?;
        // } else {
        //     self.queue_orders(orders);
        // }
        self.in_flight += orders.len();
        self.comms.order_sender.send(orders)?;
        // let drained_orders = self.drain_orders();
        // self.comms.order_sender.send(drained_orders)?;
        Ok(())
    }

    fn push_orders(&mut self, orders: Vec<PathBuf>) -> Result<(), WorkError> {
        self.dispatch_orders(orders)?;
        // if self.is_idle() {
        //     self.dispatch_orders(orders)?;
        // } else {
        //     self.queue_orders(orders);
        // }
        Ok(())
    }

    fn update_dirs_processed(&mut self, dirs_processed: usize) {
        self.dirs_processed += dirs_processed;
        self.in_flight -= dirs_processed;
    }

    fn drain_results(&mut self) -> Option<(usize, Vec<PathBuf>)> {
        let results: Vec<WorkResults> = self.comms.result_receiver.try_iter().collect();
        if results.len() > 0 {
            let WorkResults {
                avg_task_time,
                avg_idle_time,
                avg_processing_rate,
                dirs_processed,
                max_dir_size,
                discovered,
            } = results.into_iter().sum();
            self.avg_task_time = avg_task_time;
            self.avg_idle_time = avg_idle_time;
            self.avg_processing_rate = avg_processing_rate;
            self.update_dirs_processed(dirs_processed);
            Some((max_dir_size, discovered))
        } else {
            None
        }
    }

    fn currently_submitted(&self) -> usize {
        self.in_flight + self.orders.len()
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

trait CustomDisplay {
    fn custom_display(&self) -> String;
}

impl CustomDisplay for f64 {
    fn custom_display(&self) -> String {
        format!("{:e}", self.round_sig_figs(6))
    }
}

impl CustomDisplay for Duration {
    fn custom_display(&self) -> String {
        format!("{self:?}")
    }
}

impl CustomDisplay for usize {
    fn custom_display(&self) -> String {
        format!("{self}")
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

trait FindMaxMin
where
    Self: IntoIterator + Sized,
    <Self as IntoIterator>::Item: PartialOrd + Copy,
{
    fn find_max(
        self,
        default_for_max: <Self as IntoIterator>::Item,
    ) -> <Self as IntoIterator>::Item {
        self.into_iter()
            .max_by(|p, q| p.partial_cmp(q).unwrap_or(Ordering::Less))
            .unwrap_or(default_for_max)
    }
    fn find_min(
        self,
        default_for_min: <Self as IntoIterator>::Item,
    ) -> <Self as IntoIterator>::Item {
        self.into_iter()
            .min_by(|p, q| p.partial_cmp(q).unwrap_or(Ordering::Less))
            .unwrap_or(default_for_min)
    }
}

impl<'a, T> FindMaxMin for &'a Vec<T> where T: PartialOrd + Copy {}

impl Executor {
    fn new(work: Vec<PathBuf>, verbose: bool) -> Self {
        let (print_sender, print_receiver) = if verbose {
            let (print_sender, print_receiver) = mpsc::channel();
            (Some(print_sender), Some(print_receiver))
        } else {
            (None, None)
        };

        let handles = (0..NUM_THREADS)
            .map(|_| ThreadHandle::new(print_sender.clone()))
            .collect();

        Self {
            work_q: work,
            print_receiver,
            handles,
            max_dir_size: 0,
            last_status_print: None,
            start_time: Instant::now(),
            processed: 0,
            orders_submitted: 0,
            total_submitted: vec![0; NUM_THREADS],
            loop_sleep_time: DEFAULT_EXECUTE_LOOP_SLEEP,
            loop_sleep_time_history: HistoryVec::default(),
        }
    }

    fn all_handles_idle(&mut self) -> bool {
        self.handles.iter_mut().all(ThreadHandle::is_idle)
    }

    fn fetch_stats<T>(&self, f: fn(&ThreadHandle) -> T) -> Vec<T>
    where
        T: PartialOrd + Copy + Default,
    {
        self.handles.iter().map(f).collect()
    }

    fn fetch_stats_and_max<'a, T>(
        &self,
        f: fn(&ThreadHandle) -> T,
        default_for_max: T,
    ) -> (Vec<T>, T)
    where
        T: PartialOrd + Copy + Default,
    {
        let stats: Vec<T> = self.fetch_stats(f);
        let max = stats.find_max(&default_for_max).clone();
        (stats, max)
    }

    fn distribute_work(&mut self) -> Result<(), WorkError> {
        let (_, _) = self.fetch_stats_and_max(|h| h.avg_task_time.as_secs_f64(), 0.0);
        let (idle_times, max_idle_time) =
            self.fetch_stats_and_max(|h| h.avg_idle_time.as_secs_f64(), 0.0);
        let (processing_rates, max_processing_rate) =
            self.fetch_stats_and_max(|h| h.avg_processing_rate, 0.0);
        let currently_submitted = self.fetch_stats(|h| h.currently_submitted());
        let max_per_thread =
            (self.work_q.len() as f64 / self.handles.len() as f64).floor() as usize;
        let dispatch_sizes: Vec<usize> = if max_idle_time > 0.0 && max_processing_rate > 0.0 {
            // bias distribution
            // let ratings = task_times
            //     .iter()
            //     .zip(idle_times.iter())
            //     .zip(processing_rates.iter())
            //     .map(|((&task_time, &idle_time), _)| {
            //         (max_task_time / max_idle_time).min(1.0)
            //             * task_time_score(task_time, max_task_time)
            //             + idle_time_score(idle_time, max_idle_time)
            //     })
            //     .collect::<Vec<f64>>();
            // let normalizer: f64 = *ratings.find_max(&1.0);
            // self.ratings = ratings.iter().map(|r| r / normalizer).collect();
            // ratings
            //     .into_iter()
            //     .map(|r| (r / normalizer))
            //     .zip(currently_submitted.iter())
            //     .map(|(r, in_flight)| {
            //         ((r * max_per_thread as f64).round() as usize)
            //             .min(MAX_IN_FLIGHT - in_flight)
            //     })
            //     .collect()
            let requested_dispatches: Vec<usize> = processing_rates
                .iter()
                .zip(idle_times.iter())
                .zip(currently_submitted.iter())
                .map(|((&processing_rate, &idle_time), &in_flight)| {
                    let filled_idle_time = in_flight as f64 / processing_rate;
                    let unfilled_idle_time = (idle_time > filled_idle_time)
                        .then(|| idle_time - filled_idle_time)
                        .unwrap_or(0.0);
                    let would_like = ((processing_rate * unfilled_idle_time).round() as usize)
                        .max(if in_flight == 0 { 1 } else { 0 });
                    // let would_like = if would_like + in_flight > MAX_IN_FLIGHT {
                    //     (would_like + in_flight) - MAX_IN_FLIGHT
                    // } else {
                    //     would_like
                    // };
                    would_like
                })
                .collect();
            // println!(
            //     "({}, {}) requested_dispatches: {requested_dispatches:?}",
            //     processing_rates.len(),
            //     idle_times.len()
            // );
            let max_dispatch_total = self.work_q.len().min(MAX_TOTAL_DISPATCH_SIZE);
            if requested_dispatches.iter().sum::<usize>() > max_dispatch_total {
                let max_dispatch_request = requested_dispatches
                    .iter()
                    .max()
                    .copied()
                    .unwrap_or(1_usize) as f64;
                requested_dispatches
                    .into_iter()
                    .map(|x| {
                        ((x as f64 / max_dispatch_request) * max_dispatch_total as f64).round()
                            as usize
                    })
                    .collect()
            } else {
                requested_dispatches
            }
        } else {
            // distribute equally
            let max_per_thread = max_per_thread.max(1);
            vec![max_per_thread; self.handles.len()]
        };
        // println!("dispatches: {dispatch_sizes:?}");
        dispatch_sizes
            .into_iter()
            .zip(self.handles.iter_mut())
            .zip(currently_submitted.iter())
            .map(|((dispatch_size, handle), &_)| {
                let final_dispatch_size = dispatch_size.min(self.work_q.len());
                let orders: Vec<PathBuf> = self.work_q.drain(0..final_dispatch_size).collect();
                let dispatch_size = orders.len();
                if dispatch_size > 0 {
                    handle.push_orders(orders).unwrap();
                    self.orders_submitted += dispatch_size;
                }
                final_dispatch_size
            })
            .zip(self.total_submitted.iter_mut())
            .for_each(|(final_dispatch, current_total)| {
                *current_total = *current_total + final_dispatch
            });
        self.loop_sleep_time = Duration::from_secs_f64(
            idle_times
                .iter()
                .min_by(|p, q| p.partial_cmp(q).unwrap_or(Ordering::Less))
                .copied()
                .map(|d| d / (5.0 * NUM_THREADS as f64))
                .unwrap_or(DEFAULT_EXECUTE_LOOP_SLEEP.as_secs_f64()),
        );
        self.loop_sleep_time_history.push(self.loop_sleep_time);
        sleep(self.loop_sleep_time);
        Ok(())
    }

    fn print_handle_avg_info<T: Clone + Copy + PartialOrd + Sum + CustomDisplay>(
        &self,
        title: &'static str,
        avg_info_fetch: fn(&ThreadHandle) -> T,
        print_total: bool,
    ) {
        let avg_info: Vec<T> = self.handles.iter().map(avg_info_fetch).collect();
        let mut sorted = avg_info.clone();
        sorted.sort_unstable_by(|p, q| {
            (p < q)
                .then_some(Ordering::Less)
                .unwrap_or(Ordering::Greater)
        });
        println!(
            "{} (max: {}, min: {}{}): {}",
            title,
            sorted[sorted.len() - 1].custom_display(),
            sorted[0].custom_display(),
            print_total
                .then_some(format!(
                    ", total: {}",
                    avg_info.iter().copied().sum::<T>().custom_display()
                ))
                .unwrap_or("".into()),
            avg_info
                .iter()
                .map(|t| t.custom_display())
                .collect::<Vec<String>>()
                .join(", ")
        );
    }

    fn print_status(&mut self) {
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
                "{} directories visited. {} new orders submitted. {}/{} idle. Loop wait time: {}, Running for: {}:{}. Overall rate: {}",
                self.processed,
                self.orders_submitted,
                self.handles.iter_mut().filter_map(|p| p.is_idle().then_some(1)).sum::<usize>(),
                self.handles.len(),
                self.loop_sleep_time.custom_display(),
                minutes,
                seconds,
                ((self.processed as f64) / run_time.as_secs_f64()).round()
            );
            println!("total_submitted_since_last: {:?}", self.total_submitted);
            self.total_submitted = vec![0; NUM_THREADS];

            self.print_handle_avg_info(
                "processing rates",
                ThreadHandle::get_avg_processing_rate,
                true,
            );

            println!(
                "sleep: {}",
                self.loop_sleep_time_history
                    .iter()
                    .map(|d| format!("{}", d.custom_display()))
                    .collect::<Vec<String>>()
                    .join(", ")
            );

            self.print_handle_avg_info("task times", ThreadHandle::get_avg_task_time, true);

            self.print_handle_avg_info("idle times", ThreadHandle::get_avg_idle_time, true);

            self.print_handle_avg_info("in flight", ThreadHandle::currently_submitted, true);

            self.orders_submitted = 0;
        }
    }

    fn handle_print_requests(&self) {
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
        for (max_dir_size, discovered, dirs_processed) in self.handles.iter_mut().filter_map(|p| {
            p.drain_results()
                .map(|(max_dir_size, discovered)| (max_dir_size, discovered, p.dirs_processed))
        }) {
            // println!("Got some new work!");
            if max_dir_size > self.max_dir_size {
                self.max_dir_size = max_dir_size;
                println!("Found a directory with {} entries.", self.max_dir_size);
            }
            self.work_q.extend(discovered.into_iter());
            self.processed += dirs_processed;
        }
    }

    fn execute(mut self) -> Result<usize, WorkError> {
        self.start_time = Instant::now();
        loop {
            self.handle_print_requests();
            self.process_results();
            if self.work_q.len() > 0 {
                self.distribute_work()?;
            } else if self.all_handles_idle() {
                self.process_results();
                if self.work_q.len() > 0 {
                    self.distribute_work()?;
                } else {
                    let run_time = self.start_time.elapsed();
                    let minutes = run_time.as_secs() / 60;
                    let seconds = run_time.as_secs() % 60;
                    println!(
                        "Done! {} directories visited. Ran for: {}:{}",
                        self.processed, minutes, seconds,
                    );
                    break;
                }
            }
            self.print_status();
        }
        for worker in self.handles.into_iter() {
            worker.finish()
        }
        Ok(self.max_dir_size)
    }
}

fn map_io_error(path: &Path, io_err: IoError) -> String {
    format!("Could not open path: {path:?} due to error {io_err}")
}

fn main() {
    let start = Instant::now();
    let manager = Executor::new(vec!["C:\\".into(), "A:\\".into(), "B:\\".into()], false);
    let result = manager.execute().unwrap();
    println!("Final max dir entry count: {}", result);
    println!("Took {}.", start.elapsed().custom_display());
}
