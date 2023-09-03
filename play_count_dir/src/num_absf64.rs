use std::cmp::Ordering;

use crate::{
    adp_num::{AbsoluteNum, DivUsize},
    num::CmpWithF64,
    num_check::{FiniteTest, NonNegTest, NonZeroTest, NumErr, NumResult},
    num_conv::{FromNum, TryFromNum},
};

#[derive(Copy, Clone, Default, Debug, PartialEq, PartialOrd)]
pub struct AbsF64(f64);

impl AbsF64 {
    #[inline]
    pub fn inner(&self) -> f64 {
        self.0
    }
}

impl NonZeroTest for AbsF64 {
    fn test_non_zero(&self) -> NumResult<&Self> {
        (!(self.inner() == 0.0))
            .then_some(self)
            .ok_or(NumErr::zero(self))
    }
}

impl TryFromNum<AbsF64> for f64 {
    fn try_from_num(value: AbsF64) -> NumResult<Self> {
        value.inner().test_finite().copied()
    }
}

impl NonNegTest for AbsF64 {
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

impl FiniteTest for AbsF64 {
    fn test_finite(&self) -> NumResult<&Self> {
        self.0
            .is_finite()
            .then_some(self)
            .ok_or(NumErr::non_finite(self))
    }
}

impl FromNum<Self> for AbsF64 {
    fn from_num(value: Self) -> Self {
        value
    }
}

impl FromNum<f64> for AbsF64 {
    fn from_num(value: f64) -> Self {
        Self(value)
    }
}

impl DivUsize for AbsF64 {
    fn div_usize(&self, rhs: usize) -> NumResult<Self> {
        self.inner().div_usize(rhs).map(Self)
    }
}

impl AbsoluteNum<f64> for AbsF64 {}
