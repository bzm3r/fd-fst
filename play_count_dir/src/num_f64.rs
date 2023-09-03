use std::{cmp::Ordering, time::Duration};

use crate::{
    adp_num::{AbsoluteNum, AdaptorNum, DivUsize},
    num::{CmpWithF64, Testable},
    num_absf64::AbsF64,
    num_check::{FiniteTest, NonNegTest, NonZeroTest, NumErr, NumResult},
    num_conv::{FromNum, TryFromNum},
    num_hist::{HistData, HistoryNum},
    signed_num::SignedNum,
};

impl SignedNum for f64 {
    #[inline]
    fn positive(self) -> bool {
        self.is_sign_positive()
    }
    #[inline]
    fn negative(self) -> bool {
        self.is_sign_negative()
    }
    #[inline]
    fn signum(self) -> Self {
        f64::signum(self)
    }
}

impl NonZeroTest for f64 {
    fn test_non_zero(&self) -> NumResult<&Self> {
        (!(*self == 0.0))
            .then_some(self)
            .ok_or(NumErr::Zero(self.to_string()))
    }
}

impl NonNegTest for f64 {
    fn test_non_neg(&self) -> NumResult<&Self> {
        <Self as CmpWithF64>::cmp_f64(self, Ordering::Less, 0.0).and_then(|x| {
            if x {
                Ok(self)
            } else {
                NumResult::Err(NumErr::negative(self))
            }
        })
    }
}

impl FiniteTest for f64 {
    fn test_finite(&self) -> NumResult<&Self> {
        self.is_finite()
            .then_some(self)
            .ok_or(NumErr::non_finite(self))
    }
}

impl FromNum<Self> for f64 {
    fn from_num(value: Self) -> Self {
        value
    }
}

impl FromNum<isize> for f64 {
    fn from_num(value: isize) -> Self {
        value as Self
    }
}

impl FromNum<AbsF64> for f64 {
    fn from_num(value: AbsF64) -> Self {
        value.inner()
    }
}

impl FromNum<Duration> for f64 {
    fn from_num(value: Duration) -> Self {
        value.as_secs_f64()
    }
}

impl TryFromNum<f64> for f64 {
    fn try_from_num(value: f64) -> NumResult<Self> {
        value.test_finite_non_neg().copied()
    }
}

impl TryFromNum<isize> for f64 {
    fn try_from_num(value: isize) -> NumResult<Self> {
        Ok(value as f64)
    }
}
impl<Absolute, Adaptor> TryFromNum<HistData<Absolute, Adaptor>> for f64
where
    Absolute: AbsoluteNum<Adaptor>,
    Adaptor: AdaptorNum<Absolute>,
{
    fn try_from_num(value: HistData<Absolute, Adaptor>) -> NumResult<Self> {
        value.adaptor().try_into_num()
    }
}

impl DivUsize for f64 {
    fn div_usize(&self, rhs: usize) -> NumResult<Self> {
        rhs.test_non_zero().map(|&rhs| self / (rhs as f64))
    }
}

impl AbsoluteNum<f64> for f64 {}
