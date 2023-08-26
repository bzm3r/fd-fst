// Use nightly Rust, as we want declarative macro related debugging
// functionality
// #![feature(trace_macros)]
// #![feature(log_syntax)]

// #[cfg(debug_assertions)]
// trace_macros!(true);

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
    // For automatically generating code (an important feature, especially) in
    // the absence of other intrinsic generic programming tools, we have two
    // options in Rust: macro_rules! and proc_macro.

    // This is about macro_rules!.
    // (introducing matchers and transcribers).
    // They have the following basic structure:
    // macro_rules! macro_name {
    //     (<matcher>) => {
    //         <transcriber>
    //     };
    // }

    // Recall that in rust, macro_rules! are "expanded" into the final output:
    // metavariables in the macro's transcriber body are placeholders that
    // are replaced with tokens provided by the user.

    // Suppose we have a macro that generates "hello world!"
    macro_rules! hello_world {
        () => {
            "hello world!"
        };
    }

    // println! is a macro provided by the std lib, and the following works:
    println!("{}", hello_world!());

    // From the above, one may think that Rust's macros are expanded
    // "inside-out": that is, those which are nested deeper are expanded first,
    // and then can be used by macros at upper levels. This would also fit our
    // mental model of how functions work in Rust.

    // Let's test this out further using a macro of our own that takes something
    // to be printed as an argument, and then calls println! on it.
    #[allow(unused)]
    macro_rules! printer {
        ($x:literal) => {
            println!("printer : {}", $x)
        };
    }

    // printer!(hello_world!()); // This does not work.

    // Uncomment the above code to see the error yourself if you're curious:
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

    // In this case macro engine passes the tokens
    // `hello_world!()`
    // as the input to `printer`, rather than expanding `hello_world!()`
    // first and then passing the resulting tokens to it, contradicting our hypothesis.

    // Yet, `println!` *can* work "inside-out", as we saw earlier. What allows
    // it to do so? Looking at the source for `println!`
    // (https://doc.rust-lang.org/src/std/macros.rs.html#132-139), one
    // possibility that stands out is that iis that it takes as argument
    // `$($args:tt)*`, a sequence of arbitrary `TokenTrees`
    // (https://doc.rust-lang.org/reference/macros.html#macro-invocation). So,
    // we try:
    macro_rules! printer2a {
        ($($x:tt)*) => {
            println!("printer2a: {}", $($x)*)
        };
    }
    printer2a!(hello_world!()); // This works!

    // Next, we verify whether it is $($x:tt)* alone which can match a macro
    // call. The first case is a sanity check, we do not expect x:tt to match
    // due to reason given above its matcher. The second case however, we do
    // expect to work because a macro invocation is an expr
    // (https://doc.rust-lang.org/reference/macros.html#macro-invocation).
    macro_rules! printer2b {
        // We do not expect this to work because because a
        // macro call is built out of primitive token trees, and is not a primitive
        // token tree itself.
        ($x:tt) => {
            println!("printer2b [$x:tt]: {}", $x)
        };
        // This should work.
        ($x:expr) => {
            println!("printer2b [$x:expr]: {}", $x)
        };
        ($($x:tt)*) => {
            println!("printer2b [$($x:tt)*]: {}", $($x)*)
        };
    }
    printer2b!(hello_world!());

    // A block is just a braced expression.
    macro_rules! printer2c {
        ($x:block) => {
            println!("printer2c [$x:block]: {}", $x);
        };
    }
    printer2c!({ hello_world!() });

    // We make then the following hypothesis: if the input passed to a macro_rules!
    // contains macro invocations, then this invocation will be "eagerly" expanded for
    // use in the transcriber only if it is matched against an argument that has
    // the appropriate fragment identifier for a macro invocation. There
    // are two fragment identifiers that can be used to match a macro invocation:
    // 0) as a $($x:tt)*
    // 1) as a $x:expr
    // 2) as a $x:block
    //
    // All are useful in their own way (although I am not sure about what blocks
    // add compared to `{ $body:expr }`, which are more flexible in where they
    // can be used). For example: sometimes we do not have prior information
    // regarding the specific shape of arguments to expect, and $($x:tt)* is the
    // perfect tool for this case. On the other hand, at other times, we know
    // exactly what arguments a macro invocation will accept, so we can
    // surgically match possible macro invocations as a $x:expr.

    macro_rules! printer3a {
        ($fmt:literal, $x:expr, $y:expr) => {
            println!($fmt, $x, $y);
        };
    }

    macro_rules! printer3b {
        ($fmt:literal, $($x:tt)*) => {
            println!($fmt, $($x)*);
        }
    }

    macro_rules! hello {
        () => {
            "hello"
        };
    }

    macro_rules! world {
        () => {
            "world"
        };
    }

    printer3a!("printer3a ($x:expr): {}? {}?", hello!(), world!()); // This works!
    printer3b!("printer3b ($($x:tt)*): {}? {}?", hello!(), world!()); // This works!

    // The macro_rules! parser is greedily consumes tokens when building a token
    // tree sequence of the form $($x:tt)*. To contain this behaviour, we can
    // put sequence matcher inside valid Rust delimiters (i.e. any of {}, (), or
    // []). When using token tree sequences, is it sufficient that only the
    // portion of arguments that contains the macro invocation is passed marked
    // as a token tree sequence fragment?
    macro_rules! printer3c {
        ($fmt:literal, ($($x:tt)*), $small:literal, ($($y:tt)*)) => {
            println!($fmt, $($x)*, $small, $($y)*);
        }
    }
    printer3c!(
        "printer3c (interleaving $($x:tt)* with other fragment types): {} {} {}!",
        (hello!()),
        "small",
        (world!())
    );

    // From this point onward, we'll only use the $x:expr form of matcher pattern to
    // match against an expression.

    // Suppose we have the following situation. We would like to automatically
    // generate (using macro_rules!) an enum, and various follow-on impls that will require us to
    // match on each variant of the enum, perhaps because the situation is such that
    // we expect the variants of the enum to change frequently. So, we make
    // a macro that acts as the single location where we record what variants
    // there are in the macro. Then, we might think we can use this macro inside
    // another one, however that would not work.

    #[allow(unused)]
    macro_rules! generate_variants {
        () => {
            Alpha, Beta, Gamma
        }
    }

    #[allow(unused)]
    macro_rules! generate_enum_naive {
        ($variants:expr) => {
            enum AutoGenerated {
                $variants,
            }
        };
    }
    // generate_enum_naive!(generate_variants!()); // this does not work

    // This not a limitation of the expr fragment type, but rather a more
    // general rule (see: https://stackoverflow.com/a/62510276/3486684)

    // It does not work because a macro invocation cannot be placed in the body
    // of an enum.

    // Something like this would not work either, because it reduces to
    // problematic cases studied earlier.

    // would not work
    // macro_rules! generate_enum {
    //     (@internal $($variants:ident),*) => {
    //         enum AutoGenerated {
    //             $($variants:ident),*
    //         }
    //     };
    //     ($variants:expr) => {
    //         generate_enum!(@internal $variants)
    //     };
    // }

    // At the very least, we know we have to somehow use:
    #[allow(unused)]
    macro_rules! generate_enum_core {
        ($($variants:ident),*) => {
            enum AutoGenerated {
                $($variants),*
            }
        }
    }

    // The simple fact is that there is no way to pass the results of
    // ORIGINAL
    // Baby steps. One macro *can* create another:
    macro_rules! macro_generator {
        // matcher and transcriber must both take arbitrary token trees as
        // arguments; expr or block are not valid. This is because we have to
        // consume $ tokens, which can only be accepted by arbitrary token
        // trees.
        ( $macro_name:ident ; $(($($matcher:tt)*) => { $($body:tt)* });+ ) => {
            macro_rules! $macro_name {
                $(($($matcher)*) => { $($body)* });+
            }
        }
    }

    macro_rules! meta_gen_test {
        () => {
            macro_generator!(
                test_gen ;
                ($x:literal) => {
                    println!("meta_gen_test: {}", $x);
                }
            );
            test_gen!("meta_gen_test");
        }
    }

    meta_gen_test!();

    macro_rules! meta_gen_test2 {
        ($d:tt, $bang:tt) => {
            macro_generator $bang (
                test_gen2 ;
                ($d x:literal) => {
                    println $bang ("meta_gen_test2: {}", $d x);
                }
            );
            test_gen2!("meta_gen_test2");
        }
    }
    meta_gen_test2!($, !);
    // macro_rules! generate_enum_with_variants {
    //     (@second_pass $($tokens:tt)*) => {
    //         macro_rules! {
    //             ($($tokens:tt)*) => {

    //             }
    //         }
    //     };
    //     ($variants:expr) => {
    //         // generate_enum_core(variants!()); // we know would not work
    //         macro_rules! delay {
    //             ($($variants:ident),*) => {

    //             }
    //         }
    //         delay!()
    //     }
    // }

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
