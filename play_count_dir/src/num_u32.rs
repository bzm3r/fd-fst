use crate::{
    num_check::{FiniteTest, NonNegTest, NonZeroTest, NumErr, NumResult},
    num_conv::{FromNum, TryFromNum},
};

impl NonZeroTest for u32 {
    fn test_non_zero(&self) -> NumResult<&Self> {
        (*self != 0)
            .then_some(self)
            .ok_or(NumErr::zero(self.to_string()))
    }
}

impl NonNegTest for u32 {
    fn test_non_neg(&self) -> NumResult<&Self> {
        Ok(self)
    }
}

impl FiniteTest for u32 {
    fn test_finite(&self) -> NumResult<&Self> {
        Ok(self)
    }
}

impl FromNum<Self> for u32 {
    fn from_num(value: Self) -> Self {
        value
    }
}

impl TryFromNum<usize> for u32 {
    fn try_from_num(value: usize) -> NumResult<Self> {
        u32::try_from(value).map_err(|err| NumErr::conversion(err.to_string()))
    }
}
