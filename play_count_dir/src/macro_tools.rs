#![allow(unused)]
// Baby steps. One macro *can* create another:
macro_rules! macro_generator {
    ( $macro_name:ident ; $(($($transcriber:tt)*) => { $($body:tt)* });+ ) => {
        macro_rules! $macro_name {
            $(($($transcriber)*) => { $($body)* });+
        }
    }
}

// One can take in some input, and find and replace all occurrences of
// specified symbols. This uses the trick of skewing macro expansion
// by forcing insertion of `$` symbols, and thus delaying when a macro is
// expanded.
macro_rules! create_replacer {
    ( <($d:tt)> | ( $replacer_id:ident ; $([($($f:tt)+) -> ($($r:tt)+)])+ ) ) => {
        // We must use macro_generator here, otherwise, if we were to plainly
        // write out macro_rules! $replacer_id { ... }, we would get a weird
        // error complaining that it expected an identifier in the position
        // right after `macro_rules!`, but instead found...our identifier (the
        // expanded $replacer_id), which *should* be completely okay. It is
        // exactly our intent. Yet, the complaint. To bypass the complaint, we
        // call macro_generator with an already expanded ident, as far as
        // macro_rules is concerned.
        macro_generator! {
            $replacer_id ;
            // final output, no container delimiters
            ( ( [] [ $d ($d output:tt )* ]) ) => {
                $d ($d output )*
            };
            // final output, in braces
            ( ( brace [] [ $d ($d output:tt )* ]) ) => {
                {$d ($d output:tt )*}
            };
            // final output, in parens
            ( ( paren [] [ $d ($d output:tt )* ])  ) => {
                ($d ($d output:tt )*)
            };
            // final output, in square brackets
            ( ( square [] [ $d ($d output:tt )* ]) ) => {
                [$d ($d output )*]
            };
            // inner contents of container have finished find-and-replace: transferring results back to parents, inside associated container
            ( ( brace [] [ $d ($d output_child:tt )* ]) ($d ($d container:ident)? [ $d ($d rest:tt)* ] [ $d ($d output_parent:tt )* ]) $d ($d remaining:tt)* ) => {
                $replacer_id !( ($d ($d container)? [ $d ($d rest)* ] [ $d ($d output_parent )* {$d ($d output_child )*} ]) $d ($d remaining)* );
            };
            // inner contents of container have finished find-and-replace: transferring results back to parents, inside associated container
            ( ( paren [] [ $d ($d output_child:tt )* ]) ($d ($d container:ident)? [ $d ($d rest:tt)* ] [ $d ($d output_parent:tt )* ]) $d ($d remaining:tt)* ) => {
                $replacer_id !( ($d ($d container)? [ $d ($d rest)* ] [ $d ($d output_parent )* ($d ($d output_child )*) ]) $d ($d remaining)* );
            };
            // inner contents of container have finished find-and-replace: transferring results back to parents, inside associated container
            ( ( square [] [ $d ( $d output_child:tt )* ]) ($d ($d container:ident)? [ $d ($d rest:tt)* ] [ $d ($d output_parent:tt )* ]) $d ($d remaining:tt)* ) => {
                $replacer_id !( ($d ($d container)? [ $d ($d rest)* ] [ $d ($d output_parent )* [$d ($d output_child )*] ]) $d ($d remaining)* );
            };
            // executing a find and replace; we do this first, in case the find tokens ($f) themselves involve delimited containers
            $(
                ( ($d ($d container:ident)? [ $($f)+ $d ($d rest:tt)* ] [ $d ($d output:tt )* ] ) $d ($d remaining:tt)* ) => {
                    $replacer_id !( ($d ($d container)? [ $d ($d rest)* ] [ $d ($d output )* $($r)+ ]) $d ($d remaining)* );
                };
            )+
            // found braces, creating creating new group to be processed
            ( ($d ($d container:ident)? [ {$d ( $d inner:tt )*} $d ($d rest:tt)* ] [ $d ($d output:tt )* ] ) $d ($d remaining:tt)* ) => {
                $replacer_id !( (brace [$d ($d inner)* ] []) ($d ($d container)? [ $d ($d rest)* ] [ $d ($d output )* ]) $d ($d remaining)* );
            };
            // found parens, creating creating new group to be processed
            ( ($d ($d container:ident)? [ ($d ( $d inner:tt )*) $d ($d rest:tt)* ] [ $d ($d output:tt )* ] ) $d ($d remaining:tt)* ) => {
                $replacer_id !( (paren [$d ($d inner)* ] []) ($d ($d container)? [ $d ($d rest)* ] [ $d ($d output )* ]) $d ($d remaining)* );
            };
            // found square brackets, creating creating new group to be processed
            ( ($d ($d container:ident)? [ [$d ( $d inner:tt )*] $d ($d rest:tt)* ] [ $d ($d output:tt )* ] ) $d ($d remaining:tt)* ) => {
                $replacer_id !( (square [$d ($d inner)* ] []) ($d ($d container)? [ $d ($d rest)* ] [ $d ($d output )* ]) $d ($d remaining)* );
            };
            // nothing to replace
            ( ( $d ($d container:ident)? [ $d other:tt $d ($d rest:tt)* ] [ $d ($d output:tt )* ]) $d ($d remaining:tt)* ) => {
                $replacer_id !( ( $d ($d container)? [ $d ($d rest)* ] [ $d ($d output )* $d other ]) $d ($d remaining)*);
            };
            // Make required input for macro simpler by making it so that
            // all they have to do is provide that which initializes the
            // input bucket at first.
            ($d ($d input:tt)*) => {
                $replacer_id !( ([$d ($d input:tt)*] []) );
            }
        }
    };
    ( $($input:tt)* ) => {
        create_replacer!{ <($)> | ($($input)*) }
    }
}

// Generalizing the trick of delaying macro expansion by forcing insertion
// of dollar symbols first.
create_replacer!(paren_dollar_to_dollar ; [(($)) -> ($)]);
