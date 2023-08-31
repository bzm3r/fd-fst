use crate::{num_check::NumResult, num_conv::TryIntoNum};
use std::fmt::Debug;

use crate::num_check::{FiniteTest, NonNegTest, NonZeroTest};

pub trait Num
where
    Self: Sized + Clone + Copy + Debug + Default + PartialOrd + PartialEq + Testable,
{
}

pub trait Testable:
    Sized
    + Clone
    + Copy
    + Debug
    + Default
    + PartialOrd
    + PartialEq
    + FiniteTest
    + NonNegTest
    + NonZeroTest
{
    #[inline]
    fn test_all(&self) -> NumResult<&Self> {
        self.test_finite_non_neg().and_then(|n| n.test_non_zero())
    }

    fn test_finite_non_neg(&self) -> NumResult<&Self> {
        self.test_finite().and_then(|n| n.test_non_neg())
    }

    fn test_finite_non_zero(&self) -> NumResult<&Self> {
        self.test_finite().and_then(|n| n.test_non_zero())
    }

    fn test_finite_non_zero_then_convert<T: Num>(&self) -> NumResult<T>
    where
        Self: TryIntoNum<T>,
    {
        self.test_finite_non_zero().and_then(|v| v.try_into_num())
    }
}

impl<T> Num for T where Self: Clone + Copy + Debug + Default + PartialOrd + PartialEq + Testable {}

impl<
        T: Sized
            + Clone
            + Copy
            + Debug
            + Default
            + PartialOrd
            + PartialEq
            + FiniteTest
            + NonNegTest
            + NonZeroTest,
    > Testable for T
{
}
