use crate::{
    adp_num::{AbsoluteNum, DivUsize},
    num_check::{FiniteTest, NonNegTest, NonZeroTest, NumErr, NumResult},
    num_conv::FromNum,
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

impl NonNegTest for AbsF64 {
    fn test_non_neg(&self) -> NumResult<&Self> {
        (!(self.inner() < 0.0))
            .then_some(self)
            .ok_or(NumErr::negative(self))
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
        self.inner().div_usize(rhs).map(|f| Self(f))
    }
}

impl AbsoluteNum<f64> for AbsF64 {}
