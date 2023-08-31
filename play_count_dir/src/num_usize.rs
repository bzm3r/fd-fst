use crate::{
    adp_num::{AbsoluteNum, DivUsize},
    num_check::{FiniteTest, NonNegTest, NonZeroTest, NumErr, NumResult},
    num_conv::FromNum,
};

impl NonZeroTest for usize {
    fn test_non_zero(&self) -> NumResult<&Self> {
        (*self != 0).then_some(self).ok_or(NumErr::zero(self))
    }
}

impl NonNegTest for usize {
    fn test_non_neg(&self) -> NumResult<&Self> {
        Ok(self)
    }
}

impl FiniteTest for usize {
    fn test_finite(&self) -> NumResult<&Self> {
        Ok(self)
    }
}

impl FromNum<Self> for usize {
    fn from_num(value: Self) -> Self {
        value
    }
}

impl FromNum<f64> for usize {
    fn from_num(value: f64) -> Self {
        value.round() as Self
    }
}

impl FromNum<isize> for usize {
    fn from_num(value: isize) -> Self {
        value as Self
    }
}

impl DivUsize for usize {
    fn div_usize(&self, rhs: usize) -> NumResult<Self> {
        rhs.test_non_zero().map(|rhs| self / rhs)
    }
}

impl AbsoluteNum<isize> for usize {}
