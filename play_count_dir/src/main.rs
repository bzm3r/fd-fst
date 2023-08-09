// #![cfg(feature = "debug_macros")]
#![feature(trace_macros)]
trace_macros!(true);

use paste::paste;
use std::cmp::Ordering;
use std::fmt::{Display, LowerExp};
use std::fs::{read_dir, DirEntry};
use std::io::Error as IoError;
use std::io::Result as IoResult;
use std::ops::{Add, Div, Mul, Sub};
use std::path::{Path, PathBuf};
// use std::slice::Iter as SliceIter;
use std::iter::Sum;
use std::sync::mpsc::{self, TryRecvError};
use std::sync::mpsc::{Receiver, SendError, Sender};
use std::thread::JoinHandle;
use std::thread::{self, sleep};
use std::time::{Duration, Instant};

#[macro_use]
mod macro_tools;

const NUM_THREADS: usize = 4;
const DEFAULT_EXECUTE_LOOP_SLEEP: Duration = Duration::from_micros(0);
const UPDATE_PRINT_DELAY: Duration = Duration::from_secs(5);
// const MAX_TOTAL_SUBMISSION: usize = NUM_THREADS * 500;
const MAX_HISTORY: usize = 10;
// const MIN_TARGET_SIZE: usize = 1;
const MAX_DISPATCH_SIZE: usize = 8;
const MAX_IN_FLIGHT: usize = 8;

trait SafeData<Inner>
where
    Self: Sized,
{
    fn zero() -> Self;

    fn inner(&self) -> Option<Inner>;
}

trait SafeInto<Target>
where
    Self: Sized,
{
    fn into_target(self) -> Target;
}

trait FromSafe<Safe>
where
    Safe: SafeInto<Self>,
    Self: Sized,
{
    fn target_from(safe: Safe) -> Self {
        safe.into_target()
    }
}

impl<Safe, Target> FromSafe<Safe> for Target where Safe: SafeInto<Self> {}

trait FromSource<T> {
    fn from_source(target: T) -> Self;
}

trait SourceInto<Safe>
where
    Self: Sized,
    Safe: FromSource<Self>,
{
    fn into_safe(source: Self) -> Safe {
        Safe::from_source(source)
    }
}

impl<Safe, Source> SourceInto<Safe> for Source where Safe: FromSource<Self> {}

macro_rules! impl_specific_source_into_safe {
    (Option ; $safe:ty, $inner:ty, create_specific) => {
        paste! {
            trait [< Into $safe For Opt $inner:camel >] where Self: Sized, $safe: FromSource<Self> {
                fn [< into_ $safe:snake:lower >](self) -> $safe {
                    $safe::from_source(self)
                }
            }

            impl [< Into $safe For Opt $inner:camel >] for Option<$inner> {}
        }
    };
    ($safe:ty, $source:ty, create_specific) => {
        paste! {
            trait [< Into $safe For $source:camel >] where Self: Sized, $safe: FromSource<Self> {
                fn [< into_ $safe:snake:lower >](self) -> $safe {
                    $safe::from_source(self)
                }
            }

            impl [< Into $safe For $source:camel >] for $source {}
        }
    };
    {_} => {};
}

macro_rules! impl_safe_from_source {
    (Option ; (|$x:ident : $inner:ty| -> $safe:ty { $source_to_safe:expr })$(, $create_specific:ident)?) => {
        impl FromSource<Option<$inner>> for $safe {
            fn from_source(source: Option<$inner>) -> Self {
                (|$x : Option<$inner>| -> $safe { $source_to_safe })(source)
            }
        }

        $(
            impl_specific_source_into_safe!(
                Option ;
                $safe,
                $inner,
                $create_specific
            );
        )?
    };
    ((|$x:ident : $source:ty| -> $safe:ty { $source_to_safe:expr })$(, $create_specific:ident)?) => {
        impl FromSource<$source> for $safe {
            fn from_source(source: $source) -> Self {
                (|$x : $source| -> $safe { $source_to_safe })(source)
            }
        }

        $(
            impl_specific_source_into_safe!(
                $safe,
                $source,
                $create_specific
            );
        )?
    };
}

macro_rules! impl_specific_target_from_safe {
    (Option; ($safe:ty, $inner:ty), create_specific) => {
        paste! {
            trait [< From $safe For Opt $inner:camel >] where Self: Sized, $safe: SafeInto<Option<$inner>> {
                fn [< from_ $safe:snake:lower >](safe: $safe) -> Option<$inner> {
                    safe.into_target()
                }
            }

            impl [< From $safe For Opt $inner:camel >] for Option<$inner> where $safe: SafeInto<Option<$inner>> {}
        }
    };
    ($safe:ty, $target:ty, create_specific) => {
        paste! {
            trait [< From $safe For $target:camel >] where Self: Sized, $safe: SafeInto<$target> {
                fn [< from_ $safe:snake:lower >](safe: $safe) -> $target {
                    safe.into_target()
                }
            }

            impl [< From $safe For $target:camel >] for $target where $safe: SafeInto<$target> {}
        }
    };
    (_) => {};
}

macro_rules! impl_safe_into_target {
    (
        (|$s:ident : $safe:ty| -> $target:ty { $safe_to_target:expr })$(, $create_specific:ident)?
    ) => {
        impl SafeInto<$target> for $safe {
            fn into_target(self) -> $target {
                (|$s: $safe| -> $target { $safe_to_target })(self)
            }
        }

        paste! {
            trait [< Into $target:camel For $safe:camel >] where Self: Sized + SafeInto<$target> {
                fn [< into_ $target:snake:lower >](self) -> $target {
                    self.into_target()
                }
            }

            impl [< Into $target:camel For $safe:camel >] for $safe {}
        }

        $(
            paste! {
                impl_specific_target_from_safe!(
                    $safe,
                    $target,
                    $create_specific
                );
            }
        )?
    };
    (
        Option ; (|$s:ident : $safe:ty| -> $inner:ty { $safe_to_target:expr })$(, $create_specific:ident)?
    ) => {
        impl SafeInto<Option<$inner>> for $safe {
            fn into_target(self) -> Option<$inner> {
                (|$s: $safe| -> Option<$inner> { $safe_to_target })(self)
            }
        }
        $(
            paste! {
                impl_specific_target_from_safe!(
                    Option;
                    ($safe,
                    $inner),
                    $create_specific
                );
            }
        )?
    };
}

macro_rules! create_safe_wrapper {
    (
        $safe:ty | $inner:ty ;
        [zero:$zero_expr:expr $(; default:$default_expr:expr)?] ;
        // $(Option ;)?|$x:ident : $source:ty| -> $safe_out:ty { $source_to_safe:expr }$(, $specific_into_safe:ident)?
        [from_source: $(($($from_source_args:tt)*))* ] ;
        // $(Option ;)?|$s:ident : $safe_inp:ty| -> $target:ty { $safe_to_target:expr }$(, $specific_from_safe:ident)?
        [into_target: $(($($into_target_args:tt)*))* ]
    ) => {
        paste! {
            #[derive(Clone, Copy, Debug)]
            struct [< $safe >](Option<$inner>);
        }

        paste! {
            $(
                impl_safe_from_source!($($from_source_args)*);
            )*
        }

        paste! {
            impl_safe_from_source!((|source: $safe| -> $safe { source }), create_specific);
            impl_safe_from_source!(Option ; (|source: $inner| -> $safe { $safe(source) }), create_specific);
        }

        paste! {
            impl_safe_into_target!(Option ; (|s: $safe| -> $inner { s.inner() }), create_specific);
            impl_safe_into_target!((|target: $safe| -> $safe { target }), create_specific);
        }

        paste! {
            $(
                impl_safe_into_target!($($into_target_args)*);
            )*
        }

        impl SafeData<$inner> for $safe {
            fn zero() -> Self {
                $zero_expr
            }

            fn inner(&self) -> Option<$inner> {
                self.0
            }
        }

        $(
            impl Default for $safe {
                fn default() -> Self {
                    $default_expr
                }
            }
        )?
    };
}

create_safe_wrapper!(SafeF64 | f64 ; [ zero:(0.0_f64).into_safe_f64() ; default:Self::zero() ] ;
        [from_source:
            ((|f: f64| -> SafeF64 { SafeF64((f.is_finite() && !f.is_nan()).then_some(f)) }), create_specific)
            ((|d: Duration| -> SafeF64 { SafeF64::from_source(d.as_secs_f64()) }), create_specific)
            ((|d: SafeDuration| -> SafeF64 { d.into_safe_f64() }), create_specific)
        ] ;
        [into_target:
            (Option ; (|s: SafeF64| -> Duration { s.inner().map(Duration::from_secs_f64) }), create_specific )
            ((|s: SafeF64| -> SafeDuration { SafeDuration(s.into_target()) }), create_specific )
            // (|s: SafeF64| -> SafeUsize { SafeUsize(s.inner().map(|f| f.round() as usize)) } )
         ]
);

create_safe_wrapper!(SafeDuration | Duration ; [zero: Duration::from_secs(0).into_safe_duration() ; default:Self::zero() ] ;
        [from_source:
            ((|d: Duration| -> SafeDuration {
                d.as_secs_f64().into_safe_duration()
            }), create_specific)
            ((|f: f64| -> SafeDuration { SafeF64::from_source(f).into_target() }), create_specific)
        ] ;
        [into_target:
            (Option ; (|d: SafeDuration| -> f64 { d.inner().as_ref().map(Duration::as_secs_f64) }), create_specific)
            ((|d: SafeDuration| -> SafeF64 { d.inner().map(|d| d.as_secs_f64()).into_safe_f64() }), create_specific)
            // (|s: SafeF64| -> SafeUsize { SafeUsize(s.inner().map(|f| f.round() as usize)) } )
         ]
);

// create_safe_wrapper!(SafeDuration | Duration ; [zero: Duration::from_secs(0).into_safe_duration() ; default:Self::zero() ] ;
//         [from_source:
//             (|d: Duration| -> SafeDuration {
//                 SafeDuration(
//                     SafeF64::from_source(
//                         d.as_secs_f64()
//                     ).into_opt_f64()
//                 )
//             })
//             (|f: f64| -> (SafeDuration) { SafeF64::from_source(f).into_target() })
//         ] ;
//         [into_target:
//             (|s: SafeDuration| -> (Option<f64>) { SafeF64::from_source(s).inner() })
//             (|s: SafeDuration| -> (SafeF64) { SafeF64::from_source(s.inner().map(|d| d.as_secs_f64())) })
//         ]
// );

fn main() {
    println!("hello world!");
}

// create_safe_wrapper!(
//     SafeUsize(usize) ; [zero:(0_usize).into_safe_usize() ; default:Self::zero() ] ;
//     [from_source:
//         (|u: usize| -> (SafeUsize) { SafeUsize(Some(u)) }),
//         (|f: f64| -> (SafeUsize) { f.into_safe_f64().into_target() } )
//     ] ;
//     [into_target: (|s: SafeUsize| -> (SafeF64) { s.inner().map(|u| u as f64).into_safe_f64() })]
// );

// impl PartialEq<SafeF64> for SafeF64 {
//     fn eq(&self, other: &SafeF64) -> bool {
//         match (self, other) {
//             (SafeF64(Some(s)), SafeF64(Some(o))) => s.partial_cmp(o).unwrap().is_eq(),
//             _ => false,
//         }
//     }
// }

// impl PartialEq<f64> for SafeF64 {
//     fn eq(&self, other: &f64) -> bool {
//         match (self, other) {
//             (SafeF64(Some(s)), o) => s.partial_cmp(o).unwrap().is_eq(),
//             _ => false,
//         }
//     }
// }

// impl PartialOrd<SafeF64> for SafeF64 {
//     fn partial_cmp(&self, other: &SafeF64) -> Option<Ordering> {
//         match (self, other) {
//             (SafeF64(Some(s)), SafeF64(Some(o))) => s.partial_cmp(o),
//             _ => None,
//         }
//     }
// }

// impl Eq for SafeF64 {}

// impl Ord for SafeF64 {
//     fn cmp(&self, other: &Self) -> Ordering {
//         self.partial_cmp(other).unwrap_or(Ordering::Less)
//     }
// }

// impl Add<SafeF64> for SafeF64 {
//     type Output = SafeF64;

//     fn add(self, rhs: SafeF64) -> Self::Output {
//         match (self.inner(), rhs.inner()) {
//             (Some(s), Some(o)) => (s + o).into_safe(),
//             _ => None.into_safe(),
//         }
//     }
// }

// impl Add<SafeDuration> for SafeDuration {
//     type Output = SafeDuration;

//     fn add(self, rhs: SafeDuration) -> Self::Output {
//         match (self.inner(), rhs.inner()) {
//             (Some(s), Some(o)) => (s + o).into_safe(),
//             _ => None.into_safe(),
//         }
//     }
// }

// impl Mul<SafeF64> for SafeF64 {
//     type Output = SafeF64;

//     fn mul(self, rhs: SafeF64) -> Self::Output {
//         match (self.inner(), rhs.inner()) {
//             (Some(s), Some(o)) => (s * o).into_safe(),
//             _ => None.into_safe(),
//         }
//     }
// }

// impl Mul<SafeDuration> for SafeDuration {
//     type Output = SafeF64;

//     fn mul(self, rhs: SafeDuration) -> Self::Output {
//         match (
//             SafeF64::from_source(self).inner(),
//             SafeF64::from_source(rhs).inner(),
//         ) {
//             (Some(s), Some(o)) => (s * o).into_safe(),
//             _ => None.into_safe(),
//         }
//     }
// }

// impl Sub<SafeF64> for SafeF64 {
//     type Output = SafeF64;

//     fn sub(self, rhs: SafeF64) -> Self::Output {
//         self + (SafeF64::from_source(-1.0_f64) * rhs)
//     }
// }

// impl Sub<SafeDuration> for SafeDuration {
//     type Output = SafeDuration;

//     fn sub(self, rhs: SafeDuration) -> Self::Output {
//         match (self.inner(), rhs.inner()) {
//             (Some(lhs), Some(rhs)) => (lhs > rhs).then_some(lhs - rhs).into_safe(),
//             _ => SafeDuration(None),
//         }
//     }
// }

// impl Div<SafeF64> for SafeF64 {
//     type Output = SafeF64;

//     fn div(self, rhs: SafeF64) -> Self::Output {
//         match (self.inner(), rhs.inner()) {
//             (Some(s), Some(o)) => (s / o).into_safe(),
//             (None, _) => self,
//             // rest of the cases should be symmetric, as we can rely on symmetry of multiplication
//             _ => rhs / self,
//         }
//     }
// }

// impl Div<SafeDuration> for SafeDuration {
//     type Output = SafeF64;

//     fn div(self, rhs: SafeDuration) -> Self::Output {
//         let lhs: SafeF64 = self.into_target();
//         let rhs: SafeF64 = self.into_target();
//         lhs / rhs
//     }
// }

// impl Div<usize> for SafeF64 {
//     type Output = SafeF64;

//     fn div(self, rhs: usize) -> Self::Output {
//         match self {
//             SafeF64(Some(s)) => (s / (rhs as f64)).into_safe(),
//             SafeF64(None) => self,
//         }
//     }
// }

// impl Div<usize> for SafeDuration {
//     type Output = SafeDuration;

//     fn div(self, rhs: usize) -> Self::Output {
//         let safe_f64: SafeF64 = self.into_target();
//         (safe_f64 / rhs).into_target()
//     }
// }

// impl Div<usize> for SafeUsize {
//     type Output = SafeUsize;
//     fn div(self, rhs: usize) -> Self::Output {
//         let safe_f64: SafeF64 = self.into_target();
//         (safe_f64 / rhs).inner().map(|f| f.round()).into_safe()
//     }
// }

// impl Display for SafeF64 {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         let output = match self {
//             SafeF64(Some(x)) => format!("{}", x),
//             _ => "None".into(),
//         };
//         write!(f, "{}", output)
//     }
// }

// impl LowerExp for SafeF64 {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         let output = match self {
//             SafeF64(Some(x)) => format!("{:e}", x),
//             _ => "None".into(),
//         };
//         write!(f, "{}", output)
//     }
// }

// impl Sum for SafeF64 {
//     fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
//         SafeF64(iter.filter_map(|s| s.0).sum::<f64>().into())
//     }
// }

// trait RoundSigFigs
// where
//     Self: Copy + Clone + SafeInto<SafeF64>,
// {
//     fn delta_shift(x: SafeF64) -> Option<i32> {
//         x.0.map(|f| f.abs().log10().ceil() as i32)
//     }

//     fn round_sig_figs(&self, n_sig_figs: i32) -> Option<String> {
//         let x = self.into_target();
//         let rounded: SafeF64 = if x == 0. || n_sig_figs == 0 {
//             (0.0_f64).into_safe()
//         } else if let Some(delta_shift) = Self::delta_shift(x) {
//             let shift = n_sig_figs - delta_shift;
//             let shift_factor = 10_f64.powi(shift).into_safe();
//             SafeF64::from_source((x * shift_factor).inner().map(f64::round)) / shift_factor
//         } else {
//             None.into_safe()
//         };

//         rounded.inner().map(|r| format!("{:e}", r))
//     }
// }

// impl RoundSigFigs for SafeF64 {}
// impl RoundSigFigs for SafeDuration {}

// #[derive(PartialEq, Eq, Copy, Clone)]
// enum Status {
//     Busy,
//     Idle,
// }

// struct BufferedSender<T> {
//     buffer: Vec<T>,
//     sender: Sender<Vec<T>>,
// }

// impl<T> BufferedSender<T> {
//     fn new(sender: Sender<Vec<T>>, initial_capacity: usize) -> Self {
//         BufferedSender {
//             buffer: Vec::with_capacity(initial_capacity),
//             sender,
//         }
//     }
//     fn push(&mut self, value: T) {
//         self.buffer.push(value)
//     }

//     fn flush_send(&mut self) -> Result<(), SendError<Vec<T>>> {
//         self.sender.send(self.buffer.drain(0..).collect())
//     }
// }

// #[derive(Default)]
// struct Printer(Option<BufferedSender<String>>);

// impl Printer {
//     fn new(sender: Option<Sender<Vec<String>>>) -> Self {
//         if let Some(sender) = sender {
//             Printer(Some(BufferedSender::new(sender, 20)))
//         } else {
//             Printer(None)
//         }
//     }

//     fn push(&mut self, lazy_value: impl FnOnce() -> String) {
//         self.0
//             .as_mut()
//             .map(|buf_sender| buf_sender.push(lazy_value()))
//             .unwrap_or(());
//     }

//     fn flush_send(&mut self) -> Result<(), SendError<Vec<String>>> {
//         self.0
//             .as_mut()
//             .map(|buf_sender| buf_sender.flush_send())
//             .unwrap_or(Ok(()))
//     }

//     // fn is_some(&self) -> bool {
//     //     self.0.is_some()
//     // }
// }

// #[derive(Debug, Clone)]
// struct HistoryVec<
//     Safe: Default
//         + Clone
//         + SafeData<Data>
//         + FromSource<Data>
//         + Sub<Safe, Output = Safe>
//         + Add<Safe, Output = Safe>
//         + Div<usize, Output = Safe>,
//     Data: Default + Clone + Add<Data, Output = Data> + Sub<Data, Output = Data>,
// > {
//     capacity: usize,
//     inner: Vec<Data>,
//     // current_sum: T,
//     average: Safe,
// }

// impl<
//         Safe: Default
//             + Clone
//             + SafeData<Data>
//             + FromSource<Data>
//             + Sub<Safe, Output = Safe>
//             + Add<Safe, Output = Safe>
//             + Div<usize, Output = Safe>,
//         Data: Default + Clone + Add<Data, Output = Data> + Sub<Data, Output = Data>,
//     > Default for HistoryVec<Safe, Data>
// {
//     fn default() -> Self {
//         HistoryVec {
//             capacity: MAX_HISTORY,
//             inner: Vec::with_capacity(MAX_HISTORY),
//             average: Safe::zero(),
//         }
//     }
// }

// impl<
//         Safe: Default
//             + Clone
//             + SafeData<Data>
//             + FromSource<Data>
//             + Sub<Safe, Output = Safe>
//             + Add<Safe, Output = Safe>
//             + Div<usize, Output = Safe>,
//         Data: Default + Clone + Add<Data, Output = Data> + Sub<Data, Output = Data>,
//     > HistoryVec<Safe, Data>
// {
//     fn push(&mut self, k: Data) {
//         // We want to find a number kx such that kx + curr_avg is the new,
//         // updated average.
//         let kx: Safe = if self.inner.len() == self.capacity {
//             let k_prime = self.inner.pop().unwrap();
//             //     incremental update of average
//             //     (q + k_prime)/n  +  kx          = (q + k)/n
//             // ==  (q + k_prime)/n  -  (q + k)/n   = -kx
//             // ==  (q + k_prime)/n  -  (q + k)/n   = -kx
//             // ==  (k_prime - k)/n                 = -kx
//             // ==  (k - k_prime)/n                 =  kx
//             // current_average + kx = (current_sum - k_prime + k)/len
//             let delta_k: Safe = Safe::from_source(k) - Safe::from_source(k_prime);
//             delta_k / self.inner.len()
//         } else {
//             //     incremental update of average
//             //     q/(n - 1)  +  kx = (q + k)/n
//             //
//             // We have a list of (n - 1) numbers, with average q/(n - 1). To this list,
//             // can we add another number (the nth number), such that the new average is
//             // still q/(n - 1)? Well, we know that if this new number *is* q/(n - 1),
//             // then the average will remain unchanged:
//             //
//             //    (q + (q/n - 1))/n
//             // == ((n - 1)q + q) / n(n - 1)
//             // == (qn - q + q)/n(n - 1)
//             // == qn / (n(n-1))
//             // == q/(n - 1)
//             //
//             // Now we have a list of n numbers, with the average q/(n - 1),
//             // and we can use the formula derived for the preceding if statement
//             // as follows to get that the increment should be (k - curr_avg)/n:
//             //     (k - curr_avg)/n + curr_avg
//             // ==  (k - curr_avg + n*curr_avg)/n
//             // ==  (k + curr_avg * (n - 1))/n
//             // ==  (k + q)/n
//             let n = self.inner.len() + 1;
//             // Intriguingly, this formula also works for the case where self.inner.len() == 0
//             (Safe::from_source(k) - self.average) / n
//         };
//         self.average = self.average + kx.into();
//         self.inner.push(k);
//     }

//     fn maybe_push(&mut self, k: Option<Data>) {
//         if let Some(k) = k {
//             self.push(k);
//         }
//     }

//     fn last(&self) -> Data {
//         self.inner[self.inner.len() - 1]
//     }

//     fn iter(&self) -> std::slice::Iter<Data> {
//         self.inner.iter()
//     }
// }

// macro_rules! create_paired_comm {
//     (
//         $name:ident ;
//         LHS: $(($fid:ident, $fty:ty)),+ ;
//         RHS: $(($gid:ident, $gty:ty)),+
//     ) => {
//         paste! {
//             struct $name {
//                 $([< $fid _sender >]: Sender<$fty>,)+
//                 $([< $gid _receiver >]: Receiver<$gty>,)+
//             }
//         }
//     }
// }

// macro_rules! create_paired_comms {
//     (
//         [ $lhs_snake_name:ident ; $lhs_struct_id:ident ; $(($fid:ident, $fty:ty)),+ ] <->
//         [ $rhs_snake_name:ident ; $rhs_struct_id:ident ; $(($gid:ident, $gty:ty)),+ ]
//     ) => {
//         create_paired_comm!(
//             $lhs_struct_id ;
//             LHS: $(($fid, $fty)),+ ;
//             RHS: $(($gid, $gty)),+
//         );
//         create_paired_comm!(
//             $rhs_struct_id ;
//             LHS: $(($gid, $gty)),+ ;
//             RHS: $(($fid, $fty)),+
//         );

//         paste! {
//             fn [< new_ $lhs_snake_name _to_ $rhs_snake_name _comms>]() ->
//                 ($lhs_struct_id, $rhs_struct_id)
//             {
//                 $(let ([< $fid _sender>], [< $fid _receiver>]) = mpsc::channel();)+
//                 $(let ([< $gid _sender>], [< $gid _receiver>]) = mpsc::channel();)+

//                 (
//                     $lhs_struct_id {
//                         $([< $fid _sender >],)+
//                         $([< $gid _receiver >],)+
//                     },
//                     $rhs_struct_id {
//                         $([< $gid _sender >],)+
//                         $([< $fid _receiver >],)+
//                     }
//                 )
//             }
//         }
//     }
// }

// #[derive(Debug)]
// enum WorkError {
//     StatusRequestSendError,
//     PrintSenderDisconnected,
// }

// impl<T> From<SendError<T>> for WorkError {
//     fn from(_: SendError<T>) -> Self {
//         Self::StatusRequestSendError
//     }
// }

// // struct WorkRequest {
// //     size: usize,
// // }

// #[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Eq, Ord)]
// struct TimeStamp(Option<Instant>);

// impl TimeStamp {
//     #[inline]
//     fn mark(&mut self) {
//         self.0.replace(Instant::now());
//     }
// }

// impl Sub<TimeStamp> for TimeStamp {
//     type Output = Option<Duration>;
//     fn sub(self, rhs: TimeStamp) -> Self::Output {
//         self.0.and_then(|t1| rhs.0.and_then(|t0| (t1 - t0).into()))
//     }
// }

// #[derive(Clone, Debug)]
// struct WorkResults {
//     history_count: usize,
//     avg_task_time: SafeF64,
//     avg_idle_time: SafeF64,
//     avg_processing_rate: SafeF64,
//     dirs_processed: usize,
//     max_dir_size: usize,
//     discovered: Vec<PathBuf>,
// }

// impl Default for WorkResults {
//     fn default() -> Self {
//         WorkResults {
//             history_count: 0,
//             avg_task_time: SafeF64::zero(),
//             avg_idle_time: SafeF64::zero(),
//             avg_processing_rate: SafeF64::zero(),
//             dirs_processed: 0,
//             max_dir_size: 0,
//             discovered: Vec::new(),
//         }
//     }
// }

// impl WorkResults {
//     fn merge(mut self, next: WorkResults) -> Self {
//         self.avg_idle_time = next.avg_idle_time;
//         self.avg_task_time = next.avg_task_time;
//         self.avg_processing_rate = next.avg_processing_rate;
//         self.dirs_processed += next.dirs_processed;
//         self.max_dir_size = self.max_dir_size.max(next.max_dir_size);
//         self.discovered.extend(next.discovered.into_iter());
//         self
//     }
// }

// impl std::iter::Sum for WorkResults {
//     fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
//         iter.reduce(|a, b| a.merge(b)).unwrap_or_default()
//     }
// }

// #[derive(Default, Clone, Debug)]
// struct ResultsBuffer {
//     max_dir_size: usize,
//     discovered: Vec<PathBuf>,
// }

// #[derive(Default, Clone, Copy, Debug)]
// struct TaskHistory {
//     time_taken: Duration,
//     idle_time: Duration,
//     dirs_processed: usize,
//     processing_rate: f64,
// }

// #[derive(Default, Copy, Clone, Debug)]
// struct SafeTaskHistory {
//     time_taken: SafeDuration,
//     idle_time: SafeDuration,
//     dirs_processed: SafeUsize,
//     processing_rate: SafeF64,
// }

// impl SafeData<TaskHistory> for SafeTaskHistory {
//     fn zero() -> Self {
//         SafeTaskHistory {
//             time_taken: SafeDuration::zero(),
//             idle_time: SafeDuration::zero(),
//             dirs_processed: SafeUsize::zero(),
//             processing_rate: SafeF64::zero(),
//         }
//     }

//     fn inner(&self) -> Option<TaskHistory> {
//         let SafeTaskHistory {
//             time_taken,
//             idle_time,
//             dirs_processed,
//             processing_rate,
//         } = *self;
//         time_taken.inner().and_then(|tt| {
//             idle_time.inner().and_then(|it| {
//                 processing_rate.inner().and_then(|pr| {
//                     dirs_processed.inner().and_then(|dp| {
//                         Some(TaskHistory {
//                             time_taken: tt,
//                             idle_time: it,
//                             processing_rate: pr,
//                             dirs_processed: dp,
//                         })
//                     })
//                 })
//             })
//         })
//     }
// }

// impl FromSource<TaskHistory> for SafeTaskHistory {
//     fn from_source(target: TaskHistory) -> Self {
//         SafeTaskHistory {
//             time_taken: target.time_taken.into_safe(),
//             idle_time: target.idle_time.into_safe(),
//             dirs_processed: target.dirs_processed.into_safe(),
//             processing_rate: target.processing_rate.into_safe(),
//         }
//     }
// }

// impl SafeInto<Option<TaskHistory>> for SafeTaskHistory {
//     fn into_target(self) -> Option<TaskHistory> {
//         self.inner()
//     }
// }

// impl Add<TaskHistory> for TaskHistory {
//     type Output = TaskHistory;
//     fn add(self, rhs: TaskHistory) -> Self::Output {
//         TaskHistory {
//             time_taken: self.time_taken + rhs.time_taken,
//             idle_time: self.idle_time + rhs.idle_time,
//             dirs_processed: self.dirs_processed + rhs.dirs_processed,
//             processing_rate: self.processing_rate + rhs.processing_rate,
//         }
//     }
// }

// impl Sub<TaskHistory> for TaskHistory {
//     type Output = TaskHistory;
//     fn sub(self, rhs: TaskHistory) -> Self::Output {
//         TaskHistory {
//             time_taken: self.time_taken - rhs.time_taken,
//             idle_time: self.idle_time - rhs.idle_time,
//             dirs_processed: self.dirs_processed - rhs.dirs_processed,
//             processing_rate: self.processing_rate - rhs.processing_rate,
//         }
//     }
// }

// impl Div<usize> for TaskHistory {
//     type Output = SafeTaskHistory;
//     fn div(self, rhs: usize) -> Self::Output {
//         SafeTaskHistory {
//             time_taken: (self.time_taken.into_safe()) / rhs,
//             idle_time: (self.idle_time.into_safe()) / rhs,
//             dirs_processed: (self.dirs_processed.into_safe()) / rhs,
//             processing_rate: (self.processing_rate.into_safe()) / rhs,
//         }
//     }
// }

// impl Add<SafeTaskHistory> for SafeTaskHistory {
//     type Output = SafeTaskHistory;
//     fn add(self, rhs: SafeTaskHistory) -> Self::Output {
//         SafeTaskHistory {
//             time_taken: self.time_taken + rhs.time_taken,
//             idle_time: self.idle_time + rhs.idle_time,
//             dirs_processed: self.dirs_processed + rhs.dirs_processed,
//             processing_rate: self.processing_rate + rhs.processing_rate,
//         }
//     }
// }

// impl Sub<SafeTaskHistory> for SafeTaskHistory {
//     type Output = SafeTaskHistory;
//     fn sub(self, rhs: TaskHistory) -> Self::Output {
//         SafeTaskHistory {
//             time_taken: self.time_taken - rhs.time_taken,
//             idle_time: self.idle_time - rhs.idle_time,
//             dirs_processed: self.dirs_processed - rhs.dirs_processed,
//             processing_rate: self.processing_rate - rhs.processing_rate,
//         }
//     }
// }

// impl Div<usize> for SafeTaskHistory {
//     type Output = SafeTaskHistory;
//     fn div(self, rhs: usize) -> Self::Output {
//         SafeTaskHistory {
//             time_taken: self.time_taken / rhs,
//             idle_time: self.idle_time / rhs,
//             dirs_processed: self.dirs_processed / rhs,
//             processing_rate: self.processing_rate / rhs,
//         }
//     }
// }

// #[derive(Default)]
// struct ThreadHistory {
//     historical_data: HistoryVec<SafeTaskHistory, TaskHistory>,
//     current: TaskHistory,
//     t_started: TimeStamp,
//     t_finished: TimeStamp,
//     t_idling: TimeStamp,
// }

// impl ThreadHistory {
//     fn mark_idling(&mut self) {
//         self.t_idling.mark();
//     }

//     fn mark_started(&mut self) {
//         self.t_started.mark();
//         self.historical_data
//             .push_idle_time(self.t_started - self.t_idling);
//     }

//     fn mark_finished(&mut self, dirs_processed: usize) {
//         self.t_finished.mark();
//         self.task_times.maybe_push(self.t_finished - self.t_started);
//         self.dirs_processed.push(dirs_processed);
//         self.processing_rates
//             .push(dirs_processed.into() / self.task_times.last().into());
//         self.mark_idling();
//     }

//     fn count(&self) -> usize {
//         self.history.len()
//     }
// }

// create_paired_comms!(
//     [handle ;  ThreadHandleComms ; (order, Vec<PathBuf>)] <->
//     [thread ; ThreadComms ; (status, Status), (result, WorkResults) ]
// );

// struct Thread {
//     comms: ThreadComms,
//     printer: Printer,
//     status: Status,
//     history: ThreadHistory,
//     results_buffer: ResultsBuffer,
// }

// struct ThreadHandle {
//     comms: ThreadHandleComms,
//     status: Status,
//     worker_thread: JoinHandle<()>,
//     in_flight: usize,
//     orders: Vec<PathBuf>,
//     dirs_processed: usize,
//     avg_task_time: SafeF64,
//     avg_idle_time: SafeF64,
//     avg_processing_rate: SafeF64,
// }

// struct Executor {
//     work_q: Vec<PathBuf>,
//     print_receiver: Option<Receiver<Vec<String>>>,
//     handles: Vec<ThreadHandle>,
//     max_dir_size: usize,
//     last_status_print: Option<Instant>,
//     start_time: Instant,
//     processed: usize,
//     orders_submitted: usize,
//     loop_sleep_time: Duration,
//     loop_sleep_time_history: HistoryVec<SafeDuration, Duration>,
// }

// impl Thread {
//     fn new(comms: ThreadComms, print_sender: Option<Sender<Vec<String>>>) -> Self {
//         comms.status_sender.send(Status::Idle).unwrap();
//         Self {
//             comms,
//             printer: Printer::new(print_sender),
//             status: Status::Idle,
//             history: ThreadHistory::default(),
//             results_buffer: ResultsBuffer::default(),
//         }
//     }

//     fn send_status(&self) -> Result<(), WorkError> {
//         self.comms.status_sender.send(self.status)?;
//         Ok(())
//     }

//     fn change_status(&mut self, new_status: Status) -> Result<(), WorkError> {
//         self.status = new_status;
//         self.send_status()
//     }

//     fn start(mut self) {
//         // Just the initial idling mark to get us started, otherwise,
//         // `history.mark_finished` handles calling mark_idling for us.
//         self.history.mark_idling();
//         loop {
//             self.printer.flush_send().unwrap();

//             let work_order = self.comms.order_receiver.recv().unwrap();

//             self.history.mark_started();
//             self.change_status(Status::Busy).unwrap();

//             for path in work_order.iter() {
//                 let dir_entries = read_dir(path)
//                     .and_then(|read_iter| read_iter.collect::<IoResult<Vec<DirEntry>>>())
//                     .unwrap_or_else(|err| {
//                         self.printer.push(|| map_io_error(path, err));
//                         vec![]
//                     });

//                 if dir_entries.len() > 0 {
//                     self.results_buffer.max_dir_size = dir_entries.len().max(dir_entries.len());
//                     self.results_buffer
//                         .discovered
//                         .extend(dir_entries.iter().filter_map(|entry| {
//                             entry
//                                 .metadata()
//                                 .ok()
//                                 .and_then(|meta| meta.is_dir().then_some(entry.path()))
//                         }));
//                     self.printer
//                         .push(|| format!("Processed {:?}", &path.as_os_str(),));
//                 }
//             }
//             self.history.mark_finished(work_order.len());
//             self.comms
//                 .result_sender
//                 .send(WorkResults {
//                     history_count: self.history.count(),
//                     avg_task_time: self.history.task_times.average,
//                     avg_idle_time: self.history.idle_times.average,
//                     max_dir_size: self.results_buffer.max_dir_size,
//                     discovered: self.results_buffer.discovered.drain(0..).collect(),
//                     avg_processing_rate: self.history.processing_rates.average,
//                     dirs_processed: self.history.dirs_processed.last(),
//                 })
//                 .unwrap();
//             self.comms.status_sender.send(Status::Idle).unwrap();
//         }
//     }
// }

// impl ThreadHandle {
//     fn new(print_sender: Option<Sender<Vec<String>>>) -> Self {
//         let (handle_comms, process_thread_comms) = new_handle_to_thread_comms();
//         let worker = Thread::new(process_thread_comms, print_sender);

//         ThreadHandle {
//             comms: handle_comms,
//             status: Status::Idle,
//             worker_thread: thread::spawn(move || worker.start()),
//             in_flight: 0,
//             dirs_processed: 0,
//             avg_task_time: SafeF64::default(),
//             avg_idle_time: SafeF64::default(),
//             avg_processing_rate: SafeF64::default(),
//             orders: vec![],
//         }
//     }

//     fn get_avg_task_time(&self) -> SafeF64 {
//         self.avg_task_time
//     }

//     fn get_avg_idle_time(&self) -> SafeF64 {
//         self.avg_idle_time
//     }

//     fn get_avg_processing_rate(&self) -> SafeF64 {
//         self.avg_processing_rate
//     }

//     fn drain_orders(&mut self) -> Vec<PathBuf> {
//         let order: Vec<PathBuf> = self.orders.drain(0..).collect();
//         self.in_flight += order.len();
//         order
//     }

//     fn queue_orders(&mut self, orders: Vec<PathBuf>) {
//         self.orders.extend(orders.into_iter());
//     }

//     fn dispatch_orders(&mut self, orders: Vec<PathBuf>) -> Result<(), WorkError> {
//         if self.is_idle() {
//             self.in_flight += orders.len();
//             self.comms.order_sender.send(orders)?;
//             let drained_orders = self.drain_orders();
//             self.comms.order_sender.send(drained_orders)?;
//         } else {
//             self.queue_orders(orders);
//         }
//         Ok(())
//     }

//     fn push_orders(&mut self, orders: Vec<PathBuf>) -> Result<(), WorkError> {
//         if self.is_idle() {
//             self.dispatch_orders(orders)?;
//         } else {
//             self.queue_orders(orders);
//         }
//         Ok(())
//     }

//     fn update_dirs_processed(&mut self, dirs_processed: usize) {
//         self.dirs_processed += dirs_processed;
//         self.in_flight -= dirs_processed;
//     }

//     fn drain_results(&mut self) -> (usize, Vec<PathBuf>) {
//         let WorkResults {
//             avg_task_time,
//             avg_idle_time,
//             avg_processing_rate,
//             dirs_processed,
//             max_dir_size,
//             discovered,
//             history_count,
//         } = self.comms.result_receiver.try_iter().sum::<WorkResults>();

//         self.avg_task_time = avg_task_time;
//         self.avg_idle_time = avg_idle_time;
//         self.avg_processing_rate = avg_processing_rate;
//         self.update_dirs_processed(dirs_processed);
//         (max_dir_size, discovered)
//     }

//     fn currently_submitted(&self) -> usize {
//         self.in_flight + self.orders.len()
//     }

//     fn update_status(&mut self) {
//         self.status = self
//             .comms
//             .status_receiver
//             .try_iter()
//             .last()
//             .unwrap_or(self.status);
//     }

//     fn is_idle(&mut self) -> bool {
//         self.update_status();
//         self.status == Status::Idle
//     }

//     fn finish(self) {
//         self.worker_thread.join().unwrap();
//     }
// }

// impl Executor {
//     fn new(work: Vec<PathBuf>, verbose: bool) -> Self {
//         let (print_sender, print_receiver) = if verbose {
//             let (print_sender, print_receiver) = mpsc::channel();
//             (Some(print_sender), Some(print_receiver))
//         } else {
//             (None, None)
//         };

//         let handles = (0..NUM_THREADS)
//             .map(|_| ThreadHandle::new(print_sender.clone()))
//             .collect();

//         Self {
//             work_q: work,
//             print_receiver,
//             handles,
//             max_dir_size: 0,
//             last_status_print: None,
//             start_time: Instant::now(),
//             processed: 0,
//             orders_submitted: 0,
//             loop_sleep_time: DEFAULT_EXECUTE_LOOP_SLEEP,
//             loop_sleep_time_history: HistoryVec::default(),
//         }
//     }

//     fn all_handles_idle(&mut self) -> bool {
//         self.handles.iter_mut().all(ThreadHandle::is_idle)
//     }

//     fn distribute_work(&mut self) -> Result<(), WorkError> {
//         for handle in self.handles.iter_mut() {
//             if handle.is_idle() {
//                 let will_submit = self.work_q.len().min(MAX_DISPATCH_SIZE);
//                 handle.push_orders(self.work_q.drain(0..will_submit).collect())?;
//                 self.orders_submitted += will_submit;
//             }
//         }
//         let mut avg_idle_times = self
//             .handles
//             .iter()
//             .map(|h| h.avg_idle_time)
//             .collect::<Vec<SafeF64>>();
//         avg_idle_times.sort_unstable_by(|p, q| p.partial_cmp(q).unwrap_or(Ordering::Less));
//         self.loop_sleep_time = (avg_idle_times.len() > 0)
//             .then_some(avg_idle_times[0] * 0.2_f64.into())
//             .and_then(|s| Duration::target_from(s))
//             .unwrap_or(DEFAULT_EXECUTE_LOOP_SLEEP);
//         self.loop_sleep_time_history.push(self.loop_sleep_time);
//         sleep(self.loop_sleep_time);
//         Ok(())
//     }

//     fn print_handle_avg_info<
//         T: Clone + Copy + PartialOrd + Display + Sum + LowerExp + RoundSigFigs,
//     >(
//         &self,
//         title: &'static str,
//         avg_info_fetch: fn(&ThreadHandle) -> T,
//         n_sig_figs: i32,
//         print_total: bool,
//     ) {
//         let avg_info: Vec<T> = self.handles.iter().map(avg_info_fetch).collect();
//         let mut sorted = avg_info.clone();
//         sorted.sort_unstable_by(|p, q| {
//             (p < q)
//                 .then_some(Ordering::Less)
//                 .unwrap_or(Ordering::Greater)
//         });
//         println!(
//             "{} (max: {}, min: {}{}): {}",
//             title,
//             sorted[sorted.len() - 1].round_sig_figs(n_sig_figs),
//             sorted[0].round_sig_figs(n_sig_figs),
//             print_total
//                 .then_some(format!(
//                     ", total: {}",
//                     avg_info
//                         .iter()
//                         .copied()
//                         .sum::<T>()
//                         .round_sig_figs(n_sig_figs)
//                 ))
//                 .unwrap_or("".into()),
//             avg_info
//                 .iter()
//                 .map(|t| t.round_sig_figs(n_sig_figs))
//                 .collect::<Vec<String>>()
//                 .join(", ")
//         );
//     }

//     fn print_status(&mut self) {
//         if self
//             .last_status_print
//             .map(|t| (Instant::now() - t) > UPDATE_PRINT_DELAY)
//             .unwrap_or(true)
//         {
//             let now = Instant::now();
//             let run_time = now - self.start_time;
//             let minutes = run_time.as_secs() / 60;
//             let seconds = run_time.as_secs() % 60;

//             self.last_status_print = now.into();
//             println!(
//                 "{} directories visited. {} new orders submitted. {}/{} idle. Loop wait time: {:e}, Running for: {}:{}. Overall rate: {}",
//                 self.processed,
//                 self.orders_submitted,
//                 self.handles.iter_mut().filter_map(|p| p.is_idle().then_some(1)).sum::<usize>(),
//                 self.handles.len(),
//                 self.loop_sleep_time.as_secs_f64(),
//                 minutes,
//                 seconds,
//                 ((self.processed as f64) / run_time.as_secs_f64()).round()
//             );

//             self.print_handle_avg_info(
//                 "processing rates",
//                 ThreadHandle::get_avg_processing_rate,
//                 3,
//                 true,
//             );

//             println!(
//                 "sleep: {}",
//                 self.loop_sleep_time_history
//                     .iter()
//                     .map(|d| format!("{}", d.round_sig_figs(4)))
//                     .collect::<Vec<String>>()
//                     .join(", ")
//             );

//             self.print_handle_avg_info("task times", ThreadHandle::get_avg_task_time, 4, true);

//             self.print_handle_avg_info("idle times", ThreadHandle::get_avg_idle_time, 4, true);

//             self.print_handle_avg_info("in flight", ThreadHandle::currently_submitted, 4, true);

//             self.orders_submitted = 0;
//         }
//     }

//     fn handle_print_requests(&self) {
//         if let Some(print_receiver) = self.print_receiver.as_ref() {
//             match print_receiver.try_recv() {
//                 Ok(print_requests) => {
//                     for print_request in print_requests {
//                         println!("{print_request}");
//                     }
//                 }
//                 Err(TryRecvError::Empty) => {}
//                 Err(TryRecvError::Disconnected) => Err(WorkError::PrintSenderDisconnected).unwrap(),
//             }
//         }
//     }

//     fn process_results(&mut self) {
//         for (max_dir_size, discovered) in self.handles.iter_mut().map(|p| p.drain_results()) {
//             // println!("Got some new work!");
//             if max_dir_size > self.max_dir_size {
//                 self.max_dir_size = max_dir_size;
//                 println!("Found a directory with {} entries.", self.max_dir_size);
//             }
//             self.work_q.extend(discovered.into_iter())
//         }
//         self.processed = self.handles.iter().map(|p| p.dirs_processed).sum();
//     }

//     fn execute(mut self) -> Result<usize, WorkError> {
//         self.start_time = Instant::now();
//         loop {
//             self.handle_print_requests();
//             self.process_results();
//             if self.work_q.len() > 0 {
//                 self.distribute_work()?;
//             } else if self.all_handles_idle() {
//                 self.process_results();
//                 if self.work_q.len() > 0 {
//                     self.distribute_work()?;
//                 } else {
//                     let now = Instant::now();
//                     let run_time = now - self.start_time;
//                     let minutes = run_time.as_secs() / 60;
//                     let seconds = run_time.as_secs() % 60;
//                     println!(
//                         "Done! {} directories visited. Ran for: {}:{}",
//                         self.processed, minutes, seconds,
//                     );
//                     break;
//                 }
//             }
//             self.print_status();
//         }
//         for worker in self.handles.into_iter() {
//             worker.finish()
//         }
//         Ok(self.max_dir_size)
//     }
// }

// fn map_io_error(path: &Path, io_err: IoError) -> String {
//     format!("Could not open path: {path:?} due to error {io_err}")
// }

// fn main() {
//     let start = Instant::now();
//     let manager = Executor::new(vec!["C:\\".into(), "A:\\".into(), "B:\\".into()], false);
//     let result = manager.execute().unwrap();
//     println!("Final max dir entry count: {}", result);
//     let end = Instant::now();
//     println!("Took {} seconds.", (end - start).as_secs());
// }
