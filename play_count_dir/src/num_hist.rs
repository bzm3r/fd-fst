use std::cmp::Ordering;

use crate::{
    adp_num::{AbsoluteNum, AdaptorNum, DivUsize},
    display::CustomDisplay,
    num::Num,
    num_check::{FiniteTest, NonNegTest, NonZeroTest, NumResult},
    num_conv::{FromNum, TryIntoNum},
};

pub trait HistoryNum
where
    Self: Num + TryIntoNum<f64>,
{
    type Absolute: AbsoluteNum<Self::Adaptor>;
    type Adaptor: AdaptorNum<Self::Absolute>;

    fn new_same_sign(&self, abs_val: Self::Absolute) -> Self;
    fn adaptor(&self) -> Self::Adaptor;
    fn absolute(&self) -> Self::Absolute;
    fn try_adaptor_as_absolute(&self) -> NumResult<Self::Absolute> {
        Ok(InnerAbsolute::<Self>::from_num(
            self.adaptor()
                .test_finite()
                .and_then(|value| value.test_non_neg().copied())?,
        ))
    }
    fn from_adaptor(adaptor: Self::Adaptor) -> Self;
    fn from_absolute(absolute: Self::Absolute) -> Self;

    fn difference(&self, rhs: Self) -> Self {
        Self::from_adaptor(self.adaptor() - rhs.adaptor())
    }
    fn increment(&self, rhs: Self) -> Self {
        Self::from_adaptor(self.adaptor() + rhs.adaptor())
    }
    fn ratio(&self, rhs: Self) -> NumResult<f64> {
        self.adaptor()
            .try_into_num()
            .and_then(|l| rhs.adaptor().try_into_num().map(|r| l / r))
    }
    fn div_usize(&self, rhs: usize) -> NumResult<Self> {
        self.absolute()
            .div_usize(rhs)
            .map(|abs| Self::new_same_sign(&self, abs))
    }
    fn iter_sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::default(), |u, v| u.increment(v))
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct HistData<Absolute, Adaptor>
where
    Self: CustomDisplay,
    Absolute: AbsoluteNum<Adaptor>,
    Adaptor: AdaptorNum<Absolute>,
{
    adaptor: Adaptor,
    absolute: Absolute,
}

impl<Absolute, Adaptor> NonZeroTest for HistData<Absolute, Adaptor>
where
    Absolute: AbsoluteNum<Adaptor>,
    Adaptor: AdaptorNum<Absolute>,
{
    fn test_non_zero(&self) -> NumResult<&Self> {
        self.adaptor()
            .test_non_zero()
            .map(|_| self)
            .map_err(|err| err.map_info(|err| format!("{:?}: {}", self, err)))
    }
}

impl<Absolute, Adaptor> NonNegTest for HistData<Absolute, Adaptor>
where
    Absolute: AbsoluteNum<Adaptor>,
    Adaptor: AdaptorNum<Absolute>,
{
    fn test_non_neg(&self) -> NumResult<&Self> {
        self.adaptor()
            .test_non_neg()
            .map(|_| self)
            .map_err(|err| err.map_info(|err| format!("{:?}: {}", self, err)))
    }
}

impl<Absolute, Adaptor> FiniteTest for HistData<Absolute, Adaptor>
where
    Absolute: AbsoluteNum<Adaptor>,
    Adaptor: AdaptorNum<Absolute>,
{
    fn test_finite(&self) -> NumResult<&Self> {
        self.absolute()
            .test_finite()
            .map(|_| self)
            .map_err(|err| err.map_info(|err| format!("{:?}: {}", self, err)))
    }
}

impl<Absolute, Adaptor> FromNum<HistData<Absolute, Adaptor>> for Adaptor
where
    Absolute: AbsoluteNum<Adaptor>,
    Adaptor: AdaptorNum<Absolute>,
{
    fn from_num(value: HistData<Absolute, Adaptor>) -> Self {
        value.adaptor
    }
}

impl<Absolute, Adaptor> FromNum<Adaptor> for HistData<Absolute, Adaptor>
where
    Absolute: AbsoluteNum<Adaptor>,
    Adaptor: AdaptorNum<Absolute>,
{
    fn from_num(value: Adaptor) -> Self {
        Self {
            adaptor: value,
            absolute: value.take_absolute(),
        }
    }
}

pub type InnerAbsolute<Adapted> = <Adapted as HistoryNum>::Absolute;
// pub type InnerAdaptor<Adapted> = <Adapted as HistoryNum>::Adaptor;

impl<Absolute, Adaptor> HistoryNum for HistData<Absolute, Adaptor>
where
    Absolute: AbsoluteNum<Adaptor>,
    Adaptor: AdaptorNum<Absolute>,
{
    type Absolute = Absolute;

    type Adaptor = Adaptor;

    fn adaptor(&self) -> Self::Adaptor {
        self.adaptor
    }

    fn new_same_sign(&self, abs_val: Self::Absolute) -> Self {
        Self {
            adaptor: self.adaptor.signum() * Adaptor::from_num(abs_val),
            absolute: abs_val,
        }
    }

    fn absolute(&self) -> Self::Absolute {
        self.absolute
    }

    fn from_adaptor(adaptor: Self::Adaptor) -> Self {
        Self {
            adaptor,
            absolute: adaptor.take_absolute(),
        }
    }

    fn from_absolute(absolute: Self::Absolute) -> Self {
        Self {
            adaptor: Adaptor::from_num(absolute),
            absolute,
        }
    }
}

impl<Absolute, Adaptor> PartialEq for HistData<Absolute, Adaptor>
where
    Absolute: AbsoluteNum<Adaptor>,
    Adaptor: AdaptorNum<Absolute>,
{
    fn eq(&self, other: &Self) -> bool {
        self.adaptor.eq(&other.adaptor)
    }
}

impl<Absolute, Adaptor> PartialOrd for HistData<Absolute, Adaptor>
where
    Absolute: AbsoluteNum<Adaptor>,
    Adaptor: AdaptorNum<Absolute>,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.adaptor.partial_cmp(&other.adaptor)
    }
}
