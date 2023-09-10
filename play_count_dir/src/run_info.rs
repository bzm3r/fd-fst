use paste::paste;
use std::{
    cell::Cell,
    time::{Duration, Instant},
    fmt::Debug,
};

use crate::{
    misc_types::CellSlot,
    hist_defs::{ProcessingRate, TimeSpan},
    history::{HasHistory, HistoryVec},
    num::Num,
    num_conv::IntoNum,
    num_hist::{HistData, HistoryNum},
    Thread, MAX_HISTORY,
};

#[derive(Clone, Debug, Default)]
pub struct TimeSpanHistory {
    history: HistoryVec<TimeSpan>,
    total: Duration,
}

impl TimeSpanHistory {
    #[inline]
    fn new(capacity: usize) -> Self {
        Self {
            history: HistoryVec::new(capacity),
            total: Duration::ZERO,
        }
    }

    #[inline]
    fn new_celled(capacity: usize) -> CellSlot<Self> {
        CellSlot::new(Self::new(capacity))
    }

    #[inline]
    pub fn push(&mut self, elapsed: Duration) {
        self.history.push(elapsed);
        self.total += elapsed;
    }
}

impl HasHistory<TimeSpan> for TimeSpanHistory {
    #[inline]
    fn history_vec(&self) -> &HistoryVec<TimeSpan> {
        &self.history
    }
}

#[derive()]
pub struct Timer<'a> {
    parent: &'a ThreadMetrics,
    event: HistoryEvent,
    start: Instant,
}

impl<'a> Timer<'a> {
    pub fn new(parent: &'a ThreadMetrics, event: HistoryEvent) -> Self {
        Self {
            parent,
            event,
            start: Instant::now(),
        }
    }

    pub fn end_then_begin(mut self, next_event: HistoryEvent) -> Self {
        self.finished();
        self.event = next_event;
        self.start = Instant::now();
    }

    #[inline]
    pub fn finished(&mut self) {
        self.parent.end_event(self.start.elapsed(), self.event);
    }
}

impl<'a> Drop for Timer<'a> {
    fn drop(&mut self) {
        self.finished()
    }
}


#[derive(Default, Debug, Clone)]
struct CurrentTimings {
    disk_reader_wait: CellSlot<TimeSpan>,
    disk_access_wait: CellSlot<TimeSpan>,
    read_processing_time: CellSlot<TimeSpan>,
    misc_time: CellSlot<TimeSpan>,
    disk_read_time: CellSlot<TimeSpan>,
}


#[derive(Default, Debug, Clone)]
struct CompleteTimings {
    disk_reader_wait: TimeSpan,
    disk_access_wait: TimeSpan,
    disk_read_time: TimeSpan,
    misc_time: TimeSpan,
    read_processing_time: TimeSpan,
}

impl CurrentTimings {
    fn complete(&self) -> CompleteTimings {
        self.try_complete().unwrap()
    }

    fn try_complete(&self) -> Option<CompleteTimings> {
        Some(CompleteTimings {
            disk_access_wait: self.disk_access_wait.take().ok()?,
            disk_read_time: self.disk_read_time.take().ok()?,
            misc_time: self.misc_time.take().unwrap_or_default(),
            read_processing_time: self.read_processing_time.take().ok()?,
            disk_reader_wait: self.disk_reader_wait.take().ok()?,
        })
    }
}



#[derive(Default, Debug, Clone)]
pub struct ThreadMetrics {
    history: ThreadHistory,
    curr_timings: CurrentTimings,
    t_thread_start: CellSlot<Instant>,
    total_processed: Cell<usize>,
}

#[derive(Default, Debug, Clone)]
struct ThreadHistory {
    h_processing_rate: CellSlot<HistoryVec<ProcessingRate>>,
    h_reader_wait: CellSlot<TimeSpanHistory>,
    h_access_wait: CellSlot<TimeSpanHistory>,
    h_process_dirs_time: CellSlot<TimeSpanHistory>,
    h_misc_time: CellSlot<TimeSpanHistory>,
    h_process_tasks_time: CellSlot<TimeSpanHistory>,
}

macro_rules! gen_field_updates {
    ($self:ident [$($field:ident,)*] [$($next:ident,)*]) => {
        $($self::update_field($field, $next);)*
    };
}

impl ThreadHistory {
    fn get_average<H, T>(&self, field: &CellSlot<H>) -> T
    where
        H: HasHistory<T> + Clone + Debug,
        T: HistoryNum + Clone,
    {
        let history = field.take();
        let r = history.average();
        field.overwrite_value(history);
        r
    }

    fn get_last_value<H: HasHistory<T>, T: HistoryNum>(&self, field: &CellSlot<H>) -> T
    where
        H: HasHistory<T> + Clone,
        T: HistoryNum + Clone,
    {
        let history = field.take();
        let r = history.last();
        field.overwrite_value(history);
        r
    }

    fn get_avg_processing_rate(&self) -> TimeSpan {
        self.get_average(&self.h_processing_rate)
    }

    fn get_avg_disk_reader_wait(&self) -> TimeSpan {
        self.get_average(&self.h_reader_wait)
    }

    fn get_avg_disk_access_wait(&self) -> TimeSpan {
        self.get_average(&self.h_access_wait)
    }

    fn get_avg_process_reads_time(&self) -> TimeSpan {
        self.get_average(&self.h_process_dirs_time)
    }

    fn get_avg_misc_time(&self) -> TimeSpan {
        self.get_average(&self.h_misc_time)
    }

    fn get_avg_disk_read_time(&self) -> TimeSpan {
        self.get_average(&self.h_process_tasks_time)
    }

    fn update_field<H, T>(historical: &CellSlot<H>, next_value: <H as HasHistory>::Absolute>)
    where
        H: HasHistory<T> + Clone,
        T: HistoryNum + Clone,
    {
        let mut updated = historical.force_take();
        updated.history_vec().push(next_value);
        historical.overwrite_value(updated);
    }

    pub fn update(&self, dirs_processed: usize, complete_timings: CompleteTimings) {
        let ThreadHistory {
            h_processing_rate,
            h_reader_wait,
            h_access_wait,
            h_process_dirs_time,
            h_misc_time,
            h_process_tasks_time,
        } = &self;
        let CompleteTimings {
            disk_reader_wait: reader_wait,
            disk_access_wait: access_wait,
            disk_read_time: process_dirs_time,
            misc_time,
            read_processing_time: process_tasks_time,
        } = complete_timings;

        let processing_rate = complete_timings.read_processing_time / dirs_processed;
        gen_field_updates!(
            Self [
                h_processing_rate,
                h_reader_wait,
                h_access_wait,
                h_process_dirs_time,
                h_misc_time,
                h_process_tasks_time,
            ]
            [
                processing_rate,
                reader_wait,
                access_wait,
                process_dirs_time,
                misc_time,
                process_tasks_time,
            ]
        );
    }
}

impl ThreadHistory {
    fn new(max_history: usize) -> Self {
        Self {
            h_processing_rate: CellSlot::new(HistoryVec::new(max_history)),
            h_reader_wait: TimeSpanHistory::new_celled(max_history),
            h_access_wait: TimeSpanHistory::new_celled(max_history),
            h_process_dirs_time: TimeSpanHistory::new_celled(max_history),
            h_misc_time: TimeSpanHistory::new_celled(max_history),
            h_process_tasks_time: TimeSpanHistory::new_celled(max_history),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum HistoryEvent {
    ThreadStart,
    ReadingDisk,
    WaitingForDiskReader,
    WaitingForDiskAccess,
    ProcessingReadResults,
    Miscellaneous,
}

impl ThreadMetrics {
    pub fn new(max_history: usize) -> Self {
        Self {
            history: ThreadHistory::new(max_history),
            t_thread_start: None,
            total_processed: 0,
            curr_timings: CurrentTimings::default(),
        }
    }

    pub fn begin_event(&self, event: HistoryEvent) -> Timer {
        Timer::new(self, event)
    }

    pub fn update_histories(&self, just_processed: usize) {
        let current_raw = self.curr_timings.complete();
        let processing_rate = current_raw.read_processing_time / just_processed;
        self.total_processed += just_processed;
        self.h_processing_rate.up
    }

    pub fn end_event(&self, elapsed: Duration, event: HistoryEvent) {
        match event {
            HistoryEvent::ReadingDisk => self
                .curr_timings
                .disk_read_time
                .insert_expecting_empty(elapsed),
            HistoryEvent::WaitingForDiskReader => self
                .curr_timings
                .disk_reader_wait
                .insert_expecting_empty(elapsed),
            HistoryEvent::WaitingForDiskAccess => self
                .curr_timings
                .disk_access_wait
                .insert_expecting_empty(elapsed),
            HistoryEvent::ProcessingReadResults => self
                .curr_timings
                .read_processing_time
                .insert_expecting_empty(elapsed),
            HistoryEvent::Miscellaneous => {
                self.curr_timings.misc_time.insert_expecting_empty(elapsed)
            }
            _ => {}
        }
    }

    pub fn avg_processing_rate(&self) -> ProcessingRate {
        self.history.get_avg_processing_rate()
    }

    pub fn avg_disk_reader_wait(&self) -> TimeSpan {
        self.history.get_avg_disk_reader_wait()
    }

    pub fn avg_disk_read_time(&self) -> TimeSpan {
        self.history.get_avg_disk_read_time()
    }

    pub fn avg_misc_times(&self) -> TimeSpan {
        self.history.get_avg_misc_time()
    }

    pub fn avg_disk_access_wait(&self) -> TimeSpan {
        self.history.get_avg_disk_access_wait()
    }

    pub fn avg_process_reads_time(&self) -> TimeSpan {
        self.history.get_avg_process_reads_time()
    }
}
