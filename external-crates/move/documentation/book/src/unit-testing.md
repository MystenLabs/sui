# Unit Tests

Unit testing for Move uses three annotations in the Move source language:

- `#[test]`
- `#[test_only]`, and
- `#[expected_failure]`.

They respectively mark a function as a test, mark a module or module member (`use`, function, or
struct) as code to be included for testing only, and mark that a test is expected to fail. These
annotations can be placed on a function with any visibility. Whenever a module or module member is
annotated as `#[test_only]` or `#[test]`, it will not be included in the compiled bytecode unless it
is compiled for testing.

## Test Annotations

The `#[test]` annotation can only be placed on a function with no parameters.
This annotation marks the function as a test to be run by the unit testing harness.

```move
#[test] // OK
fun this_is_a_test() { ... }

#[test] // Will fail to compile since the test takes an argument
fun this_is_not_correct(arg: u64) { ... }
```

A test can also be annotated as an `#[expected_failure]`. This annotation marks
that the test is expected to raise an error. There are a number of options that
can be used with the `#[expected_failure]` annotation to ensure only a failure
with the specified condition is marked as passing, these options are detailed
in [Expected Failures](#expected-failures). Only functions that have the
`#[test]` annotation can also be annotated as an #`[expected_failure]`. 

Some simple examples of using the `#[expected_failure]` annotation are shown below:

```move
#[test]
#[expected_failure]
public fun this_test_will_abort_and_pass() { abort 1 }

#[test]
#[expected_failure]
public fun test_will_error_and_pass() { 1/0; }

#[test] // Will pass since test fails with the expected abort code constant.
#[expected_failure(abort_code = ENotFound)] // ENotFound is a constant defined in the module
public fun test_will_error_and_pass_abort_code() { abort ENotFound }

#[test] // Will fail since test fails with a different error than expected.
#[expected_failure(abort_code = my_module::EnotFound)]
public fun test_will_error_and_fail() { 1/0; }

#[test, expected_failure] // Can have multiple in one attribute. This test will pass.
public fun this_other_test_will_abort_and_pass() { abort 1 }
```

## Expected Failures

There are a number of different ways that you can use the `#[expected_failure]`
annotation to specify different types of error conditions. These are:

### 1. `#[expected_failure(abort_code = <constant>)]`

This will pass if the test aborts with the specified constant value in the
module that defines the constant and fail otherwise. This is the recommended
way of testing for expected test failures.

**NOTE**: You can reference constants outside of the current module or package
in `expected_failure` annotations.

```move
module pkg_addr::other_module {
    const ENotFound: u64 = 1;
    fun will_abort() {
        abort ENotFound
    }
}

module pkg_addr::my_module {
    use pkg_addr::other_module;
    const ENotFound: u64 = 1;

    #[test]
    #[expected_failure(abort_code = ENotFound)]
    fun test_will_abort_and_pass() { abort ENotFound }

    #[test]
    #[expected_failure(abort_code = other_module::ENotFound)]
    fun test_will_abort_and_pass() { other_module::will_abort() }

    // FAIL: Will not pass since we are expecting the constant from the wrong module.
    #[test]
    #[expected_failure(abort_code = ENotFound)]
    fun test_will_abort_and_pass() { other_module::will_abort() }
}
```

### 2. `#[expected_failure(arithmetic_error, location = <location>)]`

This specifies that the test is expected to fail with an arithmetic error
(e.g., integer overflow, division by zero, etc) at the specified location. The
`<location>` must be a valid path to a module location, e.g., `Self`, or
`my_package::my_module`.

```move
module pkg_addr::other_module {
    fun will_arith_error() { 1/0; }
}

module pkg_addr::my_module {
    use pkg_addr::other_module;
    #[test]
    #[expected_failure(arithmetic_error, location = Self)]
    fun test_will_arith_error_and_pass1() { 1/0; }

    #[test]
    #[expected_failure(arithmetic_error, location = pkg_addr::other_module)]
    fun test_will_arith_error_and_pass2() { other_module::will_arith_error() }

    // FAIL: Will fail since the location we expect it the fail at is different from where the test actually failed.
    #[test]
    #[expected_failure(arithmetic_error, location = Self)]
    fun test_will_arith_error_and_fail() { other_module::will_arith_error() }
}
```

### 3. `#[expected_failure(out_of_gas, location = <location>)]` 

This specifies that the test is expected to fail with an out of gas error at
the specified location. The `<location>` must be a valid path to a module
location, e.g., `Self`, or `my_package::my_module`.

```move
module pkg_addr::other_module {
    fun will_oog() { loop {} }
}

module pkg_addr::my_module {
    use pkg_addr::other_module;
    #[test]
    #[expected_failure(out_of_gas, location = Self)]
    fun test_will_oog_and_pass1() { loop {} }

    #[test]
    #[expected_failure(arithmetic_error, location = pkg_addr::other_module)]
    fun test_will_oog_and_pass2() { other_module::will_oog() }

    // FAIL: Will fail since the location we expect it the fail at is different from where the test actually failed.
    #[test]
    #[expected_failure(out_of_gas, location = Self)]
    fun test_will_oog_and_fail() { other_module::will_oog() }
}
```

### 4. `#[expected_failure(vector_error, minor_status = <u64_opt>, location = <location>)]`

This specifies that the test is expected to fail with a vector error at the
specified location and with the given `minor_status` if provided. The
`<location>` must be a valid path to a module location, e.g., `Self`, or
`my_package::my_module`. The `<u64_opt>` is an optional parameter that
specifies the minor status of the vector error. If it is not specified, the
test will pass if the test fails with any minor status. If it is specified, the
test will only pass if the test fails with a vector error with the specified
minor status.

```move
module pkg_addr::other_module {
    fun vector_borrow_empty() {
        vector::borrow(&vector::empty<u64>(), 1);
    }
}

module pkg_addr::my_module {
    #[test]
    #[expected_failure(vector_error, location = Self)]
    fun vector_abort_same_module() {
        vector::borrow(&vector::empty<u64>(), 1);
    }

    #[test]
    #[expected_failure(vector_error, location = pkg_addr::other_module)]
    fun vector_abort_same_module() {
        other_module::vector_borrow_empty();
    }

    // Can specify minor statues (i.e., vector-specific error codes) to expect.
    #[test]
    #[expected_failure(vector_error, minor_status = 1, location = Self)]
    fun native_abort_good_right_code() {
        vector::borrow(&vector::empty<u64>(), 1);
    }

    // FAIL: correct error, but wrong location.
    #[test]
    #[expected_failure(vector_error, location = pkg_addr::other_module)]
    fun vector_abort_same_module() {
        other_module::vector_borrow_empty();
    }

    // FAIL: correct error and location but the minor status differs so this test will fail.
    #[test]
    #[expected_failure(vector_error, minor_status = 0, location = Self)]
    fun vector_abort_wrong_minor_code() {
        vector::borrow(&vector::empty<u64>(), 1);
    }
}
```

### 5. `#[expected_failure]` 

This will pass if the test aborts with any error code. Because of this you
should be incredibly careful using this way of annotating expected tests
failures, and instead prefer one of the ways described above instead. Examples
of these types of annotations are:

```move
#[test]
#[expected_failure]
fun test_will_abort_and_pass1() { abort 1 }

#[test]
#[expected_failure]
fun test_will_arith_error_and_pass2() { 1/0; }
```


## Test Only Annotations

A module and any of its members can be declared as test only. If an item is
annotated as `#[test_only]` the item will only be included in the compiled Move
bytecode when compiled in test mode. Additionally, when compiled outside of
test mode, any non-test `use`s of a `#[test_only]` module will raise an error
during compilation.

**NOTE**: functions that are annotated with `#[test_only]` will only be available
to be called from test code, but they themselves are not tests and will not be
run as tests by the unit testing framework.

```move
#[test_only] // test only attributes can be attached to modules
module abc { ... }

#[test_only] // test only attributes can be attached to constants
const Addr: address = @0x1;

#[test_only] // .. to uses
use pkg_addr::some_other_module;

#[test_only] // .. to structs
public struct SomeStruct { ... }

#[test_only] // .. and functions. Can only be called from test code, but this is _not_ a test!
fun test_only_function(...) { ... }
```

## Running Unit Tests

Unit tests for a Move package can be run with the [`sui move test` command](./packages.md).

When running tests, every test will either `PASS`, `FAIL`, or `TIMEOUT`. If a test case fails, the
location of the failure along with the function name that caused the failure will be reported if
possible. You can see an example of this below.

A test will be marked as timing out if it exceeds the maximum number of
instructions that can be executed for any single test. This bound can be
changed using the options below. Additionally, while the result of a test is
always deterministic, tests are run in parallel by default, so the ordering of
test results in a test run is non-deterministic unless running with only one
thread (see `OPTIONS` below on how to do this).

There are also a number of options that can be passed to the unit testing binary to fine-tune
testing and to help debug failing tests. The available options, and a
description of what each one can do can be found by passing the help flag to
the `sui move test` command:

```
$ sui move test --help
```

## Example

A simple module using some of the unit testing features is shown in the following example:

First create an empty package and change directory into it:

```bash
$ sui move new test_example; cd test_example 
```

Next add the following module under the `sources` directory:

```move
// filename: sources/my_module.move
module test_example::my_module {

    public struct Wrapper(u64)

    const ECoinIsZero: u64 = 0;

    public fun make_sure_non_zero_coin(coin: Wrapper): Wrapper {
        assert!(coin.0 > 0, ECoinIsZero);
        coin
    }

    #[test]
    fun make_sure_non_zero_coin_passes() {
        let coin = Wrapper(1);
        let Wrapper(_) = make_sure_non_zero_coin(coin);
    }

    #[test]
    // Or #[expected_failure] if we don't care about the abort code
    #[expected_failure(abort_code = ECoinIsZero)]
    fun make_sure_zero_coin_fails() {
        let coin = Wrapper(0);
        let Wrapper(_) = make_sure_non_zero_coin(coin);
    }

    #[test_only] // test only helper function
    fun make_coin_zero(coin: &mut Wrapper) {
        coin.0 = 0;
    }

    #[test]
    #[expected_failure(abort_code = ECoinIsZero)]
    fun make_sure_zero_coin_fails2() {
        let mut coin = Wrapper(10);
        coin.make_coin_zero();
        let Wrapper(_) = make_sure_non_zero_coin(coin);
    }
}
```

### Running Tests

You can then run these tests with the `move test` command:

```
$ sui move test
INCLUDING DEPENDENCY Sui
INCLUDING DEPENDENCY MoveStdlib
BUILDING test_example
Running Move unit tests
[ PASS    ] 0x0::my_module::make_sure_non_zero_coin_passes
[ PASS    ] 0x0::my_module::make_sure_zero_coin_fails
[ PASS    ] 0x0::my_module::make_sure_zero_coin_fails2
Test result: OK. Total tests: 3; passed: 3; failed: 0
```

### Using Test Flags

#### Passing specific tests to run

You can run a specific test, or a set of tests with `sui move test <str>`. This
will only run tests whose fully qualified name contains `<str>`. For example if
we wanted to only run tests with `"non_zero"` in their name:

```
$ sui move test non_zero
INCLUDING DEPENDENCY Sui
INCLUDING DEPENDENCY MoveStdlib
BUILDING test_example
Running Move unit tests
[ PASS    ] 0x0::my_module::make_sure_non_zero_coin_passes
Test result: OK. Total tests: 1; passed: 1; failed: 0
```

#### `-i <bound>` or `--gas_used <bound>`

This bounds the amount of gas that can be consumed for any one test to `<bound>`:

```
$ sui move test -i 0
INCLUDING DEPENDENCY Sui
INCLUDING DEPENDENCY MoveStdlib
BUILDING test_example
Running Move unit tests
[ TIMEOUT ] 0x0::my_module::make_sure_non_zero_coin_passes
[ FAIL    ] 0x0::my_module::make_sure_zero_coin_fails
[ FAIL    ] 0x0::my_module::make_sure_zero_coin_fails2

Test failures:

Failures in 0x0::my_module:

┌── make_sure_non_zero_coin_passes ──────
│ Test timed out
└──────────────────


┌── make_sure_zero_coin_fails ──────
│ error[E11001]: test failure
│    ┌─ ./sources/my_module.move:22:27
│    │
│ 21 │     fun make_sure_zero_coin_fails() {
│    │         ------------------------- In this function in 0x0::my_module
│ 22 │         let coin = MyCoin(0);
│    │                           ^ Test did not error as expected. Expected test to abort with code 0 <SNIP>
│
│
└──────────────────


┌── make_sure_zero_coin_fails2 ──────
│ error[E11001]: test failure
│    ┌─ ./sources/my_module.move:34:31
│    │
│ 33 │     fun make_sure_zero_coin_fails2() {
│    │         -------------------------- In this function in 0x0::my_module
│ 34 │         let mut coin = MyCoin(10);
│    │                               ^^ Test did not error as expected. Expected test to abort with code 0 <SNIP>
│
│
└──────────────────

Test result: FAILED. Total tests: 3; passed: 0; failed: 3
```

#### `-s` or `--statistics`

With these flags you can gather statistics about the tests run and report the
runtime and gas used for each test. You can additionally add `csv` (`sui move
test -s csv`) to get the gas usage in a csv output format. For example, if we
wanted to see the statistics for the tests in the example above:

```
$ sui move test -s
INCLUDING DEPENDENCY Sui
INCLUDING DEPENDENCY MoveStdlib
BUILDING test_example
Running Move unit tests
[ PASS    ] 0x0::my_module::make_sure_non_zero_coin_passes
[ PASS    ] 0x0::my_module::make_sure_zero_coin_fails
[ PASS    ] 0x0::my_module::make_sure_zero_coin_fails2

Test Statistics:

┌────────────────────────────────────────────────┬────────────┬───────────────────────────┐
│                   Test Name                    │    Time    │         Gas Used          │
├────────────────────────────────────────────────┼────────────┼───────────────────────────┤
│ 0x0::my_module::make_sure_non_zero_coin_passes │   0.001    │             1             │
├────────────────────────────────────────────────┼────────────┼───────────────────────────┤
│ 0x0::my_module::make_sure_zero_coin_fails      │   0.001    │             1             │
├────────────────────────────────────────────────┼────────────┼───────────────────────────┤
│ 0x0::my_module::make_sure_zero_coin_fails2     │   0.001    │             1             │
└────────────────────────────────────────────────┴────────────┴───────────────────────────┘

Test result: OK. Total tests: 3; passed: 3; failed: 0
```
