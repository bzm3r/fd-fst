// //#[macro_export]
macro_rules! standardize_auto_params {
    (
        [$macro_id:ident]
        [
            (
                $source:ident$(($($source_params:tt)+))?,
                $target:ident$(($($target_params:tt)+))?
            )
            $((impl_params: $($impl_params:tt)+))?
            $(( where $($where_args:tt)+))?
        ]
        $($rest:tt)*
    ) => {
        $macro_id!(
            @standardized
            [
                (
                    $source$(($($source_params)+))?,
                    $target$(($($target_params)+))?
                )
                (impl_params: $($($impl_params)+,)? $($($source_params)+,)? $($($target_params)+)?)
                $(( where $($where_args)+))?
            ]
            $($rest)*
        );
    };
    (
        [$macro_id:ident]
        [
            $self:ident$(($($self_params:tt)+))?
            $((impl_params: $($impl_params:tt)+))?
            $(( where $($where_args:tt)+))?
        ]
        $($rest:tt)*
    ) => {
        $macro_id!(
            @standardized
            [
                (
                    Self,
                    $self$(($($self_params)+))?
                )
                (impl_params: $($($impl_params)+,)? $($($self_params)+)?)
                $(( where $($where_args)+))?
            ]
            $($rest)*
        );
    };
}

//#[macro_export]
macro_rules! auto_from_num {
    (
        @standardized
        [
            (
                $source:ident$(($($source_params:tt)+))?,
                $target:ident$(($($target_params:tt)+))?
            )
            (impl_params: $($($impl_params:tt)+)?)
            $(( where $($where_args:tt)+))?
        ]
        trivial
    ) => {
        impl$(<$($impl_params)+>)?
            FromNum<$source$(<$($source_params)+>)?>
                for
            $target$(<$($target_params)+>)?
        $(where $($where_args)+)? {
            #[inline]
            fn from_num(value: $source$(<$($source_params)+>)?) -> Self {
                value
            }
        }
    };
    (
        @standardized
        [
            (
                $source:ident$(($($source_params:tt)+))?,
                $target:ident$(($($target_params:tt)+))?
            )
            (impl_params: $($($impl_params:tt)+)?)
            $(( where $($where_args:tt)+))?
        ]
        primitive
    ) => {
        impl$(<$($impl_params)+>)?
            FromNum<$source$(<$($source_params)+>)?>
                for
            $target$(<$($target_params)+>)?
        $(where $($where_args)+)? {
            #[inline]
            fn from_num(value: $source$(<$($source_params)+>)?) -> Self {
                value as Self
            }
        }
    };
    (
        @standardized
        [
            (
                $source:ident$(($($source_params:tt)+))?,
                $target:ident$(($($target_params:tt)+))?
            )
            (impl_params: $($($impl_params:tt)+)?)
            $(( where $($where_args:tt)+))?
        ]
        |$inp:ident| { $body:expr }
    ) => {
        impl$(<$($impl_params)+>)?
            FromNum<$source$(<$($source_params)+>)?>
                for
            $target$(<$($target_params)+>)?
        $(where $($where_args)+)? {
            #[inline]
            fn from_num(value: $source$(<$($source_params)+>)?) -> Self {
                (|$inp: $source| -> Self { $body })(value)
            }
        }
    };
    (
        [
            $($type_info:tt)*
        ]
        $($rest:tt)*
    ) => {
        standardize_auto_params!(
            [auto_from_num]
            [
                $($type_info)*
            ]
            $($rest)*
        );
    };
}

//#[macro_export]
macro_rules! auto_try_from_num {
    (
        @standardized
        [
            (
                $source:ident$(($($source_params:tt)+))?,
                $target:ident$(($($target_params:tt)+))?
            )
            (impl_params: $($($impl_params:tt)+)?)
            $(( where $($where_args:tt)+))?
        ]
        (|$inp:ident| { $body:expr })
    ) => {
        impl$(<$($impl_params)+>)?
            crate::num_conv::TryFromNum<$source$(<$($source_params)+>)?>
                for
            $target$(<$($target_params)+>)?
        $(where $($where_args)+)? {
            #[inline]
            fn try_from_num(source: $source$(<$($source_params)+>)?) -> NumResult<Self> {
                (
                    |   $inp: $source$(<$($source_params)+>)?  | -> NumResult<Self> {
                        $body
                    }
                )(source)
            }
        }
    };
    (
        @standardized
        [
            (
                $source:ident$(($($source_params:tt)+))?,
                $target:ident$(($($target_params:tt)+))?
            )
            (impl_params: $($($impl_params:tt)+)?)
            $(( where $($where_args:tt)+))?
        ]
    ) => {
        impl$(<$($impl_params)+>)?
            crate::num_conv::TryFromNum<$source$(<$($source_params)+>)?>
                for
            $target$(<$($target_params)+>)?
        $(where $($where_args)+)? {
            #[inline]
            fn try_from_num(source: $source$(<$($source_params)+>)?) -> NumResult<Self> {
                <$source as crate::num::Testable>::test_all(&source)
                    .map(|&s| <Self as FromNum<$source$(<$($source_params)+>)?>>::from_num(s))
            }
        }
    };
    (
        [
            (
                $source:ident$(($($source_params:tt)+))?,
                $target:ident$(($($target_params:tt)+))?
            )
            $((impl_params: $($impl_params:tt)+))?
            $(( where $($where_args:tt)+))?
        ]
        $($rest:tt)*
    ) => {
        standardize_auto_params!(
            [auto_try_from_num]
            [
                (
                    $source$(($($source_params)+))?,
                    $target$(($($target_params)+))?
                )
                $((impl_params: $($impl_params)+))?
                $(( where $($where_args)+))?
            ]
            $($rest)*
        );
    };
}

//#[macro_export]
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

//#[macro_export]
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
        multi_auto_test!(
            @standardized
            [
                $($ty_info)*
            ]
            $(($test:[$($args)*]))*
        );
        auto_try_from_num!(
            @standardized
            [
                $($ty_info)*
            ]
        );
    };
    (
        $($rest:tt)*
    ) => {
        standardize_auto_params!(
            [ auto_basic_num ]
            $($rest)*
        );
    };
}

//#[macro_export]
macro_rules! auto_test {
    (
        @standardized
        [
            (
                $ignore:ident$(($($ignore_params:tt)+))?,
                $self:ident$(($($params:tt)+))?
            )
            (impl_params: $($($impl_params:tt)+)?)
            $(( where $($where_args:tt)+))?
        ]
        (NonNeg : [signed_num])
    ) => {
        auto_test!(
            @standardized
            [
                (
                    $ignore$(($($ignore_params)+))?,
                    $self$(($($params)+))?
                )
                (impl_params: $($($impl_params)+)?)
                $(( where $($where_args)+))?
            ]
            (NonNeg : [
                |inp| {
                    (!<$self as crate::signed_num::SignedNum>::negative(*inp))
                        .then_some(inp)
                        .ok_or(NumErr::negative(inp))
                }
            ])
        );
    };
    (
        @standardized
        [
            (
                $ignore:ident$(($($ignore_params:tt)+))?,
                $self:ident$(($($params:tt)+))?
            )
            (impl_params: $($($impl_params:tt)+)?)
            $(( where $($where_args:tt)+))?
        ]
        ($test:ident : [trivial])
    ) => {
        auto_test!(
            @standardized
            [
                (
                    $ignore$(($($ignore_params)+))?,
                    $self$(($($params)+))?
                )
                (impl_params: $($($impl_params)+)?)
                $(( where $($where_args)+))?
            ]
            ($test : [|inp| { Ok(inp) }])
        );
    };
    (
        @standardized
        [
            (
                $ignore:ident$(($($ignore_params:tt)+))?,
                $self:ident$(($($params:tt)+))?
            )
            (impl_params: $($($impl_params:tt)+)?)
            $(( where $($where_args:tt)+))?
        ]
        ($test:ident : [|$inp:ident| { $body:expr }])
    ) => {
        paste::paste! {
            impl$(<$($impl_params)+>)?
                crate::num_check::[<  $test Test >]
                    for
                $self$(<$($params)+>)?
            $(where $($where_args)+)?
            {
                #[inline]
                fn [< test_ $test:snake:lower >]<'a>(&'a self) -> NumResult<&'a Self> {
                    (|$inp: &'a $self| -> NumResult<&'a Self> { $body })(self)
                }
            }
        }
    };
    (
        [
            $($type_info:tt)*
        ]
        $($rest:tt)*
    ) => {
        standardize_auto_params!(
            [auto_test]
            [ $($type_info)* ]
            $($rest)*
        );
    };
}

//#[macro_export]
macro_rules! multi_auto_test {
    (
        [
            $($ty_info:tt)*
        ]
        $($args:tt)*
    ) => {
        standardize_auto_params!(
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
        ($test:ident : [$($closure:tt)*])
        $($rest:tt)*
    ) => {
        auto_test!(
            @standardized
            [
                $($ty_info)*
            ]
            ($test:[$($closure)*])
        );
        multi_auto_test!(@standardized [$($ty_info)*] $($rest)*);
    };
}

//#[macro_export]
macro_rules! auto_abs_num {
    (
        @standardized
        [
            (
                $abs:ident$(($($abs_params:tt)+))?,
                $adp:ident$(($($adp_params:tt)+))?
            )
            (impl_params: $($($impl_params:tt)+)?)
            $(( where $($where_args:tt)+))?
        ]
    ) => {
        impl$(<$($impl_params)+>)?
            AbsoluteNum<$adp$(<$($adp_params)+>)?>
                for
            $abs$(<$($abs_params),+>)?
            $(( where $($where_args:tt)+))?
        {}
    };
    (
        @standardized
        [
            (
                $abs:ident$(($($abs_params:tt)+))?,
                $adp:ident$(($($adp_params:tt)+))?
            )
            (impl_params: $($($impl_params:tt)+)?)
            $(( where $($where_args:tt)+))?
        ]
        (div_usize:[$($args:tt)*]) $($rest:tt)*
    ) => {
        auto_div_usize!(
            [
                $abs$($($abs_params),*)?
                $(( where $($where_args)+))?
            ]
            $($args)*
        );
        auto_abs_num!(
            @standardized
            [
                (
                    $abs$(($($abs_params)+))?,
                    $adp$(($($adp_params)+))?
                )
                (impl_params: $($($impl_params)+)?)
                $(( where $($where_args)+))?
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
                $abs:ident$(($($abs_params:tt)+))?,
                $adp:ident$(($($adp_params:tt)+))?
            )
            (impl_params: $($($impl_params:tt)+)?)
            $(( where $($where_args:tt)+))?
        ]
        (from_adp:[$($args:tt)*]$(, try_from_override:[$($closure:tt)+])?)
        $($rest:tt)*
    ) => {
        auto_from_num!(
            @standardized
            [
                (
                    $adp$(($($adp_params)+))?,
                    $abs$(($($abs_params)+))?
                )
                (impl_params: $($($impl_params)+)?)
                $(( where $($where_args)+))?
            ]
            $($args)*
        );
        auto_try_from_num!(
            @standardized
            [
                (
                    $adp$(($($adp_params)+))?,
                    $abs$(($($abs_params)+))?
                )
                (impl_params: $($($impl_params)+)?)
                $(( where $($where_args)+))?
            ]
            $($($closure)+)?
        );
        auto_abs_num!(
            @standardized
            [
                (
                    $abs$(($($abs_params)+))?,
                    $adp$(($($adp_params)+))?
                )
                (impl_params: $($($impl_params)+)?)
                $(( where $($where_args)+))?
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
        standardize_auto_params!(
            [auto_abs_num]
            [$($type_info)*] $($args)+
        );
    };
}

//#[macro_export]
macro_rules! auto_div_usize {
    (
        [
            $this:ident$(($($params:tt)+))?
            $(( where $($where_args:tt)+))?
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
            $this:ident$(($($params:tt)+))?
            $(( where $($where_args:tt)+))?
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
                            <$this$(<$($params)+>)?>::from_num((lhs/rhs).round())
                        }
                    ]
                )
            }
        }
    };
}

//#[macro_export]
macro_rules! map_enum_inner {
    ($enum:ident, $enum_var:ident, (|$match_var:ident : $match_ty:ty| -> $out_ty:ty { $body:expr }) ($($variant:ident),*)) => {
        match $enum_var {
            $($enum::$variant(x) => {
                (|$match_var: $match_ty| -> $out_ty { $body })(x)macro_rules! nested_try_from {
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
            }),*
        }
    }
}
