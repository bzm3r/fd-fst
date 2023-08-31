use crate::display::CustomDisplay;
use paste::paste;
use std::error::Error;
use std::fmt::Display;
use std::fmt::Result as FmtRes;

pub type NumResult<T> = Result<T, NumErr>;

pub trait FiniteTest: Sized {
    fn test_finite(&self) -> NumResult<&Self>;
}

pub trait NonNegTest: Sized {
    fn test_non_neg(&self) -> NumResult<&Self>;
}

pub trait NonZeroTest: Sized {
    fn test_non_zero(&self) -> NumResult<&Self>;
}

macro_rules! with_num_err_vars(
    ($macro_id:ident $(; $($args:tt)+)?) => {
        // $macro_id!(variants:[Negative, NonFinite, IsZero, Conversion, Other] $(; $($args)+)? );
        $macro_id!(variants:[Negative, NonFinite, Conversion, Other] $(; $($args)+)? );
    }
);

macro_rules! num_err_enum {
    (variants:[$($variant:ident),*]) => {
        #[derive(Clone, Debug)]
        pub enum NumErr {
            $($variant(String)),*
        }

        impl Error for NumErr {}

        impl NumErr {
            pub fn info(&self) -> &str {
                match self {
                    $(NumErr::$variant(x) => x,)*
                }
            }

            // pub fn map_info<F: FnOnce(&str) -> String>(&self, f: F) -> NumErr {
            //     match self {
            //         $(NumErr::$variant(x) => NumErr::$variant(f(x)),)*
            //     }

            // }

            paste! {
                $(
                    #[allow(unused)]
                    pub fn [< $variant:snake:lower >]<T: CustomDisplay>(n: T) -> Self { Self::$variant(format!("{}", n.custom_display())) }
                )*
            }
        }
    };
}

#[allow(unused)]
macro_rules! num_err_new_methods {
    (variants:[$($variant:ident),*]) => {
        paste! {
            $(
                pub fn [< new_ $variant >](info: T) -> Self {
                    Self::$variant(info)
                }
            )*
        }
    };
}

with_num_err_vars!(num_err_enum);

macro_rules! num_err_display {
    (variants:[$($variant:ident),*]) => {
        impl Display for NumErr {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> FmtRes {
                write!(
                    f,
                    "{}({:?})",
                    match self {
                        $(NumErr::$variant(_) => stringify!($variant),)*
                    },
                    self.info()
                )
            }
        }
    };
}
with_num_err_vars!(num_err_display);
