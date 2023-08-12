// Use nightly Rust, as we want declarative macro related debugging
// functionality
// #![feature(trace_macros)]
// #![feature(log_syntax)]

// All the problems explored in this playground are handily answered by using
// procedural macros, instead of declarative macros. But sometimes, it's much
// faster to write a declarative macro, and they might also compile
// faster. So, how much can we do with declarative macros as they exist? Does
// the exercise provide hints as to how the declarative macro engine could be
// improved?

// Turn this on if you want to see the output of various macros, even if they
// have errors. Very useful for debugging.
// trace_macros!(false);

// The section in Rust Reference on declarative macros
// (https://doc.rust-lang.org/reference/macros-by-example.html) is important,
// and worth referring to often when you need further detail on a particular
// definition. In general, I have tried to use words that should be search-able
// within this document.

fn main() {
    // Suppose we have a macro that generates "hello world!"
    macro_rules! hello_world {
        () => {
            "hello world!"
        };
    }

    // The following statement works then:
    println!("{}", hello_world!());
    // This seems to suggest that hello_world! is expanded first, and its
    // expansion is provided for use in `println!`'s expansion. This seems to
    // suggest that macro expansion in Rust happens "inside-out": first the
    // inner macro is expanded, and then its expansion is provided for use in
    // the outer macro's expansion.

    // Let's test this theory.

    // Suppose we have our own little println-like macro: it takes some inputs
    // as argument (in this case, only what is to be printed), as we provide a
    // simplified format string), and then calls println on it.
    #[allow(unused)] // Clippy, it's fine.
    macro_rules! printer {
        ($x:literal) => {
            println!("{}", $x)
        };
    }
    // The following *does not* work then:
    // printer!(hello_world!());
    // We get the error:
    //     error: no rules expected the token `hello_world`
    //   --> src\main.rs:30:17
    //    |
    // 23 |     macro_rules! printer {
    //    |     ----------------------- when calling this macro
    // ...
    // 30 |     printer!(hello_world!());
    //    |                 ^^^^^^^^^^^ no rules expected this token in macro call
    //    |
    // note: while trying to match meta-variable `$x:literal`
    //   --> src\main.rs:24:10
    //    |
    // 24 |         ($x:literal) => {
    //    |          ^^^^^^^^^^
    // From the error, it seems that the macro engine passes `hello_world!()`
    // as the input to `printer`, rather than expanding `hello_world!()`
    // first and then passing the output of `hello_world!` to `printer`.
    // In other words, it expands `printer` first, and *then* `hello_world!`:
    // it goes "outside-in" in its expansion order.

    // Yet, `println!` *can* clearly go "inside-out", as we saw earlier. What's
    // behind the different behaviour? If we inspect the source code of
    // `println!` (https://doc.rust-lang.org/src/std/macros.rs.html#132-139), one possibility
    // that stands out is that it takes see that it takes as argument any sequence of `TokenTrees`
    // (https://doc.rust-lang.org/reference/macros.html#macro-invocation). So,
    // we try:
    macro_rules! printer_v2 {
        ($($x:tt)*) => {
            println!("{}", $($x)*)
        };
    }
    printer_v2!(hello_world!());
    // This works!
    // So it seems that when passing in input to a declarative macro using
    // a generic sequence of `TokenTree`s, then the input is *expanded first*
    // if it contains a macro invocation. "Inside-out"!

    // Suppose we have two macros that *do not* take as argument a generic
    // sequence of `TokenTree`s as argument, like `hello_world!` (no arguments)
    // and  `printer`. Is there *some* way we can use the inside-out property
    // of generic token trees to execute `printer` in an inside out fashion?

    // For motivation: there are a lot of macros one can imagine, where it
    // would be nice if we could use the output of an inner macro in the outer
    // macro. A common use case would be to ease the burden of code duplication
    // within a macro.

    // One guess is to create an "eager" adaptor that should expand its its input
    // first, before having it used elsewhere. (Note the similarity here between
    // `eager_adaptor`, and `printer_v2`.)
    #[allow(unused)]
    macro_rules! eager_adaptor {
        ($($t:tt)*) => {
            printer!($($t)*);
        };
    }
    // eager_adaptor!(hello_world!()); // This does not work
    // The error we get this time is:
    //     error: no rules expected the token `hello_world`
    //   --> src\main.rs:94:17
    //    |
    // 38 |     macro_rules! printer {
    //    |     -------------------- when calling this macro
    // ...
    // 94 |     eager_adaptor!(hello_world!());
    //    |                    ^^^^^^^^^^^ no rules expected this token in macro call
    //    |
    // note: while trying to match meta-variable `$x:literal`
    //   --> src\main.rs:39:10
    //    |
    // 39 |         ($x:literal) => {
    //    |          ^^^^^^^^^^
    //
    // So, it seems that the "magic" of `TokenTree`s' ability to transcend the
    // outside-in limitations of the current macro engine is *not* because
    // the the the `TokenTree`'s arguments are eagerly evaluated.

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

    paren_dollar_to_dollar!(([macro_rules! simple_macro {
        (($)x:literal) => {
            println!("{}", ($)x);
        }
    }] []));
    simple_macro!("simple_macro works!");

    #[allow(unused)]
    macro_rules! create_helper {
        ( ($($rule_matcher:tt)*) ) => {
            macro_generator!(
                helper ;
                ($($rule_matcher)*) => {
                    println!("input {} MATCHES rule!", stringify!(Option<($) inner_ty>))
                };
                (($) (($) t:tt)*) => {
                    println!("input {} does not match rule!", stringify!(($) (($) t)*));
                }
            );
        };
    }
}

// macro_generator!(
//     auto_gen ;
//     () => { println!("hello world!"); } ;
//     ($($t:tt)*) => { println!("found some input!") }
// );

// create_helper!( (Option<$inner_ty:ty>) );

// create_replacer_2!(test_replacer ; [(a) -> (0)] [(b) -> (1)] );
