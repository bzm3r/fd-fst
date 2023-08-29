use crate::{
    num::Num,
    num_check::NumResult,
    num_conv::{TryFromNum, TryIntoNum},
};

pub trait RoundSigFigs: Num + TryFromNum<f64> + TryIntoNum<f64> {
    fn from_f64(f: f64) -> NumResult<Self>;
    fn into_f64(self) -> NumResult<f64>;

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

impl<T> RoundSigFigs for T
where
    T: Num + TryFromNum<f64> + TryIntoNum<f64>,
{
    fn from_f64(f: f64) -> NumResult<T> {
        Self::try_from_num(f)
    }

    fn into_f64(self) -> NumResult<f64> {
        self.try_into_num()
    }

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
