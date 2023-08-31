use crate::{
    display::CustomDisplay,
    hist_defs::*,
    num_check::NumResult,
    num_hist::{HistoryNum, InnerAbsolute},
    signed_num::SignedNum,
};
use std::fmt::Debug;
use std::{cmp::Ordering, fmt::Display};

pub trait Averageable
where
    Self: HistoryNum,
{
    fn sub_then_div(&self, sub_rhs: Self, div_rhs: usize) -> NumResult<Self> {
        self.difference(sub_rhs).div_usize(div_rhs)
    }

    fn add_delta(&self, delta: Self) -> Self {
        self.increment(delta)
    }

    fn incremdent_existing_avg(
        self,
        existing_avg: Self,
        popped: Option<Self>,
        new_n: usize,
    ) -> NumResult<Self> {
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
        delta.map(|d| existing_avg.add_delta(d))
    }
}

#[derive(Debug, Clone)]
pub struct HistoryVec<Data, const MAX_HISTORY: usize>
where
    Data: HistoryNum,
{
    pub inner: Vec<Data>,
    // current_sum: T,
    pub average: Data,
}

impl<Data: HistoryNum, const MAX_HISTORY: usize> HistoryVec<Data, MAX_HISTORY> {
    pub fn last(&self) -> Option<Data> {
        self.inner.last().copied()
    }
}

impl<Data, const MAX_HISTORY: usize> Default for HistoryVec<Data, MAX_HISTORY>
where
    Data: HistoryNum,
{
    fn default() -> Self {
        HistoryVec {
            inner: Vec::with_capacity(MAX_HISTORY),
            average: Data::default(),
        }
    }
}
#[derive(Default, Clone, Copy, Debug)]
pub struct AvgInfo<T>
where
    T: HistoryNum,
{
    pub data: T,
    pub delta: T,
}

impl<T> Display for AvgInfo<T>
where
    T: HistoryNum,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?}({}{:?})",
            self.data,
            self.delta
                .adaptor()
                .positive()
                .then_some("+")
                .unwrap_or("-"),
            self.delta.absolute()
        )
    }
}

impl<T> AvgInfo<T>
where
    T: HistoryNum,
{
    pub fn update(&mut self, new_data: T) -> Self {
        let last = *self;
        self.data = new_data;
        self.delta = self.data.difference(last.data);
        last
    }
}

impl<T> Averageable for T where T: HistoryNum {}

impl<Data, const MAX_HISTORY: usize> HistoryVec<Data, MAX_HISTORY>
where
    Data: HistoryNum,
{
    pub fn push(&mut self, k: InnerAbsolute<Data>) {
        if self.inner.len() == MAX_HISTORY {
            self.inner.pop();
        }
        self.inner.push(Data::from_absolute(k));
        self.average = (Data::iter_sum(self.inner.iter().copied()))
            .div_usize(self.inner.len())
            .unwrap_or(Data::default());
    }

    pub fn iter(&self) -> std::slice::Iter<Data> {
        self.inner.iter()
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct AvgInfoBundle {
    pub processing_rate: AvgInfo<ProcessingRate>,
    pub task_time: AvgInfo<TimeSpan>,
    pub idle_time: AvgInfo<TimeSpan>,
}

impl AvgInfoBundle {
    pub fn update(
        &mut self,
        processing_rate: ProcessingRate,
        task_time: TimeSpan,
        idle_time: TimeSpan,
    ) -> Self {
        Self {
            processing_rate: self.processing_rate.update(processing_rate),
            task_time: self.task_time.update(task_time),
            idle_time: self.idle_time.update(idle_time),
        }
    }
}

impl Display for AvgInfoBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "pr: {} | tt/it: {} | tt: {} | it : {}",
            self.processing_rate.data.custom_display(),
            self.task_time
                .data
                .ratio(self.idle_time.data)
                .map(|f| f.custom_display())
                .unwrap_or("None".into()),
            self.task_time.data.custom_display(),
            self.idle_time.data.custom_display(),
        )
    }
}

pub trait FindMaxMin
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

pub struct AvgInfoSummary<T>
where
    T: HistoryNum,
{
    pub max: Option<T>,
    pub min: Option<T>,
    pub total: T,
}

impl<T, I> From<I> for AvgInfoSummary<T>
where
    T: Averageable + Debug + Copy + Clone,
    I: Iterator<Item = AvgInfo<T>>,
{
    fn from(info_vec: I) -> Self {
        let data: Vec<T> = info_vec.map(|x| x.data).collect();
        Self {
            max: data
                .iter()
                .copied()
                .max_by(|p, q| p.partial_cmp(&q).unwrap_or(Ordering::Less)),
            min: data
                .iter()
                .copied()
                .min_by(|p, q| p.partial_cmp(&q).unwrap_or(Ordering::Greater)),
            total: data
                .iter()
                .copied()
                .fold(T::default(), |p, q| p.increment(q)),
        }
    }
}

pub struct AvgInfoWithSummaries {
    // processing_rates: Vec<AvgInfo<AdaptedProcessingRate>>,
    // task_times: Vec<AvgInfo<AdaptedDuration>>,
    // idle_times: Vec<AvgInfo<AdaptedDuration>>,
    pub summary_processing_rates: AvgInfoSummary<ProcessingRate>,
    pub summary_task_times: AvgInfoSummary<TimeSpan>,
    pub summary_idle_times: AvgInfoSummary<TimeSpan>,
}

impl From<Vec<AvgInfoBundle>> for AvgInfoWithSummaries {
    fn from(avg_info_bundle: Vec<AvgInfoBundle>) -> Self {
        let processing_rates: Vec<AvgInfo<ProcessingRate>> =
            avg_info_bundle.iter().map(|x| x.processing_rate).collect();
        let task_times: Vec<AvgInfo<TimeSpan>> =
            avg_info_bundle.iter().map(|x| x.task_time).collect();
        let idle_times: Vec<AvgInfo<TimeSpan>> =
            avg_info_bundle.iter().map(|x| x.idle_time).collect();
        let summary_processing_rates = processing_rates.iter().copied().into();
        let summary_task_times = task_times.iter().copied().into();
        let summary_idle_times = idle_times.iter().copied().into();
        Self {
            // processing_rates,
            // task_times,
            // idle_times,
            summary_processing_rates,
            summary_task_times,
            summary_idle_times,
        }
    }
}
