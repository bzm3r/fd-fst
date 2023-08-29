use crate::adp_num::*;
use crate::hist_num::HistData;
use crate::hist_num::HistoryNum;
use crate::history::HistoryVec;
use crate::num_check::*;
use crate::num_conv::*;
use crate::MAX_HISTORY;
use paste::paste;
use std::time::Duration;

auto_basic_num!(
    [Duration]
    (NonNeg:[trivial])
    (Finite:[trivial])
    (NonZero:[trivial])
);

auto_basic_num!(
    [usize]
    (NonNeg:[trivial])
    (Finite:[trivial])
    (NonZero:[trivial])
);

auto_basic_num!(
    [u32]
    (NonNeg:[trivial])
    (Finite:[trivial])
    (NonZero:[trivial])
);

auto_basic_num!(
    [isize]
    (NonNeg:[signed_num])
    (Finite:[trivial])
    (NonZero:[trivial])
);

auto_basic_num!(
    [f64]
    (NonNeg:[signed_num])
    (Finite:[|inp| { inp.is_finite().then_some(inp).ok_or(NumErr::non_finite(inp)) }])
    (NonZero:[trivial])
);

#[derive(Copy, Clone, Default, Debug, PartialEq, PartialOrd)]
pub struct AbsF64(f64);

impl AbsF64 {
    pub fn value(&self) -> f64 {
        self.0
    }
}

auto_basic_num!(
    [AbsF64]
    (NonNeg:[|inp| { inp.test_non_neg() } ])
    (Finite:[|inp| { inp.test_finite() }])
    (NonZero:[trivial])
);

impl TryFromNum<usize> for u32 {
    fn try_from_num(value: usize) -> NumResult<Self> {
        <u32 as TryFrom<usize>>::try_from(value).map_err(|err| NumErr::conversion(err))
    }
}

auto_from_num!([(f64, usize)] | inp | { inp.round() as usize });
auto_from_num!([(usize, f64)] primitive);
auto_try_from_num!([(usize, f64)]);

auto_try_from_num!([(isize, f64)]);

auto_abs_num!(
    [(usize, isize)]
    (div_usize:[trivial])
    (from_abs:[primitive])
    (from_adp:[primitive])
);

auto_abs_num!(
    [(Duration, f64)]
    (div_usize:[
        |lhs, rhs| {
            rhs
                .test_non_zero()
                .and_then(
                    |&rhs|
                        u32::try_from_num(rhs)
                            .and_then(|rhs| Ok(lhs / rhs) ))
                            .map_err(|err| NumErr::Other(format!("{err}"))
                )
        }
    ])
    (from_abs:[ |inp| { inp.as_secs_f64() } ])
    (from_adp:[ |inp| { Duration::from_secs_f64(inp) } ])
);

auto_abs_num!(
    [(AbsF64, f64)]
    (
        div_usize:[
            |lhs, rhs| {
                    rhs
                        .test_non_zero()
                        .and_then(
                            |&rhs|
                                u32::try_from_num(rhs)
                                    .and_then(|rhs| Ok(AbsF64(lhs.0 / rhs as f64)) ))
                                    .map_err(|err| NumErr::Other(format!("{err}"))
                        )
                }
        ]
    )
    (from_abs:[|inp| { inp.0 }])
    (from_adp:[|inp| { AbsF64(inp) }])
);
impl AbsoluteNum<f64> for f64 {}

impl DivUsize for f64 {
    fn div_usize(&self, rhs: usize) -> NumResult<Self> {
        rhs.test_non_zero().map(|&rhs| self / (rhs as f64))
    }
}

pub type TimeSpan = HistData<Duration, f64>;

impl FromNum<f64> for isize {
    fn from_num(value: f64) -> Self {
        value.test_finite().map(|&f| f as isize).unwrap()
    }
}

impl FromNum<isize> for f64 {
    fn from_num(value: isize) -> Self {
        value as f64
    }
}

pub type ProcessingRate = HistData<AbsF64, f64>;

