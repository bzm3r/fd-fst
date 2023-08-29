use std::ops::{Add, Div, Mul, Sub};

use crate::{
    num::Num,
    num_check::{FiniteTest, NonNegTest},
};

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
