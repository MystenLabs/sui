# Unit Tests

Unit testing for Move adds three new annotations to the Move source language:

* `#[test]`
* `#[test_only]`, and
* `#[expected_failure]`.

They respectively mark a function as a test, mark a module or module member (`use`, function, or struct) as code to be included for testing only, and mark that a test is expected to fail. These annotations can be placed on a function with any visibility. Whenever a module or module member is annotated as `#[test_only]` or `#[test]`, it will not be included in the compiled bytecode unless it is compiled for testing.

## Testing Annotations: Their Meaning and Usage

Both the `#[test]` and `#[expected_failure]` annotations can be used either with or without arguments.

Without arguments, the `#[test]` annotation can only be placed on a function with no parameters. This annotation simply marks this function as a test to be run by the unit testing harness.

```
#[test] // OK
fun this_is_a_test() { ... }

#[test] // Will fail to compile since the test takes an argument
fun this_is_not_correct(arg: signer) { ... }
```

A test can also be annotated as an `#[expected_failure]`. This annotation marks that the test should is expected to raise an error. You can ensure that a test is aborting with a specific abort code by annotating it with `#[expected_failure(abort_code = <code>)]`, if it then fails with a different abort code or with a non-abort error the test will fail. Only functions that have the `#[test]` annotation can also be annotated as an #`[expected_failure]`.

```
#[test]
#[expected_failure]
public fun this_test_will_abort_and_pass() { abort 1 }

#[test]
#[expected_failure]
public fun test_will_error_and_pass() { 1/0; }

#[test]
#[expected_failure(abort_code = 0)]
public fun test_will_error_and_fail() { 1/0; }

#[test, expected_failure] // Can have multiple in one attribute. This test will pass.
public fun this_other_test_will_abort_and_pass() { abort 1 }
```

With arguments, a test annotation takes the form `#[test(<param_name_1> = <address>, ..., <param_name_n> = <address>)]`. If a function is annotated in such a manner, the function's parameters must be a permutation of the parameters <`param_name_1>, ..., <param_name_n>`, i.e., the order of these parameters as they occur in the function and their order in the test annotation do not have to be the same, but they must be able to be matched up with each other by name.

Only parameters with a type of `signer` are supported as test parameters. If a non-`signer` parameter is supplied, the test will result in an error when run.

```
#[test(arg = @0xC0FFEE)] // OK
fun this_is_correct_now(arg: signer) { ... }

#[test(wrong_arg_name = @0xC0FFEE)] // Not correct: arg name doesn't match
fun this_is_incorrect(arg: signer) { ... }

#[test(a = @0xC0FFEE, b = @0xCAFE)] // OK. We support multiple signer arguments, but you must always provide a value for that argument
fun this_works(a: signer, b: signer) { ... }

// somewhere a named address is declared
#[test_only] // test-only named addresses are supported
address TEST_NAMED_ADDR = @0x1;
...
#[test(arg = @TEST_NAMED_ADDR)] // Named addresses are supported!
fun this_is_correct_now(arg: signer) { ... }
```

An expected failure annotation can also take the form `#[expected_failure(abort_code = <u64>)]`. If a test function is annotated in such a way, the test must abort with an abort code equal to `<u64>`. Any other failure or abort code will result in a test failure.

```
#[test, expected_failure(abort_code = 1)] // This test will fail
fun this_test_should_abort_and_fail() { abort 0 }

#[test]
#[expected_failure(abort_code = 0)] // This test will pass
fun this_test_should_abort_and_pass_too() { abort 0 }
```

A module and any of its members can be declared as test only. In such a case the item will only be included in the compiled Move bytecode when compiled in test mode. Additionally, when compiled outside of test mode, any non-test `use`s of a `#[test_only]` module will raise an error during compilation.

```
#[test_only] // test only attributes can be attached to modules
module abc { ... }

#[test_only] // test only attributes can be attached to named addresses
address ADDR = @0x1;

#[test_only] // .. to uses
use 0x1::some_other_module;

#[test_only] // .. to structs
struct SomeStruct { ... }

#[test_only] // .. and functions. Can only be called from test code, but not a test
fun test_only_function(...) { ... }
```

## Running Unit Tests

Unit tests for a Move package can be run with the [`move test`
command](./packages.md).

When running tests, every test will either `PASS`, `FAIL`, or `TIMEOUT`. If a test case fails, the location of the failure along with the function name that caused the failure will be reported if possible. You can see an example of this below.

A test will be marked as timing out if it exceeds the maximum number of instructions that can be executed for any single test. This bound can be changed using the options below, and its default value is set to 5000 instructions. Additionally, while the result of a test is always deterministic, tests are run in parallel by default, so the ordering of test results in a test run is non-deterministic unless running with only one thread (see `OPTIONS` below).

There are also a number of options that can be passed to the unit testing binary to fine-tune testing and to help debug failing tests. These can be found using the the help flag:

```
$ move -h
```

## Example

A simple module using some of the unit testing features is shown in the following example:

First create an empty package and change directory into it:

```
$ move new TestExample; cd TestExample
```

Next add the following to the `Move.toml`:

```
[dependencies]
MoveStdlib = { git = "https://github.com/diem/diem.git", subdir="language/move-stdlib", rev = "56ab033cc403b489e891424a629e76f643d4fb6b", addr_subst = { "std" = "0x1" } }
```

Next add the following module under the `sources` directory:

```
// filename: sources/my_module.move
module 0x1::my_module {

    struct MyCoin has key { value: u64 }

    public fun make_sure_non_zero_coin(coin: MyCoin): MyCoin {
        assert!(coin.value > 0, 0);
        coin
    }

    public fun has_coin(addr: address): bool {
        exists<MyCoin>(addr)
    }

    #[test]
    fun make_sure_non_zero_coin_passes() {
        let coin = MyCoin { value: 1 };
        let MyCoin { value: _ } = make_sure_non_zero_coin(coin);
    }

    #[test]
    // Or #[expected_failure] if we don't care about the abort code
    #[expected_failure(abort_code = 0)]
    fun make_sure_zero_coin_fails() {
        let coin = MyCoin { value: 0 };
        let MyCoin { value: _ } = make_sure_non_zero_coin(coin);
    }

    #[test_only] // test only helper function
    fun publish_coin(account: &signer) {
        move_to(account, MyCoin { value: 1 })
    }

    #[test(a = @0x1, b = @0x2)]
    fun test_has_coin(a: signer, b: signer) {
        publish_coin(&a);
        publish_coin(&b);
        assert!(has_coin(@0x1), 0);
        assert!(has_coin(@0x2), 1);
        assert!(!has_coin(@0x3), 1);
    }
}
```

### Running Tests

You can then run these tests with the `move test` command:

```
$ move test
BUILDING MoveStdlib
BUILDING TestExample
Running Move unit tests
[ PASS    ] 0x1::my_module::make_sure_non_zero_coin_passes
[ PASS    ] 0x1::my_module::make_sure_zero_coin_fails
[ PASS    ] 0x1::my_module::test_has_coin
Test result: OK. Total tests: 3; passed: 3; failed: 0
```

### Using Test Flags

#### `-f <str>` or `--filter <str>`
This will only run tests whose fully qualified name contains `<str>`. For example if we wanted to only run tests with `"zero_coin"` in their name:

```
$ move test -f zero_coin
CACHED MoveStdlib
BUILDING TestExample
Running Move unit tests
[ PASS    ] 0x1::my_module::make_sure_non_zero_coin_passes
[ PASS    ] 0x1::my_module::make_sure_zero_coin_fails
Test result: OK. Total tests: 2; passed: 2; failed: 0
```

#### `-i <bound>` or `--gas_used <bound>`
This bounds the amount of gas that can be consumed for any one test to `<bound>`:

```
$ move test -i 0
CACHED MoveStdlib
BUILDING TestExample
Running Move unit tests
[ TIMEOUT ] 0x1::my_module::make_sure_non_zero_coin_passes
[ TIMEOUT ] 0x1::my_module::make_sure_zero_coin_fails
[ TIMEOUT ] 0x1::my_module::test_has_coin

Test failures:

Failures in 0x1::my_module:

┌── make_sure_non_zero_coin_passes ──────
│ Test timed out
└──────────────────


┌── make_sure_zero_coin_fails ──────
│ Test timed out
└──────────────────


┌── test_has_coin ──────
│ Test timed out
└──────────────────

Test result: FAILED. Total tests: 3; passed: 0; failed: 3
```

#### `-s` or `--statistics`
With these flags you can gather statistics about the tests run and report the runtime and gas used for each test. For example, if we wanted to see the statistics for the tests in the example above:

```
$ move test -s
CACHED MoveStdlib
BUILDING TestExample
Running Move unit tests
[ PASS    ] 0x1::my_module::make_sure_non_zero_coin_passes
[ PASS    ] 0x1::my_module::make_sure_zero_coin_fails
[ PASS    ] 0x1::my_module::test_has_coin

Test Statistics:

┌────────────────────────────────────────────────┬────────────┬───────────────────────────┐
│                   Test Name                    │    Time    │   Gas Used   │
├────────────────────────────────────────────────┼────────────┼───────────────────────────┤
│ 0x1::my_module::make_sure_non_zero_coin_passes │   0.009    │             1             │
├────────────────────────────────────────────────┼────────────┼───────────────────────────┤
│ 0x1::my_module::make_sure_zero_coin_fails      │   0.008    │             1             │
├────────────────────────────────────────────────┼────────────┼───────────────────────────┤
│ 0x1::my_module::test_has_coin                  │   0.008    │             1             │
└────────────────────────────────────────────────┴────────────┴───────────────────────────┘

Test result: OK. Total tests: 3; passed: 3; failed: 0
```

#### `-g` or `--state-on-error`
These flags will print the global state for any test failures. e.g., if we added the following (failing) test to the `my_module` example:

```
module 0x1::my_module {
    ...
    #[test(a = @0x1)]
    fun test_has_coin_bad(a: signer) {
        publish_coin(&a);
        assert!(has_coin(@0x1), 0);
        assert!(has_coin(@0x2), 1);
    }
}
```

we would get the following output when running the tests:

```
$ move test -g
CACHED MoveStdlib
BUILDING TestExample
Running Move unit tests
[ PASS    ] 0x1::my_module::make_sure_non_zero_coin_passes
[ PASS    ] 0x1::my_module::make_sure_zero_coin_fails
[ PASS    ] 0x1::my_module::test_has_coin
[ FAIL    ] 0x1::my_module::test_has_coin_bad

Test failures:

Failures in 0x1::my_module:

┌── test_has_coin_bad ──────
│ error[E11001]: test failure
│    ┌─ /home/tzakian/TestExample/sources/my_module.move:47:10
│    │
│ 44 │      fun test_has_coin_bad(a: signer) {
│    │          ----------------- In this function in 0x1::my_module
│    ·
│ 47 │          assert!(has_coin(@0x2), 1);
│    │          ^^^^^^^^^^^^^^^^^^^^^^^^^^ Test was not expected to abort but it aborted with 1 here
│
│
│ ────── Storage state at point of failure ──────
│ 0x1:
│       => key 0x1::my_module::MyCoin {
│           value: 1
│       }
│
└──────────────────

Test result: FAILED. Total tests: 4; passed: 3; failed: 1
```
