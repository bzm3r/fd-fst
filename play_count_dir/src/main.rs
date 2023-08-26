// #![cfg(feature = "debug_macros")]
#![feature(trace_macros)]
trace_macros!(true);

use paste::paste;
use std::cmp::Ordering;
use std::error::Error;
use std::fmt::Result as FmtRes;
use std::fmt::{Debug, Display};
use std::fs::{read_dir, DirEntry};
use std::io::Error as IoErr;
use std::ops::{Add, Deref, Div, Mul, Sub};
use std::path::PathBuf;
use std::sync::mpsc::{self, TryRecvError};
use std::sync::mpsc::{Receiver, SendError, Sender};
use std::sync::{Arc, RwLock, RwLockReadGuard};
use std::thread::JoinHandle;
use std::thread::{self, sleep};
use std::time::{Duration, Instant};
#[macro_use]
mod macro_tools;

const NUM_THREADS: usize = 8;
const DEFAULT_EXECUTE_LOOP_SLEEP: Duration = Duration::from_micros(10);
const UPDATE_PRINT_DELAY: Duration = Duration::from_secs(5);
const DEFAULT_WORK_CHUNK_SIZE: usize = 64;
// const MAX_TOTAL_SUBMISSION: usize = NUM_THREADS * 500;
const MAX_HISTORY: usize = 10;
// const MIN_TARGET_SIZE: usize = 1;
// const MAX_DISPATCH_SIZE: usize = 2_usize.pow(6);
// const MAX_TOTAL_DISPATCH_SIZE: usize = if (NUM_THREADS * 10_usize.pow(4)) < (6 * 10_usize.pow(4)) {
//     ((NUM_THREADS * 10_usize.pow(4)) as f64 * 1e-2) as usize
// } else {
//     ((NUM_THREADS * 10_usize.pow(4)) as f64 * 1e-2) as usize
//     //((6 * 10_usize.pow(4)) as f64 * 1e-2) as usize
// };
// const MAX_IN_FLIGHT: usize = MAX_DISPATCH_SIZE * 2_usize.pow(10);
// const PESSIMISTIC_PROCESSING_RATE_ESTIMATE: f64 = 1e3;

macro_rules! with_num_err_vars(
    ($macro_id:ident $(; $($args:tt)+)?) => {
        $macro_id!(variants:[Negative, NonFinite, IsZero, Conversion, Other] $(; $($args)+)? );
    }
);

macro_rules! num_err_enum {
    (variants:[$($variant:ident),*]) => {
        #[derive(Clone, Debug)]
        enum NumErr {
            $($variant(String)),*
        }

        impl Error for NumErr {}

        impl NumErr {
            fn info(&self) -> &str {
                match self {
                    $(NumErr::$variant(x) => x,)*
                }
            }

            fn map_info<F: FnOnce(&str) -> String>(&self, f: F) -> NumErr {
                match self {
                    $(NumErr::$variant(x) => NumErr::$variant(f(x)),)*
                }

            }

            paste! {
                $(fn [< $variant:snake:lower >]<T: Display>(n: T) -> Self { Self::$variant(format!("{}", n)) })*
            }
        }
    };
}
with_num_err_vars!(num_err_enum);

macro_rules! num_err_display {
    (variants:[$($variant:ident),*]) => {
        impl Display for NumErr {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> FmtRes {
                write!(f, "{}({:?})", match self {
                    $(NumErr::$variant(x) => stringify!($variant),)*
                }, self.info())
            }
        }
    };
}
with_num_err_vars!(num_err_display);

macro_rules! num_err_new_methods {
    (variants:[$($variant:ident),*]) => {
        paste! {
            $(
                fn [< new_ $variant >](info: T) -> Self {
                    Self::$variant(info)
                }
            )*
        }
    };
}

// trait NumErr: Sized + Display {
//     type Data;
//     type MapOutput;
//     fn map<Map: FnOnce(Self::Data) -> DataTarget, DataTarget: Clone + Debug>(
//         self,
//         f: Map,
//     ) -> Self::MapOutput<DataTarget>;
//     with_num_err_vars!(num_err_new_methods);
// }

// macro_rules! num_err_structs {
//     (@gen_struct $struct_id:ident) => {
//         #[derive(Clone, Debug)]
//         struct $struct_id<T: Clone + Debug> {
//             ty: NumErrType,
//             info: T,
//         }

//         impl<T: Clone + Debug> NumErr for $struct_id<T> {
//             type Data = T;
//             type MapOutput<U: Clone + Debug> = $struct_id<U>;

//             fn new(ty: NumErrType, info: T) -> Self {
//                 Self { ty, info }
//             }

//             fn map<Map: FnOnce(T) -> Target, Target: Clone + Debug>(self, f: Map) -> $struct_id<Target> {
//                 $struct_id {
//                     ty: self.ty,
//                     info: f(self.info),
//                 }
//             }
//         }

//         impl<T: Clone + Debug> Display for $struct_id<T> {
//             fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> FmtRes {
//                 write!(f, "{}({:?})", self.ty, self.info)
//             }
//         }
//     };
//     (variants:[$($variant:ident),*]) => {
//         paste!{
//             $(num_err_structs!(@gen_struct [< $variant Err >]);)*
//         }
//     };
// }
// with_num_err_vars!(num_err_structs);

// #[derive(Clone, Debug)]
// struct NumErr<T: Clone + Debug> {
//     ty: NumErrType,
//     info: T,
// }

// impl<T: Clone + Debug> NumErr for NumErr<T> {
//     type Data = T;
//     type MapOutput<U: Clone + Debug> = NumErr<U>;

//     fn new(ty: NumErrType, info: T) -> Self {
//         Self { ty, info }
//     }

//     fn map<Map: FnOnce(T) -> Target, Target: Clone + Debug>(self, f: Map) -> NumErr<Target> {
//         NumErr {
//             ty: self.ty,
//             info: f(self.info),
//         }
//     }
// }

// impl<T: Clone + Debug> Display for NumErr<T> {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> FmtRes {
//         write!(f, "{}({:?})", self.ty, self.info)
//     }
// }

type NumResult<T> = Result<T, NumErr>;

trait RoundSigFigs: BasicNum + TryFromNum<f64> + TryIntoNum<f64> {
    fn from_f64(f: f64) -> NumResult<Self>;
    fn into_f64(self) -> NumResult<f64>;

    fn delta(x: f64) -> Option<i32> {
        let f = x.abs().log10().ceil();
        f.is_finite().then_some(f as i32)
    }

    fn round_sig_figs(&self, n_sig_figs: i32) -> Self {
        let x: f64 = self.into_f64().unwrap();
        Self::from_f64(if x == 0. || n_sig_figs == 0 {
            0.0_f64
        } else {
            if let Some(delta) = Self::delta(x) {
                let shift = n_sig_figs - delta;
                let shift_factor = 10_f64.powi(shift);
                (x * shift_factor).round() / shift_factor
            } else {
                0.0_f64
            }
        })
        .unwrap()
    }
}

impl<T> RoundSigFigs for T
where
    T: BasicNum + TryFromNum<f64> + TryIntoNum<f64>,
{
    fn from_f64(f: f64) -> NumResult<T> {
        Self::try_from_num(f)
    }

    fn into_f64(self) -> NumResult<f64> {
        self.try_into_num()
    }
}

#[derive(PartialEq, Eq, Copy, Clone)]
enum Status {
    Busy,
    Idle,
}

struct BufferedSender<T> {
    buffer: Vec<T>,
    sender: Sender<Vec<T>>,
}

impl<T> BufferedSender<T> {
    fn new(sender: Sender<Vec<T>>, initial_capacity: usize) -> Self {
        BufferedSender {
            buffer: Vec::with_capacity(initial_capacity),
            sender,
        }
    }
    fn push(&mut self, value: T) {
        self.buffer.push(value)
    }

    fn flush_send(&mut self) -> Result<(), SendError<Vec<T>>> {
        self.sender.send(self.buffer.drain(0..).collect())
    }
}

#[derive(Default)]
struct Printer(Option<BufferedSender<String>>);

impl Printer {
    fn new(sender: Option<Sender<Vec<String>>>) -> Self {
        if let Some(sender) = sender {
            Printer(Some(BufferedSender::new(sender, 20)))
        } else {
            Printer(None)
        }
    }

    fn push(&mut self, lazy_value: impl FnOnce() -> String) {
        self.0
            .as_mut()
            .map(|buf_sender| buf_sender.push(lazy_value()))
            .unwrap_or(());
    }

    fn flush_send(&mut self) -> Result<(), SendError<Vec<String>>> {
        let result = self
            .0
            .as_mut()
            .map(|buf_sender| buf_sender.flush_send())
            .unwrap_or(Ok(()));
        result
    }
}

trait CustomDisplay {
    fn custom_display(&self) -> String;
}

impl CustomDisplay for f64 {
    fn custom_display(&self) -> String {
        format!("{:e}", self.round_sig_figs(4))
    }
}

impl CustomDisplay for Duration {
    fn custom_display(&self) -> String {
        format!("{}s", self.as_secs_f64().custom_display())
    }
}

impl CustomDisplay for usize {
    fn custom_display(&self) -> String {
        format!("{self}")
    }
}

impl<T> CustomDisplay for Option<T>
where
    T: CustomDisplay,
{
    fn custom_display(&self) -> String {
        if let Some(inner) = self {
            inner.custom_display()
        } else {
            format!("None")
        }
    }
}

trait BasicNum
where
    Self: Sized + Clone + Copy + Debug + Default + PartialOrd + PartialEq + BasicTests,
{
}

impl<T> BasicNum for T where
    Self: Clone + Copy + Debug + Default + PartialOrd + PartialEq + BasicTests
{
}

trait BasicTests: Clone + Debug + FiniteTest + NonNegTest + NonZeroTest {
    #[inline]
    fn test_all(&self) -> NumResult<&Self> {
        self.test_finite()
            .and_then(|n| n.test_non_neg().and_then(|n| n.test_non_zero()))
    }
}

impl<T: Clone + Debug + FiniteTest + NonNegTest + NonZeroTest> BasicTests for T {}

trait SignedNum
where
    Self: BasicNum
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

trait DivUsize
where
    Self: Sized + BasicNum,
{
    fn div_usize(&self, rhs: usize) -> NumResult<Self>;
}

trait FromNum<T>
where
    T: BasicNum,
{
    fn from_num(value: T) -> Self;
}

trait TryFromNum<T>
where
    T: BasicNum,
    Self: Sized + BasicNum,
{
    fn try_from_num(value: T) -> NumResult<Self>;
}

trait IntoNum<T>
where
    T: BasicNum,
{
    fn into_num(&self) -> T;
}

impl<Source, Target> IntoNum<Target> for Source
where
    Target: FromNum<Source> + BasicNum,
    Source: BasicNum,
{
    fn into_num(&self) -> Target {
        Target::from_num(*self)
    }
}

trait TryIntoNum<Target>
where
    Self: BasicNum,
    Target: BasicNum,
{
    fn try_into_num(&self) -> NumResult<Target>;
}

impl<Source, Target> TryIntoNum<Target> for Source
where
    Target: TryFromNum<Source> + BasicNum,
    Source: BasicNum,
{
    fn try_into_num(&self) -> NumResult<Target> {
        Target::try_from_num(*self)
    }
}

macro_rules! map_enum_inner {
    ($enum:ident, $enum_var:ident, (|$match_var:ident : $match_ty:ty| -> $out_ty:ty { $body:expr }) ($($variant:ident),*)) => {
        match $enum_var {
            $($enum::$variant(x) => {
                (|$match_var: $match_ty| -> $out_ty { $body })(x)
            }),*
        }
    }
}

trait FiniteTest: Sized {
    fn test_finite(&self) -> NumResult<&Self>;
}

trait NonNegTest: Sized {
    fn test_non_neg(&self) -> NumResult<&Self>;
}

trait NonZeroTest: Sized {
    fn test_non_zero(&self) -> NumResult<&Self>;
}

macro_rules! standardize_num_auto_impl_args {
    (
        [$macro_id:ident]
        [
            $self:ident$(<$($self_params:ident),+>)?
            $( where $($where_args:tt)+)?
        ]
        $($rest:tt)*
    ) => {
        $macro_id!(
            @standardized
            [
                (
                    Self,
                    $self$(<$($self_params),+>)?
                )$(<$($self_params)+,>)?
                $( where $($where_args)+)?
            ]
            $($rest)*
        );
    };
    (
        [$macro_id:ident]
        [
            (
                $source:ident$(<$($source_params:tt)*>)?,
                $target:ident$(<$($target_params:ident),+>)?
            )$(<$($impl_args:tt)+>)?
            $( where $($where_args:tt)+)?
        ]
        $($rest:tt)*
    ) => {
        $macro_id!(
            @standardized
            [
                (
                    $source$(<$($source_params),+>)?,
                    $target$(<$($target_params),+>)?
                )$(<$($source_params)+,$($target_params)+,>)?
                $( where $($where_args)+)?
            ]
            $($rest)*
        );
    };
}

macro_rules! auto_from_num {
    (
        [
            $($type_info:tt)*
        ]
        $($rest:tt)*
    ) => {
        standardize_num_auto_impl_args!(
            [auto_from_num]
            [
                $($type_info)*
            ]
            $($rest)*
        );
    };
    (
        @standardized
        [
            (
                $source:ident$(<$($source_params:ident),+>)?,
                $target:ident$(<$($target_params:ident),+>)?
            )$(<$($impl_args:ident)+,>)?
            $( where $($where_args:tt)+)?
        ]
        trivial
    ) => {
        impl$(<$($impl_args),+>)?
            FromNum<$source$(<$($source_params),+>)?>
                for
            $target$(<$($target_params),+>)?
        $(where $($where_args)+)? {
            #[inline]
            fn from_num(value: $source$(<$($source_params),+>)?) -> Self {
                value
            }
        }
    };
    (
        @standardized
        [
            (
                $source:ident$(<$($source_params:ident),+>)?,
                $target:ident$(<$($target_params:ident),+>)?
            )$(<$($impl_args:ident)+,>)?
            $( where $($where_args:tt)+)?
        ]
        primitive
    ) => {
        impl$(<$($impl_args),+>)?
            FromNum<$source$(<$($source_params),+>)?>
                for
            $target$(<$($target_params)+>)?
        $(where $($where_args)+)? {
            #[inline]
            fn from_num(value: $source$(<$($source_params),+>)?) -> Self {
                value as Self
            }
        }
    };
    (
        @standardized
        [
            (
                $source:ident$(<$($source_params:ident),+>)?,
                $target:ident$(<$($target_params:ident),+>)?
            )$(<$($impl_args:ident)+,>)?
            $( where $($where_args:tt)+)?
        ]
        |$inp:ident| { $body:expr }
    ) => {
        impl$(<$($impl_args),+>)?
            FromNum<$source$(<$($source_params),+>)?>
                for
            $target$(<$($target_params)+>)?
        $(where $($where_args)+)? {
            #[inline]
            fn from_num(value: $source$(<$($source_params),+>)?) -> Self {
                (|$inp: $source| -> Self { $body })(value)
            }
        }
    };
}

macro_rules! auto_try_from_num {
    (
        @standardized
        [
            (
                $source:ident$(<$($source_params:ident),+>)?,
                $target:ident$(<$($target_params:ident),+>)?
            )$(<$($impl_args:ident)+,>)?
            $( where $($where_args:tt)+)?
        ]
        |$inp:ident| { $body:expr }
    ) => {
        impl$(<$($impl_args),+>)?
            TryFromNum<$source$(<$($source_params),+>)?>
                for
            $target$(<$($target_params),+>)?
        $(where $($where_args)+)? {
            #[inline]
            fn try_from_num(source: $source$(<$($source_params),+>)?) -> NumResult<Self> {
                (
                    |   $inp: $source$(<$($source_params),+>)?  | -> $target$(<$($target_params),+>)? {
                        $body:expr
                    }
                )(source)
            }
        }
    };
    (
        @standardized
        [
            (
                $source:ident$(<$($source_params:ident),+>)?,
                $target:ident$(<$($target_params:ident),+>)?
            )$(<$($impl_args:ident)+,>)?
            $( where $($where_args:tt)+)?
        ]
    ) => {
        impl$(<$($impl_args),+>)?
            TryFromNum<$source$(<$($source_params),+>)?>
                for
            $target$(<$($target_params),+>)?
        $(where $($where_args)+)? {
            #[inline]
            fn try_from_num(source: $source$(<$($source_params),+>)?) -> NumResult<Self> {
                source
                    .test_all()
                    .map(|&s| <Self as FromNum<$source$(<$($source_params),+>)?>>::from_num(s))
            }
        }
    };
    (
        [
            (
                $source:ident$(<$($source_params:ident),+>)?,
                $target:ident$(<$($target_params:ident),+>)?
            )$(<$($impl_args:ident)+,>)?
            $( where $($where_args:tt)+)?
        ] $($rest:tt)*
    ) => {
        standardize_num_auto_impl_args!(
            [auto_try_from_num]
            [
                (
                    $source$(<$($source_params),+>)?,
                    $target$(<$($target_params),+>)?
                )$(<$($impl_args)+,>)?
                $( where $($where_args)+)?
            ]
            $($rest)*
        );
    };
    (
        $($type_info:tt)*
    ) => {
        standardize_num_auto_impl_args!(
            [auto_try_from_num]
            [ $($type_info)* ]
        );
    };
}

macro_rules! auto_test {
    (
        [
            $($type_info:tt)*
        ]
        $($rest:tt)*
    ) => {
        standardize_num_auto_impl_args!(
            [auto_test]
            [ $($type_info)* ]
            $($rest)*
        );
    };
    (
        @standardized
        [
            $($type_info:tt)*
        ]
        NonNeg: [signed_num]
    ) => {
        auto_test!(
            @standardized
            [$($type_info)*]
            NonNeg:[
                |inp| {
                    (!inp.negative()).then_some(inp).ok_or(NumErr::negative(inp))
                }
            ]
        );
    };
    (
        @standardized
        [
            (
                $ignore:ident$(<$($ignore_params:ident),+>)?,
                $self:ident$(<$($params:ident),+>)?
            )$(<$($impl_args:ident)+,>)?
            $( where $($where_args:tt)+)?
        ]
        $test:ident : [trivial]
    ) => {
        paste! {
            impl$(<$($impl_args),+>)?
                [< $test Test >]
                    for
                $self$(<$($params),+>)?
            $(where $($where_args)+)?
            {
                #[inline]
                fn [< test_ $test:snake:lower >](&self) -> NumResult<&Self> {
                    Ok(&self)
                }
            }
        }
    };
    (
        @standardized
        [
            (
                $ignore:ident$(<$($ignore_params:ident),+>)?,
                $self:ident$(<$($params:ident),+>)?
            )$(<$($impl_args:ident)+,>)?
            $( where $($where_args:tt)+)?
        ]
        $test:ident : [|$inp:ident| { $body:expr }]
    ) => {
        paste! {
            impl$(<$($impl_args),+>)?
                [< $test Test >]
                    for
                $self$(<$($params),+>)?
            $(where $($where_args)+)?
            {
                #[inline]
                fn [< test_ $test:snake:lower >]<'a>(&'a self) -> NumResult<&'a Self> {
                    (|$inp: &'a $self| -> NumResult<&'a Self> { $body })(self)
                }
            }
        }
    };
}

macro_rules! multi_auto_test {
    (
        [
            $($ty_info:tt)*
        ]
        $($args:tt)*
    ) => {
        standardize_num_auto_impl_args!(
            [multi_auto_test]
            [
                $($ty_info)*
            ]
            $($args)*
        )
    };
    (
        @standardized
        [
            $($ty_info:tt)*
        ]
    ) => {};
    (
        @standardized
        [
            $($ty_info:tt)*
        ]
        ($test:ident : [$($args:tt)*])
        $($rest:tt)*
    ) => {
        auto_test!(
            @standardized
            [
                $($ty_info)*
            ]
            $test:[$($args)*]
        );
        multi_auto_test!(@standardized [$($ty_info)*] $($rest)*);
    };
}

macro_rules! auto_basic_num {
    (
        @standardized
        [
            $($ty_info:tt)*
        ]
        $(($test:ident:[$($args:tt)*]))*
    ) => {
        auto_from_num!(
            @standardized
            [
                $($ty_info)*
            ]
            trivial
        );
        auto_try_from_num!(
            @standardized
            [
                $($ty_info)*
            ]
        );
        multi_auto_test!(
            @standardized
            [
                $($ty_info)*
            ]
            $(($test:[$($args)*]))*
        );
    };
    (
        [
            $($ty_info:tt)*
        ]
        $($rest:tt)*

    ) => {
        standardize_num_auto_impl_args!(
            [ auto_basic_num ]
            [
                $($ty_info)*
            ]
            $($rest)*
        );
    };
}

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

auto_basic_num!(
    [AbsF64]
    (NonNeg:[|inp| { inp.test_non_neg() } ])
    (Finite:[|inp| { inp.test_finite() }])
    (NonZero:[trivial])
);

trait TakeAbsolute<Absolute> {
    fn take_absolute(self) -> Absolute;
}

trait AbsoluteNum<Adaptor>
where
    Self: BasicNum + DivUsize + FromNum<Adaptor>,
    Adaptor: AdaptorNum<Self>,
{
}

trait AdaptorNum<Absolute>
where
    Absolute: BasicNum + DivUsize + FromNum<Self>,
    Self: SignedNum + FromNum<Absolute> + TakeAbsolute<Absolute>,
{
}

impl<Absolute, Adaptor> TakeAbsolute<Absolute> for Adaptor
where
    Absolute: BasicNum + DivUsize + FromNum<Self>,
    Self: SignedNum + FromNum<Absolute>,
{
    fn take_absolute(self) -> Absolute {
        Absolute::from_num(self.signum() * self)
    }
}

impl<Adaptor, Absolute> AdaptorNum<Absolute> for Adaptor
where
    Absolute: BasicNum + DivUsize + FromNum<Self>,
    Self: SignedNum + FromNum<Absolute> + TakeAbsolute<Absolute>,
{
}

impl TryFromNum<usize> for u32 {
    fn try_from_num(value: usize) -> NumResult<Self> {
        <u32 as TryFrom<usize>>::try_from(value).map_err(|err| NumErr::conversion(err))
    }
}

#[derive(Copy, Clone, Default, Debug, PartialEq, PartialOrd)]
struct AbsF64(f64);

macro_rules! auto_abs_num {
    (
        @standardized
        [
            (
                $abs:ident$(<$($abs_params:ident),+>)?,
                $adp:ident$(<$($adp_params:ident),+>)?
            )$(<$($impl_args:ident)+,>)?
            $( where $($where_args:tt)+)?
        ]
    ) => {
        impl$(<$($impl_args)+,>)?
            AbsoluteNum<$adp$(<$($adp_params),+>)?>
                for
            $abs$(<$($abs_params),+>)?
            $( where $($where_args:tt)+)?
        {}
    };
    (
        @standardized
        [
            (
                $abs:ident$(<$($abs_params:ident),+>)?,
                $adp:ident$(<$($adp_params:ident),+>)?
            )$(<$($impl_args:ident)+,>)?
            $( where $($where_args:tt)+)?
        ]
        (div_usize:[$($args:tt)*]) $($rest:tt)*
    ) => {
        auto_div_usize!(
            [
                $abs$(<$($abs_params),+>)?
                $( where $($where_args)+)?
            ]
            $($args)*
        );
        auto_abs_num!(
            @standardized
            [
                (
                    $abs$(<$($abs_params),+>)?,
                    $adp$(<$($adp_params),+>)?
                )$(<$($impl_args)+,>)?
                $( where $($where_args)+)?
            ]
            $($rest)*
        );
    };
    (
        @standardized
        [
            $($try_info:tt)*
        ]
        (from_abs:[$($args:tt)*]) $($rest:tt)*
    ) => {
        auto_from_num!(
            @standardized
            [
                $($try_info)*
            ]
            $($args)*
        );
        auto_abs_num!(
            @standardized
            [
                $($try_info)*
            ]
            $($rest)*
        );
    };
    (
        @standardized
        [
            (
                $abs:ident$(<$($abs_params:ident),+>)?,
                $adp:ident$(<$($adp_params:ident),+>)?
            )$(<$($impl_args:ident)+,>)?
            $( where $($where_args:tt)+)?
        ]
        (from_adp:[$($args:tt)*]$(, try_from_override:[$($closure:tt)+])?)
        $($rest:tt)*
    ) => {
        auto_from_num!(
            @standardized
            [
                (
                    $adp$(<$($adp_params),+>)?,
                    $abs$(<$($abs_params),+>)?
                )$(<$($impl_args)+,>)?
                $( where $($where_args)+)?
            ]
            $($args)*
        );
        auto_try_from_num!(
            @standardized
            [
                (
                    $adp$(<$($adp_params),+>)?,
                    $abs$(<$($abs_params),+>)?
                )$(<$($impl_args)+,>)?
                $( where $($where_args)+)?
            ]
            $($($closure)+)?
        );
        auto_abs_num!(
            @standardized
            [
                (
                    $abs$(<$($abs_params),+>)?,
                    $adp$(<$($adp_params),+>)?
                )$(<$($impl_args)+,>)?
                $( where $($where_args)+)?
            ]
            $($rest)*
        );
    };
    (
        @standardized
        [
            $($type_info:tt)*
        ]
    ) => {};
    (
        [
            $($type_info:tt)*
        ]
        $($args:tt)+
    ) => {
        standardize_num_auto_impl_args!(
            [auto_abs_num]
            [$($type_info)*] $($args)+
        );
    };
}

macro_rules! nested_try_from {
    ( @internal [$(($inps:ident))*] [$closure:expr]) => {
        Ok(($closure)($($inps),*))
    };
    ( @internal [$($tracked_ids:tt)*] ($id:ident -> $target:ty) $($rest:tt)*) => {
        nested_try_from!{
            @internal [$($tracked_ids)*] ($id:($id) -> $target) $($rest)*
        }
    };
    ( @internal [$($tracked_ids:tt)*] ($id:ident:($val_expr:expr) -> $target:ty) $($rest:tt)* ) => {
        <$target>::try_from_num($val_expr).and_then(|$id| nested_try_from!{
            @internal
            [$($tracked_ids)* ($id)]
            $($rest)*
        })

    };
    ( ($id:ident$(:($val_expr:expr))? -> $target:ty) $($rest:tt)* ) => {
        nested_try_from!(@internal [] ($id$(:($val_expr))? -> $target) $($rest)* )
    };
}

auto_from_num!([(f64, usize)] | inp | { inp.round() as usize });
auto_from_num!([(usize, f64)] primitive);
auto_try_from_num!([(usize, f64)]);

macro_rules! auto_div_usize {
    (
        [
            $this:ident$(<$($params:ident),+>)?
            $( where $($where_args:tt)+)?
        ]
        |$lhs:ident, $rhs:ident| { $body:expr }
    ) => {
        impl$(<$($params),+>)? DivUsize for $this$(<$($params),+>)? $( where $($where_args)+)? {
            fn div_usize(&self, rhs: usize) -> NumResult<Self> {
                (|$lhs : Self, $rhs : usize| -> NumResult<Self> { $body })(*self, rhs)
            }
        }
    };
    (
        [
            $this:ident$(<$($params:ident),+>)?
            $( where $($where_args:tt)+)?
        ]
        trivial
    ) => {
        impl$(<$($params),+>)? DivUsize for $this$(<$($params),+>)? $( where $($where_args)+)? {
            fn div_usize(&self, rhs: usize) -> NumResult<Self> {
                nested_try_from!(
                    (lhs:(*self) -> f64)
                    (rhs:(rhs) -> f64)
                    [
                        |lhs: f64, rhs:f64| -> Self
                        {
                            <$this$(<$($params),+>)?>::from_num((lhs/rhs).round())
                        }
                    ]
                )
            }
        }
    };
}

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

// impl FromNum<AbsF64> for f64 {
//     #[inline]
//     fn from_num(value: AbsF64) -> Self {
//         value.0
//     }
// }

// impl AbsoluteNum<f64> for AbsF64 {}
// impl FromNum<f64> for AbsF64 {
//     fn from_num(value: f64) -> Self {
//         Self(value)
//     }
// }
impl CustomDisplay for AbsF64 {
    fn custom_display(&self) -> String {
        self.0.custom_display()
    }
}

// impl DivUsize for AbsF64 {
//     fn div_usize(&self, rhs: usize) -> NumResult<Self, usize> {
//         rhs.test_non_zero()
//             .and_then(|rhs| (self.0 / rhs as f64).try_into_num())
//             .or_else(|_| Ok(Self::from(0.0)))
//     }
// }

impl AbsoluteNum<f64> for f64 {}

trait AdaptedNum
where
    Self: BasicNum + TryIntoNum<f64>,
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
                .and_then(|value| value.test_non_neg())?,
        ))
    }
    fn from_adaptor(adaptor: Self::Adaptor) -> Self;
    fn from_absolute(absolute: Self::Absolute) -> Self;
}

type InnerAbsolute<Adapted> = <Adapted as AdaptedNum>::Absolute;
type InnerAdaptor<Adapted> = <Adapted as AdaptedNum>::Adaptor;

impl<Absolute, Adaptor> CustomDisplay for Adapted<Absolute, Adaptor>
where
    Absolute: AbsoluteNum<Adaptor> + CustomDisplay,
    Adaptor: AdaptorNum<Absolute>,
{
    fn custom_display(&self) -> String {
        self.absolute().custom_display()
    }
}

trait HistoryNum
where
    Self: AdaptedNum,
{
    fn difference(&self, rhs: Self) -> Self {
        Self::from_adaptor(self.adaptor() - rhs.adaptor())
    }
    fn increment(&self, rhs: Self) -> Self {
        Self::from_adaptor(self.adaptor() + rhs.adaptor())
    }
    fn ratio(&self, rhs: Self) -> Option<f64> {
        if rhs.absolute() > InnerAbsolute::<Self>::default() {
            let lhs_as_f64: f64 = self.adaptor().into_num();
            let rhs_as_f64: f64 = rhs.adaptor().into_num();
            Some(lhs_as_f64 / rhs_as_f64)
        } else {
            None
        }
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

impl<T> HistoryNum for T where Self: AdaptedNum {}

#[derive(Copy, Clone, Debug, Default)]
struct Adapted<Absolute, Adaptor>
where
    Absolute: AbsoluteNum<Adaptor>,
    Adaptor: AdaptorNum<Absolute>,
{
    adaptor: Adaptor,
    absolute: Absolute,
}

impl<Absolute, Adaptor> FromNum<Adapted<Absolute, Adaptor>> for Adaptor
where
    Absolute: AbsoluteNum<Adaptor>,
    Adaptor: AdaptorNum<Absolute>,
{
    fn from_num(value: Adapted<Absolute, Adaptor>) -> Self {
        value.adaptor
    }
}
// multi_auto_test!(
//     [Adapted<Absolute, Adaptor> where
//     Absolute: AbsoluteNum<Adaptor>,
//     Adaptor: AdaptorNum<Absolute>,]
// (NonNeg:[|inp| { (!inp.adaptor().is_negative()).then_some(inp).ok_or(NumErr::Negative(inp)) }])
// (NonZero:[|inp| { !inp.absolute().test_non_zero().map_err(|err| err.replace_inner(inp)) }])
// (Finite:[|inp| { inp.adaptor().test_finite().map_err(|err| err.replace_inner(inp)) } ])
// );
auto_basic_num!(
    [
        Adapted<Absolute, Adaptor>
            where
                Absolute: AbsoluteNum<Adaptor>,
                Adaptor: AdaptorNum<Absolute>,
    ]
    (NonNeg:[|inp| { (!inp.adaptor().is_negative()).then_some(inp).ok_or(NumErr::Negative(inp)) }])
    (NonZero:[|inp| { !inp.absolute().test_non_zero().map_err(|err| err.replace_inner(inp)) }])
    (Finite:[|inp| { inp.adaptor().test_finite().map_err(|err| err.replace_inner(inp)) } ])
);

impl<Absolute, Adaptor> FromNum<Adaptor> for Adapted<Absolute, Adaptor>
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

auto_try_from_num!(
    [
        (Adapted<Absolute, Adaptor>, f64) where
        Absolute: AbsoluteNum<Adaptor>,
        Adaptor: AdaptorNum<Absolute>,
    ]
    |inp| { f64::try_from_num(inp.adaptor()) }
);

impl<Absolute, Adaptor> AdaptedNum for Adapted<Absolute, Adaptor>
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

impl<Absolute, Adaptor> PartialEq for Adapted<Absolute, Adaptor>
where
    Absolute: AbsoluteNum<Adaptor>,
    Adaptor: AdaptorNum<Absolute>,
{
    fn eq(&self, other: &Self) -> bool {
        self.adaptor.eq(&other.adaptor)
    }
}

impl<Absolute, Adaptor> PartialOrd for Adapted<Absolute, Adaptor>
where
    Absolute: AbsoluteNum<Adaptor>,
    Adaptor: AdaptorNum<Absolute>,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.adaptor.partial_cmp(&other.adaptor)
    }
}

impl DivUsize for f64 {
    fn div_usize(&self, rhs: usize) -> NumResult<Self> {
        rhs.test_non_zero().map(|rhs| self / (rhs as f64))
    }
}

type AdaptedF64 = Adapted<AbsF64, f64>;

struct ProcessingRate {
    history: HistoryVec<AdaptedF64>,
}

impl FromNum<f64> for isize {
    fn from_num(value: f64) -> Self {
        value.test_finite().map(|f| f as isize).unwrap()
    }
}

impl FromNum<isize> for f64 {
    fn from_num(value: isize) -> Self {
        value as f64
    }
}

trait Averageable
where
    Self: HistoryNum,
{
    fn sub_then_div(&self, sub_rhs: Self, div_rhs: usize) -> Self {
        self.difference(sub_rhs)
            .div_usize(div_rhs)
            .unwrap_or(Self::default())
    }

    fn add_delta(&self, delta: Self) -> Self {
        self.increment(delta)
    }

    fn increment_existing_avg(
        self,
        existing_avg: Self,
        popped: Option<Self>,
        new_n: usize,
    ) -> Self {
        // We want to find a number delta such that delta + existing_avg is the new,
        // updated average.

        // suppose we have hit the maximum capacity in our history vector
        // therefore, we will be popping out the last element (`popped`) and then
        // adding `self` to the history vec. Let `q` be the sum of all the
        // elements in the history vector apart from the popped one.
        // So, in this case:
        //     (q + popped)/n  +  delta          = (q + self)/n
        // ==  (q + popped)/n  -  (q + self)/n   = -delta
        // ==  (q + popped)/n  -  (q + self)/n   = -delta
        // ==  (popped - self)/n                 = -delta
        // ==  (self - popped)/n                 =  delta
        // current_average + delta = (current_sum - popped + self)/len
        // which implies that
        //
        // In the case where we have not yet reached the maximum capacity of our
        // history vector, we will be appending self to the list without changing.
        // So the increment `delta` should be the solution to:
        //     q/(n - 1)  +  delta = (q + self)/n
        //
        // Instead of solving this directly, note that we have a list of (n - 1)
        // numbers, with average q/(n - 1). To this list, can we add another number
        // (the nth number), such that the new average is still q/(n - 1)?
        // Yes! We know (intuitively) that if we add a new number which is
        // exactly the existing average, then the existing average will not shift:
        //    (q + (q/n - 1))/n
        // == ((n - 1)q + q) / n(n - 1)
        // == (qn - q + q)/n(n - 1)
        // == qn / (n(n-1))
        // == q/(n - 1)
        //
        // Going back to our problem of interest, suppose that we have (n - 1)
        // numbers, and we would like to add another number to it that leaves
        // the average unchanged. We know that this number is existing_avg. Now
        // we have a list of n numbers, where the last number is existing_avg.
        // So, following what we derived in the last section: let
        // popped == existing_avg
        //
        // Then, the new average should be: (self - popped)/n
        // Now we have a list of n numbers, with the average q/(n - 1),
        // and we can use the formula derived for the preceding if statement
        // as follows to get that the increment should be (self - existing_avg)/n:
        //     (self - existing_avg)/n + existing_avg
        // ==  (self - existing_avg + n*existing_avg)/n
        // ==  (self + existing_avg * (n - 1))/n
        // ==  (self + q)/n
        // Intriguingly, this formula also works for the case (n - 1) == 0
        let popped = popped.unwrap_or(existing_avg);
        let delta = self.sub_then_div(popped, new_n);
        existing_avg.add_delta(delta)
    }
}

#[derive(Debug, Clone)]
struct HistoryVec<Data>
where
    Data: HistoryNum,
{
    capacity: usize,
    inner: Vec<Data>,
    // current_sum: T,
    average: Data,
}

impl<Data> Default for HistoryVec<Data>
where
    Data: HistoryNum,
{
    fn default() -> Self {
        HistoryVec {
            capacity: MAX_HISTORY,
            inner: Vec::with_capacity(MAX_HISTORY),
            average: Data::default(),
        }
    }
}

#[derive(Default, Clone, Copy, Debug)]
struct AvgInfo<T>
where
    T: HistoryNum,
{
    data: T,
    delta: T,
}

impl<T> Display for AvgInfo<T>
where
    T: HistoryNum,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?}({}{:?})",
            self.data,
            self.delta
                .adaptor()
                .is_positive()
                .then_some("+")
                .unwrap_or("-"),
            self.delta.absolute()
        )
    }
}

impl<T> AvgInfo<T>
where
    T: HistoryNum,
{
    fn update(&mut self, new_data: T) -> Self {
        let last = *self;
        self.data = new_data;
        self.delta = self.data.difference(last.data);
        last
    }
}

type TimeSpan = Adapted<Duration, f64>;

#[derive(Default, Clone, Debug)]
struct AvgInfoBundle {
    processing_rate: AvgInfo<AdaptedF64>,
    task_time: AvgInfo<TimeSpan>,
    idle_time: AvgInfo<TimeSpan>,
}

impl AvgInfoBundle {
    fn update(
        &mut self,
        processing_rate: AdaptedF64,
        task_time: TimeSpan,
        idle_time: TimeSpan,
    ) -> Self {
        Self {
            processing_rate: self.processing_rate.update(processing_rate),
            task_time: self.task_time.update(task_time),
            idle_time: self.idle_time.update(idle_time),
        }
    }
}

impl Display for AvgInfoBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "pr: {} | tt/it: {} | tt: {} | it : {}",
            self.processing_rate.data.custom_display(),
            self.task_time
                .data
                .ratio(self.idle_time.data)
                .map(|f| f.custom_display())
                .unwrap_or("None".into()),
            self.task_time.data.custom_display(),
            self.idle_time.data.custom_display(),
        )
    }
}

impl<T> Averageable for T where T: HistoryNum {}

impl<Data> HistoryVec<Data>
where
    Data: HistoryNum,
{
    fn push(&mut self, k: InnerAbsolute<Data>) {
        if self.inner.len() == self.capacity {
            self.inner.pop();
        }
        self.inner.push(Data::from_absolute(k));
        self.average = (Data::iter_sum(self.inner.iter().copied()))
            .div_usize(self.inner.len())
            .unwrap_or(Data::default());
    }

    // fn last(&self) -> Option<Data> {
    //     self.inner.last().copied()
    // }

    fn iter(&self) -> std::slice::Iter<Data> {
        self.inner.iter()
    }
}

macro_rules! create_paired_comm {
    (
        $name:ident$(($($params:tt)+))? ;
        LHS: $(($fid:ident, $fty:ty)),+ ;
        RHS: $(($gid:ident, $gty:ty)),+
    ) => {
        paste! {
            struct $name$(<$($params:ident),+>)? {
                $([< $fid _sender >]: Sender<$fty>,)+
                $([< $gid _receiver >]: Receiver<$gty>,)+
            }
        }
    }
}

macro_rules! create_paired_comms {
    (
        [ $lhs_snake_name:ident ; $lhs_struct_id:ident$(($($lhs_params:tt)+))? ; $(($fid:ident, $fty:ty)),+ ] <->
        [ $rhs_snake_name:ident ; $rhs_struct_id:ident$(($($rhs_params:tt)+))? ; $(($gid:ident, $gty:ty)),+ ]
    ) => {
        create_paired_comm!(
            $lhs_struct_id$(($($lhs_params)+))? ;
            LHS: $(($fid, $fty)),+ ;
            RHS: $(($gid, $gty)),+
        );
        create_paired_comm!(
            $rhs_struct_id$(($($rhs_params)+))? ;
            LHS: $(($gid, $gty)),+ ;
            RHS: $(($fid, $fty)),+
        );

        paste! {
            fn [< new_ $lhs_snake_name _to_ $rhs_snake_name _comms>]() ->
                ($lhs_struct_id, $rhs_struct_id)
            {
                $(let ([< $fid _sender>], [< $fid _receiver>]) = mpsc::channel();)+
                $(let ([< $gid _sender>], [< $gid _receiver>]) = mpsc::channel();)+

                (
                    $lhs_struct_id {
                        $([< $fid _sender >],)+
                        $([< $gid _receiver >],)+
                    },
                    $rhs_struct_id {
                        $([< $gid _sender >],)+
                        $([< $fid _receiver >],)+
                    }
                )
            }
        }
    }
}

#[derive(Debug)]
enum WorkError {
    StatusRequestSendError,
    PrintSenderDisconnected,
}

impl<T> From<SendError<T>> for WorkError {
    fn from(_: SendError<T>) -> Self {
        Self::StatusRequestSendError
    }
}

#[derive(Clone, Debug, Default)]
struct Timer {
    start: Option<Instant>,
    history: HistoryVec<TimeSpan>,
    total: Duration,
}

impl Timer {
    #[inline]
    fn begin(&mut self) {
        self.start.replace(Instant::now());
    }

    fn end(&mut self) -> Option<Duration> {
        self.start.take().map(|instant| {
            let elapsed = instant.elapsed();
            self.total += elapsed;
            self.history.push(elapsed);
            elapsed
        })
    }
}

#[derive(Clone, Debug)]
struct WorkResults {
    avg_task_time: TimeSpan,
    avg_idle_time: TimeSpan,
    avg_processing_rate: AdaptedF64,
    newly_processed: usize,
    max_dir_size: usize,
    in_flight: usize,
}

impl Default for WorkResults {
    fn default() -> Self {
        WorkResults {
            avg_task_time: TimeSpan::default(),
            avg_idle_time: TimeSpan::default(),
            avg_processing_rate: 0.0.into_num(),
            newly_processed: 0,
            in_flight: 0,
            max_dir_size: 0,
        }
    }
}

impl WorkResults {
    fn merge(mut self, next: WorkResults) -> Self {
        let WorkResults {
            avg_task_time,
            avg_idle_time,
            avg_processing_rate,
            newly_processed: dirs_processed,
            max_dir_size,
            in_flight,
        } = next;
        self.avg_task_time = avg_task_time;
        self.avg_idle_time = avg_idle_time;
        self.avg_processing_rate = avg_processing_rate;
        self.newly_processed = self.newly_processed + dirs_processed;
        self.max_dir_size = self.max_dir_size.max(max_dir_size);
        self.in_flight += in_flight;
        self
    }
}

impl std::iter::Sum for WorkResults {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.reduce(|a, b| a.merge(b)).unwrap_or_default()
    }
}

#[derive(Default)]
struct ThreadHistory {
    dirs_processed: HistoryVec<Adapted<usize, isize>>,
    processing_rates: HistoryVec<AdaptedF64>,
    work_request_sizes: HistoryVec<Adapted<usize, isize>>,
    t_order: Timer,
    t_idle: Timer,
    t_post_order: Timer,
    start_t: Timer,
    total_processed: usize,
}

macro_rules! end_timer {
    ($self:ident, $name:ident) => {
        paste! {
            $self.[< t_began_ post_order >]
            .end(&mut $self.[< total_t_ $name >], &mut $self.[<post_order_ times>])
        }
    };
}

impl ThreadHistory {
    fn began_process(&mut self) {
        self.start_t.begin();
    }

    fn began_idling(&mut self) {
        end_timer!(self, post_order);
        self.t_idle.begin();
    }

    fn began_order(&mut self) {
        end_timer!(self, idle);
        end_timer!(self, post_order);
        self.t_order.begin();
    }

    fn began_post_order(&mut self, newly_processed_count: usize) {
        if let Some(elapsed) = end_timer!(self, order) {
            self.processing_rates
                .push((newly_processed_count as f64 / elapsed.as_secs_f64()).into());
            self.dirs_processed.push(newly_processed_count);
            self.total_processed += newly_processed_count;
        }
        self.t_post_order.begin();
    }
}

create_paired_comms!(
    [handle ;  ThreadHandleComms ; (new_work, Vec<WorkSlice>), (surplus_request, usize)] <->
    [thread ; ThreadComms ; (status, Status), (result, WorkResults), (out_of_work, usize), (surplus_fulfill, WorkSlice) ]
);

type LockedPathBuf = Arc<RwLock<Vec<PathBuf>>>;

#[derive(Default, Debug, Clone)]
struct WorkBuf {
    buf: LockedPathBuf,
    pending_from: usize,
    shared_with_others: Vec<(usize, usize)>,
}

impl From<Vec<PathBuf>> for WorkBuf {
    fn from(seed_work: Vec<PathBuf>) -> Self {
        WorkBuf {
            buf: Arc::new(RwLock::new(seed_work)),
            pending_from: 0,
            shared_with_others: vec![],
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum WorkSource {
    Local,
    Shared,
}

#[derive(Clone, Debug)]
struct WorkSlice {
    source: WorkSource,
    buf: LockedPathBuf,
    start: usize,
    end: usize,
    cursor: usize,
}

impl WorkSlice {
    fn new(source: WorkSource, buf: LockedPathBuf, start: usize, end: usize) -> WorkSlice {
        WorkSlice {
            source,
            buf,
            start,
            end,
            cursor: start,
        }
    }

    fn buf(&self) -> RwLockReadGuard<'_, Vec<PathBuf>> {
        self.buf.read().unwrap()
    }

    fn len(&self) -> usize {
        self.end - self.start
    }

    fn split(self, split_size: usize) -> (WorkSlice, Option<WorkSlice>) {
        if split_size > self.len() {
            (self, None)
        } else {
            (
                WorkSlice::new(
                    self.source,
                    self.buf.clone(),
                    self.start,
                    self.start + split_size,
                ),
                Some(WorkSlice::new(
                    self.source,
                    self.buf.clone(),
                    self.start + split_size,
                    self.end,
                )),
            )
        }
    }
}

#[allow(unused)]
// TODO: handle error dirs
struct ErrorDir {
    err: IoErr,
    path: PathBuf,
}

impl ErrorDir {
    fn new(path: PathBuf, err: IoErr) -> Self {
        Self { err, path }
    }
}

type DirEntryResult = Result<DirEntry, ErrorDir>;
type WorkSliceIterItem = Result<Vec<DirEntryResult>, ErrorDir>;

impl Iterator for WorkSlice {
    type Item = WorkSliceIterItem;

    fn next(&mut self) -> Option<WorkSliceIterItem> {
        if self.start <= self.cursor && self.cursor < self.end {
            let result = {
                let path = &self.buf()[self.cursor];
                read_dir(path)
                    .map(|read_iter| {
                        read_iter
                            .map(|dir_entry_result| {
                                dir_entry_result.map_err(|err| ErrorDir::new(path.clone(), err))
                            })
                            .collect::<Vec<DirEntryResult>>()
                    })
                    .map_err(|err| ErrorDir::new(path.clone(), err))
            };
            self.cursor += 1;
            Some(result)
        } else {
            None
        }
    }
}

impl WorkBuf {
    #[inline]
    fn len(&self) -> usize {
        self.buf.read().unwrap().len()
    }

    #[inline]
    fn empty_pending(&self) -> bool {
        self.pending_from == self.len()
    }

    #[inline]
    fn next_pending_from(&self, size: usize) -> usize {
        (self.pending_from + size).min(self.len())
    }

    #[inline]
    fn get_work_slice(&mut self, source: WorkSource, size: usize) -> WorkSlice {
        let start = self.pending_from;
        self.pending_from = self.next_pending_from(size);
        WorkSlice::new(source, self.buf.clone(), start, self.pending_from)
    }

    #[inline]
    fn get_work_for_local(&mut self, size: usize) -> WorkSlice {
        self.get_work_slice(WorkSource::Local, size)
    }

    #[inline]
    fn get_work_for_sharing(&mut self, size: usize) -> WorkSlice {
        let slice = self.get_work_slice(WorkSource::Shared, size);
        self.shared_with_others.push((slice.start, slice.end));
        slice
    }

    fn total_pending(&self) -> usize {
        self.len() - self.pending_from
    }

    fn extend(&self, paths: impl Iterator<Item = PathBuf>) {
        // println!("attempting to extend workbuf!");
        self.buf.write().unwrap().extend(paths);
        // println!("finished extending workbuf!");
    }
}

struct Thread {
    id: usize,
    comms: ThreadComms,
    printer: Printer,
    status: Status,
    history: ThreadHistory,
    max_dir_size: usize,
    shared_from_others: Vec<usize>,
    work_buf: WorkBuf,
    errored: Vec<ErrorDir>,
}

struct ThreadHandle {
    comms: ThreadHandleComms,
    status: Status,
    worker_thread: JoinHandle<()>,
    in_flight: usize,
    new_dirs_processed: usize,
    avg_info_bundle: AvgInfoBundle,
    // work_request_size: usize,
    // shared_work: Vec<PathBuf>,
}

struct Executor {
    print_receiver: Option<Receiver<Vec<String>>>,
    handles: Vec<ThreadHandle>,
    max_dir_size: usize,
    last_status_print: Option<Instant>,
    start_time: Instant,
    processed: usize,
    orders_submitted: usize,
    loop_sleep_time: Duration,
    loop_sleep_time_history: HistoryVec<TimeSpan>,
    is_finished: bool,
    unfulfilled_requests: [usize; NUM_THREADS],
    available_surplus: Vec<Vec<WorkSlice>>,
}

macro_rules! thread_print {
    ($self:ident, $str_lit:literal$(, $($args:tt)+)?) => {
        $self.printer.push(|| format!("{}: {}", $self.id, format!($str_lit$(, $($args)+)?)));
    };
}

macro_rules! printer_print {
    ($printer:ident, $str_lit:literal$(, $($args:tt)+)?) => {
        $printer.push(|| format!("{}", format!($str_lit$(, $($args)+)?)));
    };
}

impl Thread {
    fn new(
        id: usize,
        seed_work: Vec<PathBuf>,
        comms: ThreadComms,
        print_sender: Option<Sender<Vec<String>>>,
    ) -> Self {
        comms.status_sender.send(Status::Idle).unwrap();
        let mut printer = Printer::new(print_sender);
        printer_print!(printer, "{}: Beginning with seed work: {:?}", id, seed_work);
        Self {
            id,
            comms,
            printer,
            status: Status::Idle,
            history: ThreadHistory::default(),
            max_dir_size: 0,
            work_buf: WorkBuf::from(seed_work),
            shared_from_others: vec![],
            errored: vec![],
        }
    }

    fn send_status(&self) -> Result<(), WorkError> {
        self.comms.status_sender.send(self.status)?;
        Ok(())
    }

    fn change_status(&mut self, new_status: Status) -> Result<(), WorkError> {
        self.status = new_status;
        self.send_status()
    }

    fn get_local_work(&mut self) -> WorkSlice {
        let work = self.work_buf.get_work_for_local(DEFAULT_WORK_CHUNK_SIZE);
        self.history.began_order();
        work
    }

    fn get_work(&mut self) -> Vec<WorkSlice> {
        if self.work_buf.empty_pending() {
            self.get_shared_work()
        } else {
            vec![self.get_local_work()]
        }
    }

    fn finished_work(&mut self, work_slice: WorkSlice) {
        self.history.began_post_order(work_slice.len());
    }

    fn get_share_request_size(&self) -> usize {
        let work_request_size = ((self.history.processing_rates.average.adaptor()
            * self.history.order_times.average.adaptor())
        .round() as usize)
            .max(1);
        work_request_size
    }

    fn get_shared_work(&mut self) -> Vec<WorkSlice> {
        self.history.began_idling();
        thread_print!(self, "Began idling!");
        self.comms.status_sender.send(Status::Idle).unwrap();
        let request_size = self.get_share_request_size();
        thread_print!(self, "share request size: {}", request_size);
        self.history.work_request_sizes.push(request_size);
        self.comms.out_of_work_sender.send(request_size).unwrap();
        let shared_work = self.comms.new_work_receiver.recv().unwrap();
        self.shared_from_others.push(shared_work.len());
        thread_print!(
            self,
            "Done idling, took: {:?}",
            self.history.idle_times.inner.last().map(|t| t.absolute())
        );
        shared_work
    }

    fn share_work(&mut self, surplus_request: usize) -> Result<(), WorkError> {
        let shared_work_slice = self.work_buf.get_work_for_sharing(
            surplus_request.min(self.work_buf.total_pending()), // if surplus_request > 0 { 1 } else { 0 },
        );
        thread_print!(
            self,
            "sending work for sharing: {:?}",
            shared_work_slice.len()
        );
        if shared_work_slice.len() > 0 {
            self.comms.surplus_fulfill_sender.send(shared_work_slice)?;
        }
        Ok(())
    }

    fn in_flight(&self) -> usize {
        self.work_buf.total_pending()
    }

    fn start(mut self) {
        self.history.began_process();
        loop {
            self.printer.flush_send().unwrap();

            let work_slices = self.get_work();
            self.history.began_order();
            self.change_status(Status::Busy).unwrap();

            let mut newly_processed = 0;
            // println!("Non printer: beginning task...");
            for mut work_slice in work_slices {
                // println!("Non printer: beginning inner loop");
                'inner: loop {
                    if let Some(dir_entries) = work_slice.next() {
                        if let Err(err_dir) = dir_entries.map(|entry_vec| {
                            if entry_vec.len() > 0 {
                                let mut this_dir_size = 0;
                                self.work_buf.extend(entry_vec.into_iter().filter_map(
                                    |dir_entry_result| {
                                        dir_entry_result
                                            .map(|entry| {
                                                // dir_entry.metadata().ok().and_then(|meta| meta.is_dir().then_some(dir_entry.path()))
                                                match entry.metadata() {
                                                    Ok(meta) => meta.is_dir().then_some({
                                                        this_dir_size += 1;
                                                        entry.path()
                                                    }),
                                                    Err(err) => {
                                                        self.errored.push(ErrorDir {
                                                            err,
                                                            path: entry.path(),
                                                        });
                                                        None
                                                    }
                                                }
                                            })
                                            .map_err(|err_dir| self.errored.push(err_dir))
                                            .ok()
                                            .flatten()
                                    },
                                ));
                                self.max_dir_size = self.max_dir_size.max(this_dir_size);
                            }
                        }) {
                            self.errored.push(err_dir)
                        }
                    } else {
                        break 'inner;
                    }
                }
                // println!("Non printer: finished inner loop");
                newly_processed += work_slice.len();
                self.finished_work(work_slice);
            }
            // println!("Non printer: finished task...");
            self.history.began_post_order(newly_processed);
            if let Some(surplus_request) = self.comms.surplus_request_receiver.try_iter().last() {
                thread_print!(
                    self,
                    "found something in the surplus requests: {surplus_request}"
                );
                self.share_work(surplus_request).unwrap();
            }
            self.comms
                .result_sender
                .send(WorkResults {
                    avg_task_time: self.history.order_times.average,
                    avg_idle_time: self.history.idle_times.average,
                    max_dir_size: self.max_dir_size,
                    avg_processing_rate: self.history.processing_rates.average,
                    newly_processed,
                    in_flight: self.in_flight(),
                })
                .unwrap();
        }
    }
}

impl ThreadHandle {
    fn new(id: usize, seed_work: Vec<PathBuf>, print_sender: Option<Sender<Vec<String>>>) -> Self {
        let (handle_comms, process_thread_comms) = new_handle_to_thread_comms();
        let worker = Thread::new(id, seed_work, process_thread_comms, print_sender);

        ThreadHandle {
            comms: handle_comms,
            status: Status::Idle,
            worker_thread: thread::spawn(move || worker.start()),
            in_flight: 0,
            new_dirs_processed: 0,
            avg_info_bundle: AvgInfoBundle::default(),
        }
    }

    // fn get_avg_task_time(&self) -> Duration {
    //     self.avg_info_bundle.task_time.data
    // }

    // fn get_avg_idle_time(&self) -> Duration {
    //     self.avg_info_bundle
    //         .idle_time
    //         .data
    //         .try_adaptor_as_absolute()
    //         .unwrap()
    // }

    // fn get_avg_processing_rate(&self) -> f64 {
    //     self.avg_info_bundle.processing_rate.data
    // }

    fn get_avg_info(&self) -> AvgInfoBundle {
        self.avg_info_bundle
    }

    // fn drain_orders(&mut self) -> Vec<PathBuf> {
    //     let order: Vec<PathBuf> = self.orders.drain(0..).collect();
    //     self.in_flight += order.len();
    //     order
    // }

    // fn queue_orders(&mut self, orders: Vec<PathBuf>) {
    //     self.orders.extend(orders.into_iter());
    // }

    fn dispatch_surplus(&self, surplus: Vec<WorkSlice>) -> Result<(), WorkError> {
        // if self.is_idle() {
        //     self.in_flight += orders.len();
        //     self.comms.order_sender.send(orders)?;
        //     let drained_orders = self.drain_orders();
        //     self.comms.order_sender.send(drained_orders)?;
        // } else {
        //     self.queue_orders(orders);
        // }
        self.comms.new_work_sender.send(surplus)?;
        // let drained_orders = self.drain_orders();
        // self.comms.order_sender.send(drained_orders)?;
        Ok(())
    }

    fn dispatch_surplus_request(&self, request_size: usize) -> Result<(), WorkError> {
        // println!("sending surplus request of size: {request_size}");
        self.comms.surplus_request_sender.send(request_size)?;
        Ok(())
    }

    // fn push_orders(&mut self, orders: Vec<PathBuf>) -> Result<(), WorkError> {
    //     self.dispatch_orders(orders)?;
    //     // if self.is_idle() {
    //     //     self.dispatch_orders(orders)?;
    //     // } else {
    //     //     self.queue_orders(orders);
    //     // }
    //     Ok(())
    // }

    fn update_dirs_processed(&mut self, newly_processed: usize) {
        self.new_dirs_processed += newly_processed;
        if self.in_flight < newly_processed {
            self.in_flight = 0;
        } else {
            self.in_flight -= newly_processed;
        }
    }

    fn drain_results(&mut self) -> Option<usize> {
        let results: Vec<WorkResults> = self.comms.result_receiver.try_iter().collect();
        if results.len() > 0 {
            let WorkResults {
                avg_task_time,
                avg_idle_time,
                avg_processing_rate,
                newly_processed,
                max_dir_size,
                in_flight,
            } = results.into_iter().sum();
            self.avg_info_bundle
                .update(avg_processing_rate, avg_task_time, avg_idle_time);
            self.in_flight = in_flight;
            self.update_dirs_processed(newly_processed);
            Some(max_dir_size)
        } else {
            None
        }
    }

    fn in_flight(&self) -> usize {
        self.in_flight
    }

    fn update_status(&mut self) {
        self.status = self
            .comms
            .status_receiver
            .try_iter()
            .last()
            .unwrap_or(self.status);
    }

    fn is_idle(&mut self) -> bool {
        self.update_status();
        self.status == Status::Idle
    }

    fn finish(self) {
        self.worker_thread.join().unwrap();
    }
}

// fn task_time_score(avg_task: f64, max_avg_task_time: f64) -> f64 {
//     1.0 - (avg_task / max_avg_task_time)
// }

// fn idle_time_score(avg_idle: f64, max_avg_idle_time: f64) -> f64 {
//     avg_idle / max_avg_idle_time
// }

// // fn in_flight_penalty(in_flight: usize) -> f64 {
// //     (1.0 - (in_flight as f64 / MAX_IN_FLIGHT as f64))
// //         .min(0.0)
// //         .max(1.0)
// // }

// fn processing_rate_score(processing_rate: f64, max_processing_rate: f64) -> f64 {
//     processing_rate / max_processing_rate
// }

trait FindMaxMin
where
    Self: IntoIterator + Sized,
    <Self as IntoIterator>::Item: PartialOrd + Copy,
{
    fn find_max(
        self,
        default_for_max: <Self as IntoIterator>::Item,
    ) -> <Self as IntoIterator>::Item {
        self.into_iter()
            .max_by(|p, q| p.partial_cmp(q).unwrap_or(Ordering::Less))
            .unwrap_or(default_for_max)
    }
    fn find_min(
        self,
        default_for_min: <Self as IntoIterator>::Item,
    ) -> <Self as IntoIterator>::Item {
        self.into_iter()
            .min_by(|p, q| p.partial_cmp(q).unwrap_or(Ordering::Less))
            .unwrap_or(default_for_min)
    }
}

impl<'a, T> FindMaxMin for &'a Vec<T> where T: PartialOrd + Copy {}

struct AvgInfoSummary<T>
where
    T: HistoryNum,
{
    max: Option<T>,
    min: Option<T>,
    total: T,
}

impl<T, I> From<I> for AvgInfoSummary<T>
where
    T: Averageable + Debug + Copy + Clone,
    I: Iterator<Item = AvgInfo<T>>,
{
    fn from(info_vec: I) -> Self {
        let data: Vec<T> = info_vec.map(|x| x.data).collect();
        Self {
            max: data
                .iter()
                .copied()
                .max_by(|p, q| p.partial_cmp(&q).unwrap_or(Ordering::Less)),
            min: data
                .iter()
                .copied()
                .min_by(|p, q| p.partial_cmp(&q).unwrap_or(Ordering::Greater)),
            total: data
                .iter()
                .copied()
                .fold(T::default(), |p, q| p.increment(q)),
        }
    }
}

struct AvgInfoWithSummaries {
    // processing_rates: Vec<AvgInfo<AdaptedProcessingRate>>,
    // task_times: Vec<AvgInfo<AdaptedDuration>>,
    // idle_times: Vec<AvgInfo<AdaptedDuration>>,
    summary_processing_rates: AvgInfoSummary<AdaptedF64>,
    summary_task_times: AvgInfoSummary<TimeSpan>,
    summary_idle_times: AvgInfoSummary<TimeSpan>,
}

impl From<Vec<AvgInfoBundle>> for AvgInfoWithSummaries {
    fn from(avg_info_bundle: Vec<AvgInfoBundle>) -> Self {
        let processing_rates: Vec<AvgInfo<AdaptedF64>> =
            avg_info_bundle.iter().map(|x| x.processing_rate).collect();
        let task_times: Vec<AvgInfo<TimeSpan>> =
            avg_info_bundle.iter().map(|x| x.task_time).collect();
        let idle_times: Vec<AvgInfo<TimeSpan>> =
            avg_info_bundle.iter().map(|x| x.idle_time).collect();
        let summary_processing_rates = processing_rates.iter().copied().into();
        let summary_task_times = task_times.iter().copied().into();
        let summary_idle_times = idle_times.iter().copied().into();
        Self {
            // processing_rates,
            // task_times,
            // idle_times,
            summary_processing_rates,
            summary_task_times,
            summary_idle_times,
        }
    }
}

enum RedistributeResult {
    SurplusRequestsSent,
    SurplusesDistributed,
    NoDistributionRequired,
    NoPathsInFlight,
}

impl Executor {
    fn new(mut seed: Vec<PathBuf>, verbose: bool) -> Self {
        let (print_sender, print_receiver) = if verbose {
            let (print_sender, print_receiver) = mpsc::channel();
            (Some(print_sender), Some(print_receiver))
        } else {
            (None, None)
        };

        let seed_work_size = ((seed.len() as f64 / NUM_THREADS as f64).round() as usize).max(1);
        let handles = (0..NUM_THREADS)
            .map(|id| {
                let seed_work = seed
                    .drain(0..seed_work_size.min(seed.len()))
                    .collect::<Vec<PathBuf>>();
                ThreadHandle::new(id, seed_work, print_sender.clone())
            })
            .collect();

        Self {
            print_receiver,
            handles,
            max_dir_size: 0,
            last_status_print: None,
            start_time: Instant::now(),
            processed: 0,
            orders_submitted: 0,
            loop_sleep_time: DEFAULT_EXECUTE_LOOP_SLEEP,
            loop_sleep_time_history: HistoryVec::default(),
            is_finished: false,
            unfulfilled_requests: [0; NUM_THREADS],
            available_surplus: vec![Vec::default(); NUM_THREADS],
        }
    }

    // fn fetch_stats<T>(&self, f: fn(&ThreadHandle) -> T) -> Vec<T>
    // where
    //     T: PartialOrd + Copy + Default,
    // {
    //     self.handles.iter().map(f).collect()
    // }

    fn update_unfulfilled_requests(&mut self) {
        self.handles
            .iter()
            .zip(self.unfulfilled_requests.iter_mut())
            .for_each(|(h, slot)| {
                *slot = *slot + h.comms.out_of_work_receiver.try_iter().last().unwrap_or(0);
            });
    }

    fn get_total_surplus(&self) -> usize {
        self.available_surplus
            .iter()
            .map(|surplus_vec| {
                surplus_vec
                    .iter()
                    .map(|slice: &WorkSlice| slice.len())
                    .sum::<usize>()
            })
            .sum()
    }

    fn update_available_surplus(&mut self) {
        self.handles
            .iter()
            .zip(self.available_surplus.iter_mut())
            .for_each(|(h, surplus_slot)| {
                // let initial_slot_len = surplus_slot.len();
                surplus_slot.extend(h.comms.surplus_fulfill_receiver.try_iter());
                // let final_slot_len = surplus_slot.len();
                // if unfulfilled > 0 && surplus_slot.len() > 0 {
                //     panic!("surplus slot is non-empty, but unfulfilled request also exists!")
                // }
            });
    }

    fn get_surplus_of_size(&mut self, size: usize) -> Vec<WorkSlice> {
        let mut unfulfilled = size;
        let mut result = vec![];
        self.available_surplus = self
            .available_surplus
            .drain(0..)
            .map(|mut surplus_from_thread| {
                surplus_from_thread = surplus_from_thread
                    .into_iter()
                    .filter_map(|surplus| {
                        let (will_give, remaining) = if unfulfilled < surplus.len() {
                            surplus.split(unfulfilled)
                        } else {
                            (surplus, None)
                        };
                        unfulfilled -= will_give.len();
                        result.push(will_give);
                        remaining
                    })
                    .collect::<Vec<WorkSlice>>();
                surplus_from_thread
            })
            .collect();
        result
    }

    fn send_surplus_requests(
        &self,
        total_required: usize,
    ) -> Result<RedistributeResult, WorkError> {
        let in_flights: Vec<usize> = self.handles.iter().map(|h| h.in_flight()).collect();
        // println!("in flights: {in_flights:?}");
        let max_in_flight: usize = in_flights.iter().max().copied().unwrap_or(0);
        // println!("max in flight: {max_in_flight:?}");
        if max_in_flight > 0 && total_required > 0 {
            let mut remaining = total_required;
            let mut handle_info: Vec<(usize, usize, f64)> = self
                .handles
                .iter()
                .enumerate()
                .zip(in_flights.into_iter())
                .map(|((ix, h), in_flight)| {
                    (
                        ix,
                        in_flight,
                        h.get_avg_info().processing_rate.data.adaptor(),
                    )
                })
                .collect();
            handle_info.sort_unstable_by(|p, q| {
                (p.1 as f64 / p.2)
                    .partial_cmp(&(q.1 as f64 / q.2))
                    .unwrap_or(Ordering::Less)
            });
            // println!("sorted handles with pr: {handle_info:?}");
            for (ix, in_flight, _) in handle_info.into_iter() {
                if remaining == 0 {
                    break;
                } else {
                    if in_flight > 0 {
                        let request_size = (((in_flight as f64 / max_in_flight as f64)
                            * total_required as f64)
                            .round() as usize)
                            .min(remaining);
                        self.handles[ix].dispatch_surplus_request(request_size)?;
                        remaining -= request_size;
                    }
                }
            }
            Ok(RedistributeResult::SurplusRequestsSent)
        } else {
            Ok(RedistributeResult::NoPathsInFlight)
        }
    }

    fn update_loop_sleep_time(&mut self) {
        // let idle_times = self.fetch_stats(|h| h.get_avg_idle_time());
        // let min_idle_time = idle_times
        //     .iter()
        //     .copied()
        //     .min()
        //     .unwrap_or(DEFAULT_EXECUTE_LOOP_SLEEP);
        // self.loop_sleep_time = (min_idle_time / NUM_THREADS as u32).min(DEFAULT_EXECUTE_LOOP_SLEEP);
        // self.loop_sleep_time_history.push(self.loop_sleep_time);
        self.loop_sleep_time = DEFAULT_EXECUTE_LOOP_SLEEP;
    }

    // fn redistribute_work(&mut self) -> Result<RedistributeResult, WorkError> {
    //     // println!("initial unfulfilled: {:?}", self.unfulfilled_requests);
    //     self.update_unfulfilled_requests();
    //     // println!("updated unfulfilled: {:?}", self.unfulfilled_requests);
    //     let unfulfilled_total = self.unfulfilled_requests.iter().sum();
    //     if unfulfilled_total > 0 {
    //         self.update_available_surplus();
    //         let initial_surplus_total = self.surplus_total();
    //         // println!("available surplus: {:?}", initial_surplus_total);
    //         if initial_surplus_total == 0 {
    //             // println!("no surplus found, sending another request");
    //             return self.send_surplus_requests(unfulfilled_total);
    //         } else {
    //             // println!("dispatching surplus");
    //             let mut current_surplus_total = initial_surplus_total;
    //             let will_takes = self
    //                 .unfulfilled_requests
    //                 .iter_mut()
    //                 .enumerate()
    //                 .filter_map(|(ix, unfulfilled)| {
    //                     (*unfulfilled > 0).then_some({
    //                         let will_take = (((*unfulfilled as f64 / unfulfilled_total as f64)
    //                             * (initial_surplus_total as f64))
    //                             .round() as usize)
    //                             .min(current_surplus_total)
    //                             .min(*unfulfilled)
    //                             .max(1);
    //                         current_surplus_total -= will_take;
    //                         (ix, will_take)
    //                     })
    //                 })
    //                 .collect::<Vec<(usize, usize)>>();
    //             // println!("{will_takes:?}");
    //             for (ix, will_take) in will_takes {
    //                 let surplus = self.get_surplus_of_size(will_take);
    //                 self.unfulfilled_requests[ix] -= surplus.len();
    //                 // println!(
    //                 //     "dispatching surplus of size: {} to thread {}",
    //                 //     surplus.len(),
    //                 //     ix
    //                 // );
    //                 self.handles[ix].dispatch_surplus(surplus)?;
    //             }
    //             return Ok(RedistributeResult::SurplusesDistributed);
    //         }
    //     } else {
    //         // println!("no re-distribution was required!");
    //         Ok(RedistributeResult::NoDistributionRequired)
    //     }
    // }

    fn redistribute_work(&mut self) -> Result<RedistributeResult, WorkError> {
        let in_flights = self
            .handles
            .iter()
            .map(|h| h.in_flight())
            .collect::<Vec<usize>>();
        let max_in_flights = in_flights.iter().copied().max().unwrap_or(0);
        if max_in_flights > 0 {
            Ok(RedistributeResult::NoDistributionRequired)
        } else {
            self.update_available_surplus();
            let each_thread_should_have = in_flights.iter().sum::<usize>() / NUM_THREADS;
            let mut total_surplus = self.get_total_surplus();
            if total_surplus > 0 {
                for (ix, in_flight) in in_flights.into_iter().enumerate() {
                    if in_flight > each_thread_should_have {
                        // self.handles[ix]
                        //     .dispatch_surplus_request(in_flight - each_thread_should_have)?;
                    } else {
                        let surplus = self.get_surplus_of_size(each_thread_should_have);
                        total_surplus -= surplus.len().min(total_surplus);
                        self.handles[ix].dispatch_surplus(surplus)?;
                    }
                    if total_surplus == 0 {
                        break;
                    }
                }
                Ok(RedistributeResult::SurplusesDistributed)
            } else {
                for (ix, in_flight) in in_flights.into_iter().enumerate() {
                    if in_flight > each_thread_should_have {
                        self.handles[ix]
                            .dispatch_surplus_request(in_flight - each_thread_should_have)?;
                    }
                }
                Ok(RedistributeResult::SurplusRequestsSent)
            }
        }
    }

    // fn distribute_work(&mut self) -> Result<(), WorkError> {
    //     let (_, _) = self.fetch_stats_and_max(|h| h.avg_task_time.as_secs_f64(), 0.0);
    //     let (idle_times, max_idle_time) =
    //         self.fetch_stats_and_max(|h| h.avg_idle_time.as_secs_f64(), 0.0);
    //     let (processing_rates, max_processing_rate) =
    //         self.fetch_stats_and_max(|h| h.avg_processing_rate, 0.0);
    //     let currently_submitted = self.fetch_stats(|h| h.in_flight());
    //     let max_per_thread =
    //         (self.work_q.len() as f64 / self.handles.len() as f64).floor() as usize;
    //     let dispatch_sizes: Vec<usize> = if max_idle_time > 0.0 && max_processing_rate > 0.0 {
    //         // bias distribution
    //         // let ratings = task_times
    //         //     .iter()
    //         //     .zip(idle_times.iter())
    //         //     .zip(processing_rates.iter())
    //         //     .map(|((&task_time, &idle_time), _)| {
    //         //         (max_task_time / max_idle_time).min(1.0)
    //         //             * task_time_score(task_time, max_task_time)
    //         //             + idle_time_score(idle_time, max_idle_time)
    //         //     })
    //         //     .collect::<Vec<f64>>();
    //         // let normalizer: f64 = *ratings.find_max(&1.0);
    //         // self.ratings = ratings.iter().map(|r| r / normalizer).collect();
    //         // ratings
    //         //     .into_iter()
    //         //     .map(|r| (r / normalizer))
    //         //     .zip(currently_submitted.iter())
    //         //     .map(|(r, in_flight)| {
    //         //         ((r * max_per_thread as f64).round() as usize)
    //         //             .min(MAX_IN_FLIGHT - in_flight)
    //         //     })
    //         //     .collect()
    //         let requested_dispatches: Vec<usize> = processing_rates
    //             .iter()
    //             .zip(idle_times.iter())
    //             .zip(currently_submitted.iter())
    //             .map(|((&processing_rate, &idle_time), &in_flight)| {
    //                 let filled_idle_time = in_flight as f64 / processing_rate;
    //                 let unfilled_idle_time = (idle_time > filled_idle_time)
    //                     .then(|| idle_time - filled_idle_time)
    //                     .unwrap_or(0.0);
    //                 let would_like = ((processing_rate * unfilled_idle_time).round() as usize)
    //                     .max(if in_flight == 0 { 1 } else { 0 });
    //                 // let would_like = if would_like + in_flight > MAX_IN_FLIGHT {
    //                 //     (would_like + in_flight) - MAX_IN_FLIGHT
    //                 // } else {
    //                 //     would_like
    //                 // };
    //                 would_like
    //             })
    //             .collect();
    //         // println!(
    //         //     "({}, {}) requested_dispatches: {requested_dispatches:?}",
    //         //     processing_rates.len(),
    //         //     idle_times.len()
    //         // );
    //         let max_dispatch_total = self.work_q.len().min(MAX_TOTAL_DISPATCH_SIZE);
    //         if requested_dispatches.iter().sum::<usize>() > max_dispatch_total {
    //             let max_dispatch_request = requested_dispatches
    //                 .iter()
    //                 .max()
    //                 .copied()
    //                 .unwrap_or(1_usize) as f64;
    //             requested_dispatches
    //                 .into_iter()
    //                 .map(|x| {
    //                     ((x as f64 / max_dispatch_request) * max_dispatch_total as f64).round()
    //                         as usize
    //                 })
    //                 .collect()
    //         } else {
    //             requested_dispatches
    //         }
    //     } else {
    //         // distribute equally
    //         let max_per_thread = max_per_thread.max(1);
    //         vec![max_per_thread; self.handles.len()]
    //     };
    //     // println!("dispatches: {dispatch_sizes:?}");
    //     dispatch_sizes
    //         .into_iter()
    //         .zip(self.handles.iter_mut())
    //         .zip(currently_submitted.iter())
    //         .map(|((dispatch_size, handle), &_)| {
    //             let final_dispatch_size = dispatch_size.min(self.work_q.len());
    //             let orders: Vec<PathBuf> = self.work_q.drain(0..final_dispatch_size).collect();
    //             let dispatch_size = orders.len();
    //             if dispatch_size > 0 {
    //                 handle.push_orders(orders).unwrap();
    //                 self.orders_submitted += dispatch_size;
    //             }
    //             final_dispatch_size
    //         })
    //         .zip(self.total_submitted.iter_mut())
    //         .for_each(|(final_dispatch, current_total)| {
    //             *current_total = *current_total + final_dispatch
    //         });
    //     self.loop_sleep_time = Duration::from_secs_f64(
    //         idle_times
    //             .iter()
    //             .min_by(|p, q| p.partial_cmp(q).unwrap_or(Ordering::Less))
    //             .copied()
    //             .map(|d| d / (5.0 * NUM_THREADS as f64))
    //             .unwrap_or(DEFAULT_EXECUTE_LOOP_SLEEP.as_secs_f64()),
    //     );
    //     self.loop_sleep_time_history.push(self.loop_sleep_time);
    //     sleep(self.loop_sleep_time);
    //     Ok(())
    // }

    fn print_handle_avg_info(&self) {
        let avg_infos = self
            .handles
            .iter()
            .map(|h| h.get_avg_info())
            .collect::<Vec<AvgInfoBundle>>();
        let info_with_summaries: AvgInfoWithSummaries = avg_infos.clone().into();

        println!(
            "{} (max: {}, min: {}, total: {})",
            "processing rates: ",
            info_with_summaries
                .summary_processing_rates
                .max
                .custom_display(),
            info_with_summaries
                .summary_processing_rates
                .min
                .custom_display(),
            info_with_summaries
                .summary_processing_rates
                .total
                .custom_display(),
        );

        println!(
            "in flight: {:?}",
            self.handles
                .iter()
                .map(|h| h.in_flight())
                .collect::<Vec<usize>>()
        );

        println!(
            "{} (max: {}, min: {}, total:{})",
            "task times: ",
            info_with_summaries.summary_task_times.max.custom_display(),
            info_with_summaries.summary_task_times.min.custom_display(),
            info_with_summaries
                .summary_task_times
                .total
                .custom_display(),
        );

        println!(
            "{} (max: {}, min: {}, total: {})",
            "idle times: ",
            info_with_summaries.summary_idle_times.max.custom_display(),
            info_with_summaries.summary_idle_times.min.custom_display(),
            info_with_summaries
                .summary_idle_times
                .total
                .custom_display(),
        );

        for (ix, avg_info_bundle) in avg_infos.iter().enumerate() {
            println!("{ix}: {avg_info_bundle}");
        }
    }

    fn print_status(&mut self) {
        if self
            .last_status_print
            .map(|t| (Instant::now() - t) > UPDATE_PRINT_DELAY)
            .unwrap_or(true)
        {
            let now = Instant::now();
            let run_time = self.start_time.elapsed();
            let minutes = run_time.as_secs() / 60;
            let seconds = run_time.as_secs() % 60;

            self.last_status_print = now.into();
            println!(
                "{} directories visited. {}/{} idle. Loop wait time: {}, Running for: {}:{}. Overall rate: {}",
                self.processed,
                self.handles.iter_mut().filter_map(|p| p.is_idle().then_some(1)).sum::<usize>(),
                self.handles.len(),
                self.loop_sleep_time.custom_display(),
                minutes,
                seconds,
                ((self.processed as f64) / run_time.as_secs_f64()).round()
            );

            self.print_handle_avg_info();

            println!(
                "sleep: {}",
                self.loop_sleep_time_history
                    .iter()
                    .map(|d| format!("{}", d.custom_display()))
                    .collect::<Vec<String>>()
                    .join(", ")
            );

            self.orders_submitted = 0;
        }
    }

    fn handle_print_requests(&self) {
        if let Some(print_receiver) = self.print_receiver.as_ref() {
            match print_receiver.try_recv() {
                Ok(print_requests) => {
                    for print_request in print_requests {
                        println!("{print_request}");
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => Err(WorkError::PrintSenderDisconnected).unwrap(),
            }
        }
    }

    fn process_results(&mut self) {
        for (max_dir_size, new_dirs_processed) in self.handles.iter_mut().filter_map(|p| {
            p.drain_results()
                .map(|max_dir_size| (max_dir_size, p.new_dirs_processed))
        }) {
            // println!("Got some new work!");
            if max_dir_size > self.max_dir_size {
                self.max_dir_size = max_dir_size;
                println!("Found a directory with {} entries.", self.max_dir_size);
            }
            self.processed += new_dirs_processed;
        }
    }

    fn execute(mut self) -> Result<usize, WorkError> {
        self.start_time = Instant::now();
        // Initial short sleep to ensure everyone is initialized and ready;
        // TODO: replace this with a status check on each handle
        sleep(Duration::from_millis(1));
        loop {
            self.handle_print_requests();

            self.process_results();
            match self.redistribute_work()? {
                RedistributeResult::SurplusRequestsSent => {}
                RedistributeResult::SurplusesDistributed => {}
                RedistributeResult::NoDistributionRequired => {}
                RedistributeResult::NoPathsInFlight => {
                    self.is_finished = self.handles.iter_mut().all(|h| {
                        h.update_status();
                        h.is_idle()
                    });
                }
            }
            self.print_status();

            if self.is_finished {
                let run_time = self.start_time.elapsed();
                let minutes = run_time.as_secs() / 60;
                let seconds = run_time.as_secs() % 60;
                println!(
                    "Done! {} directories visited. Ran for: {}:{}",
                    self.processed, minutes, seconds,
                );
                break;
            } else {
                self.update_loop_sleep_time();
                sleep(self.loop_sleep_time);
            }
        }
        for worker in self.handles.into_iter() {
            worker.finish()
        }
        Ok(self.max_dir_size)
    }
}

// fn map_io_error(path: &Path, io_err: IoErr) -> String {
//     format!("Could not open path: {path:?} due to error {io_err}")
// }

fn main() {
    let start = Instant::now();
    let manager = Executor::new(vec!["C:\\".into(), "A:\\".into(), "B:\\".into()], false);
    let result = manager.execute().unwrap();
    println!("Final max dir entry count: {}", result);
    println!("Took {}.", start.elapsed().custom_display());
}
