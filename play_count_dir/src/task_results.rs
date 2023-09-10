use crate::{
    disk::{ErrorDir, FoundTasks},
    hist_defs::{ProcessingRate, TimeSpan},
};

#[derive(Debug, Clone)]
pub struct WorkResult {
    avg_t_order: TimeSpan,
    avg_t_waiting: TimeSpan,
    avg_processing_rate: ProcessingRate,
    newly_processed: usize,
    max_dir_size: usize,
}

impl Default for WorkResult {
    fn default() -> Self {
        WorkResult {
            avg_t_order: TimeSpan::default(),
            avg_t_waiting: TimeSpan::default(),
            avg_processing_rate: ProcessingRate::default(),
            newly_processed: 0,
            max_dir_size: 0,
        }
    }
}

pub struct ReadDirSummary {}

impl WorkResult {
    pub fn summarize(errors: &Vec<ErrorDir>, discovered: &FoundTasks) -> ReadDirSummary {
        unimplemented!()
    }

    pub fn merge(mut self, next: WorkResult) -> Self {
        let WorkResult {
            avg_t_order,
            avg_t_waiting,
            avg_processing_rate,
            newly_processed,
            max_dir_size,
        } = next;
        self.avg_t_order = avg_t_order;
        self.avg_t_waiting = avg_t_waiting;
        self.avg_processing_rate = avg_processing_rate;
        self.newly_processed += newly_processed;
        self.max_dir_size = self.max_dir_size.max(max_dir_size);
        self
    }
}

impl std::iter::Sum for WorkResult {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.reduce(|a, b| a.merge(b)).unwrap_or_default()
    }
}
