---
id: move-unit-test
title: Move Unit Testing Framework
custom_edit_url: https://github.com/move-language/move/edit/main/language/tools/move-unit-test/README.md
---

# Summary

This crate defines the core logic for running and reporting Move unit
tests. Move unit testing is made up of two main components; a test runner,
and a test reporter.

It's important to also note here that unit tests can be run using the
[stackless bytecode interpreter](../../move-prover/interpreter). If the
unit tests are run with the stackless bytecode interpreter and the test
returns a value, then the result of executing the unit test with the Move
VM and the result of the interpreter will be compared and an error will
be raised if they are not equal.

Detailed information on how to use unit tests as a user of Move can be
found [here](https://move-language.github.io/move/unit-testing.html).

## Test Runner

The test runner consumes a
[`TestPlan`](../../move-compiler/src/unit_test/mod.rs): this is a
datastructure that is built by the Move compiler, based on source `#[test]`
attributes. At a high level, this test plan consists of:
1. A list of `ModuleTestPlan`s for each non-dependency module. A
   `ModuleTestPlan` consists of a list of unit tests declared in a module,
   along with its arguments and whether the unit test is an expected
   failure or not.
2. Compiled modules for each source module, along with compiled modules for
   all transitive dependencies.
3. The source text and source maps for every source and transitive dependency.

The test runner takes this `TestPlan` along with various configuration
options (e.g., number of threads). From this information the test runner
creates an initial test state consisting solely of the modules in bullet
(2) above. This will be the same initial state for all unit tests.

After constructing this initial state, a work queue of `ModuleTestPlan`s is
passed to a [`rayon`](https://docs.rs/rayon/latest/rayon/) threadpool to
execute.  After the execution of each test a `PASS`, `FAIL` or `TIMEOUT` is
reported to the writer (usually `std::io::stdout`) as soon as the test's
result is known. The result of running tests in a `ModuleTestPlan` is a
mapping of failing and passing tests (where failing means that the tests
failed when it was expected to fail, or vis versa) along with profiling
information and failure information if applicable. These test statistics
for each module are combined in parallel and produce a `TestResults` data
structure.

## Test Reporter

After all of the unit tests have been run and a `TestResults` data
structure has been created, the test reporter will iterate through the test
results, and will use the data in the `TestFailure` info along with the
source maps and source text in the test plan to display source-level error
messages for any failing tests.

Depending on the options passed to the unit testing framework, additional
info, such as the global storage state at the point of error for each
failing test, or the execution time and number of instructions for each
test may be display at the end of a test run.
