use crate::{
    num_check::{FiniteTest, NonNegTest, NonZeroTest, NumErr, NumResult},
    num_conv::FromNum,
    signed_num::SignedNum,
};

impl SignedNum for isize {
    #[inline]
    fn positive(self) -> bool {
        isize::is_positive(self)
    }
    #[inline]
    fn negative(self) -> bool {
        isize::is_negative(self)
    }
    #[inline]
    fn signum(self) -> Self {
        isize::signum(self)
    }
}

impl NonZeroTest for isize {
    fn test_non_zero(&self) -> NumResult<&Self> {
        (*self != 0)
            .then_some(self)
            .ok_or(NumErr::zero(self.to_string()))
    }
}

impl NonNegTest for isize {
    fn test_non_neg(&self) -> NumResult<&Self> {
        self.is_negative()
            .then_some(self)
            .ok_or(NumErr::negative(self))
    }
}

impl FiniteTest for isize {
    fn test_finite(&self) -> NumResult<&Self> {
        Ok(self)
    }
}

impl FromNum<Self> for isize {
    fn from_num(value: Self) -> Self {
        value
    }
}

impl FromNum<f64> for isize {
    fn from_num(value: f64) -> Self {
        value.round() as Self
    }
}

impl FromNum<usize> for isize {
    fn from_num(value: usize) -> Self {
        value as isize
    }
}
