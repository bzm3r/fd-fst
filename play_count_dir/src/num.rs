use crate::num_check::NumResult;
use std::fmt::Debug;

use crate::num_check::{FiniteTest, NonNegTest, NonZeroTest};

pub trait Num
where
    Self: Sized + Clone + Copy + Debug + Default + PartialOrd + PartialEq + Testable,
{
}

pub trait Testable: Clone + Debug + FiniteTest + NonNegTest + NonZeroTest {
    #[inline]
    fn test_all(&self) -> NumResult<&Self> {
        self.test_finite()
            .and_then(|n| n.test_non_neg().and_then(|n| n.test_non_zero()))
    }
}

impl<T> Num for T where Self: Clone + Copy + Debug + Default + PartialOrd + PartialEq + Testable {}

impl<T: Clone + Debug + FiniteTest + NonNegTest + NonZeroTest> Testable for T {}
