use crate::{
    num_check::{NumErr, NumResult},
    num_conv::TryIntoNum,
};
use std::sync::atomic::Ordering as AtomicOrdering;
use std::{
    cmp::Ordering,
    fmt::Debug,
    sync::atomic::{AtomicU16, AtomicU32, AtomicU64, AtomicUsize},
};
use std::{
    ops::{Add, Div, Mul, Sub},
    sync::atomic::AtomicU8,
};

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

pub trait CmpWithF64
where
    Self: TryIntoNum<f64>,
{
    fn cmp_f64(&self, required: Ordering, rhs: f64) -> NumResult<bool> {
        let lhs: f64 = self.try_into_num()?;
        lhs.partial_cmp(&rhs)
            .map(|ordering| ordering == required)
            .ok_or(NumErr::Other(format!("Could not compare {lhs} with {rhs}")))
    }
}

impl<T> CmpWithF64 for T where T: TryIntoNum<f64> {}

pub trait SignedNum
where
    Self: Num
        + FiniteTest
        + NonNegTest
        + Add<Self, Output = Self>
        + Mul<Self, Output = Self>
        + Sub<Self, Output = Self>
        + Div<Self, Output = Self>,
{
    fn positive(self) -> bool;
    fn negative(self) -> bool;
    fn signum(self) -> Self;
}

pub trait UnsignedNum
where
    Self: Num
        + FiniteTest
        + Add<Self, Output = Self>
        + Mul<Self, Output = Self>
        + Sub<Self, Output = Self>
        + Div<Self, Output = Self>,
{
    const ONE: Self;
    const ZERO: Self;
}

macro_rules! impl_unum {
    ($($t:ty),*) => {
        $(impl UnsignedNum for $t {
            const ONE: Self = 1;
            const ZERO: Self = 0;
        })*
    };
}

impl_unum!(u8, u32, usize);
