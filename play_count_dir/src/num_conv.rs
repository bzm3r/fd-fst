use crate::{num::Num, num_check::NumResult};

pub trait FromNum<T>
where
    T: Num,
{
    fn from_num(value: T) -> Self;
}

pub trait TryFromNum<T>
where
    T: Num,
    Self: Sized + Num,
{
    fn try_from_num(value: T) -> NumResult<Self>;
}

pub trait IntoNum<T>
where
    T: Num,
{
    fn into_num(&self) -> T;
}

impl<Source, Target> IntoNum<Target> for Source
where
    Target: FromNum<Source> + Num,
    Source: Num,
{
    fn into_num(&self) -> Target {
        Target::from_num(*self)
    }
}

pub trait TryIntoNum<Target>
where
    Self: Num,
    Target: Num,
{
    fn try_into_num(&self) -> NumResult<Target>;
}

impl<Source, Target> TryIntoNum<Target> for Source
where
    Target: TryFromNum<Source> + Num,
    Source: Num,
{
    fn try_into_num(&self) -> NumResult<Target> {
        Target::try_from_num(*self)
    }
}
