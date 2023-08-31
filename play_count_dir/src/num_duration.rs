use std::time::Duration;

use crate::{
    adp_num::{AbsoluteNum, DivUsize},
    num_check::{FiniteTest, NonNegTest, NonZeroTest, NumErr, NumResult},
    num_conv::{FromNum, TryFromNum},
};

impl NonZeroTest for Duration {
    fn test_non_zero(&self) -> NumResult<&Self> {
        (!self.is_zero()).then_some(self).ok_or(NumErr::zero(self))
    }
}

impl NonNegTest for Duration {
    fn test_non_neg(&self) -> NumResult<&Self> {
        Ok(self)
    }
}

impl FiniteTest for Duration {
    fn test_finite(&self) -> NumResult<&Self> {
        Ok(self)
    }
}

impl FromNum<Self> for Duration {
    fn from_num(value: Self) -> Self {
        value
    }
}

impl DivUsize for Duration {
    fn div_usize(&self, rhs: usize) -> NumResult<Self> {
        rhs.test_non_zero()
            .and_then(|&rhs| u32::try_from_num(rhs))
            .and_then(|rhs| {
                self.checked_div(rhs).ok_or(NumErr::other(format!(
                    "Duration by u32 checked_div error: {self:?}/{rhs}"
                )))
            })
    }
}

impl FromNum<f64> for Duration {
    fn from_num(value: f64) -> Self {
        Duration::from_secs_f64(value)
    }
}

impl AbsoluteNum<f64> for Duration {}
