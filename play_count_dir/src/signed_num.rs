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




