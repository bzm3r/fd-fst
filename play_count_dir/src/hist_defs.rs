use crate::num_hist::HistData;
use crate::num_absf64::AbsF64;
use std::time::Duration;

pub type TimeSpan = HistData<Duration, f64>;

pub type ProcessingRate = HistData<AbsF64, f64>;
