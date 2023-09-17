use std::cmp::Ordering;

use crate::num_display::NumDisplay;

use crate::num_check::NumErr;
use crate::{
    num::Num,
    num::SignedNum,
    num_check::NumResult,
    num_conv::{FromNum, TryIntoNum},
};

pub trait AbsoluteNum<Adaptor>
where
    Self: Num + DivUsize + FromNum<Adaptor> + NumDisplay,
    Adaptor: AdaptorNum<Self>,
{
}

pub trait TakeAbsolute<Absolute> {
    fn take_absolute(self) -> Absolute;
}

pub trait AdaptorNum<Absolute>
where
    Absolute: Num + DivUsize + FromNum<Self> + NumDisplay,
    Self: SignedNum + FromNum<Absolute> + TakeAbsolute<Absolute> + TryIntoNum<f64>,
{
}

impl<Absolute, Adaptor> TakeAbsolute<Absolute> for Adaptor
where
    Absolute: Num + DivUsize + FromNum<Self>,
    Self: SignedNum + FromNum<Absolute>,
{
    fn take_absolute(self) -> Absolute {
        Absolute::from_num(self.signum() * self)
    }
}

impl<Adaptor, Absolute> AdaptorNum<Absolute> for Adaptor
where
    Absolute: Num + DivUsize + FromNum<Self> + NumDisplay,
    Self: SignedNum + FromNum<Absolute> + TakeAbsolute<Absolute> + TryIntoNum<f64>,
{
}

pub trait DivUsize
where
    Self: Sized + Num,
{
    fn div_usize(&self, rhs: usize) -> NumResult<Self>;
}
