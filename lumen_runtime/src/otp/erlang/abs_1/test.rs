use proptest::prop_assert;
use proptest::test_runner::{Config, TestRunner};

use liblumen_alloc::erts::term::prelude::Term;

use crate::otp::erlang::abs_1::native;
use crate::scheduler::with_process_arc;
use crate::test::strategy;

#[test]
fn without_number_errors_badarg() {
    with_process_arc(|arc_process| {
        TestRunner::new(Config::with_source_file(file!()))
            .run(
                &strategy::term::is_not_number(arc_process.clone()),
                |number| {
                    prop_assert_badarg!(
                        native(&arc_process, number),
                        format!("number ({}) must be an integer or float", number)
                    );

                    Ok(())
                },
            )
            .unwrap();
    });
}

#[test]
fn with_number_returns_non_negative() {
    with_process_arc(|arc_process| {
        TestRunner::new(Config::with_source_file(file!()))
            .run(&strategy::term::is_number(arc_process.clone()), |number| {
                let result = native(&arc_process, number);

                prop_assert!(result.is_ok());

                let abs = result.unwrap();
                let zero: Term = 0.into();

                prop_assert!(zero <= abs);

                Ok(())
            })
            .unwrap();
    });
}
