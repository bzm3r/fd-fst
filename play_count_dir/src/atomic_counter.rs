use std::{
    ops::{Add, Sub},
    sync::atomic::Ordering,
};

use crate::num::{Num, UnsignedNum};
use paste::paste;

pub trait HasAtomicForm
where
    Self: UnsignedNum,
{
    type AtomicForm: AtomicForm;
    const ONE: Self;
    const ZERO: Self;

    fn into_atomic(&self) -> Self::AtomicForm;
}

pub trait AtomicForm
where
    Self: Sized,
{
    type N: HasAtomicForm;
}

macro_rules! has_atomic_form {
    ($($t:ty),*) => {
        paste! {
            $(
                impl HasAtomicForm for $t {
                    const ZERO: Self = 0;
                    const ONE: Self = 1;
                    type AtomicForm = std::sync::atomic::[< Atomic $t:camel >];
                    fn into_atomic(&self) -> Self::AtomicForm {
                        Self::AtomicForm::from(*self)
                    }
                }
            )*
        }
    };
}

has_atomic_form!(u8, u32, usize);

macro_rules! impl_atomic_counter {
    ($($t:ty),*) => {
        paste! {
            $(
                impl AtomicForm for std::sync::atomic::[< Atomic $t:camel >] {
                    type N = $t;
                }
            )*
        }
    };
}

impl_atomic_counter!(u8, u32, usize);
