# Tests for the Move Prover

This directory contains the unit tests for the Move Prover. Those tests are using baseline expectations of produced
prover output (no output on success, specific diagnosis on failure). In addition to the tests here, the prover's
stability also depends on verification tests of the Move standard library and the Diem framework, which are run via the
prover's integration into the Move CLI and configured outside this tree.

> NOTE: in order to run these tests locally, you must have installed tools and setup a few
> environment variables. See [`../doc/user/install.md`](../doc/user/install.md) for details. If the
> environment variables for configuring the prover are not set as described there, **all tests and this
> directory will trivially pass**.

> NOTE: these are baseline tests, with expectations of prover output stored in files ending in
> `.exp`. To update those files, use `UPBL=1 cargo test`. To update or test a single file, you can
> also provide a fragment of the Move source path.

## Running the Prover on Sources in this Tree

> NOTE: in contrast to older versions, the prover does not longer automatically pick a configuration file via the MOVE_PROVER_CONFIG variable.

The sources in this tree can currently not be integrated with the Move package system, since some of them don't compile
in combination. To call the prover directly, skipping the package system, use a command as below:

```shell
alias mvp=cargo run -p move-prover -- --config=<my_config.toml>
```

The file at the path to `<my_config.toml>` should contain (at least) the following content:

```toml
move_deps = [
    "<your-path-to>/diem/language/move-stdlib/sources"
]
move_named_address_values = [
    "Std=0x1",
]
```

## Running Tests: Quick Guide

- In order to regenerate baseline files, use `UPBL=1 cargo test <optional test filter>`
- In order to narrow tests to a particular feature, use `MVP_TEST_FEATURE=<feature> cargo test`. If not set, all
  features will be tested for each test they are enabled for. (See discussion below about feature enabling).
- In order to run tests with consistency checking enabled, use `MVP_TEST_INCONSISTENCY=1 cargo test`.
- In order to run tests with a specific flag combination, use `MVP_TEST_FLAGS=<flags> cargo test`.
- In order to run the tests in the `tests/xsources` tree instead of the default locations, use
  `MVP_TEST_X=1 cargo test`.

Certain comments in the test sources are interpreted by the test driver as test directives. A directive is a single line
comment in the source of the form `// <directive>: <value>`. Directives can be repeated. The following directives are
supported:

- `// flag: <flags>` to run the test with the given flags in addition to the default flags.
- `// no_ci:` exclude this test from running in CI.
- `// exclude_for: <feature>` to exclude a test for a feature configured as "inclusive" (see below).
- `// also_include_for: <feature>` to include a test for a feature configured as
  "exclusive".

Features can be either inclusive or exclusive. For an inclusive feature all tests are run unless explicitly excluded
with `// exclude_for`. For an exclusive feature only those tests are run which have the directive `// also_include_for`.

Currently, the following features are available:

- `default`: runs tests with all default flags.
- `no_opaque`: runs tests in a special mode where the `opaque` pragma is ignored. This increases the load on the prover,
  and functions as a stress test.
- `cvc5`: runs tests configured to use the cvc5 solver as a backend.

## Conventions

There is a convention for test cases under this directory. In general, there are two kinds of test cases, which can be
mixed in a file. The first type of test cases are "correct" Move functions which are expected to be proven so. Another
type of test cases are incorrect Move functions which are expected to be disproven, with the created errors stored in
so-called 'expectation baseline files' (`.exp`). The incorrect functions have suffix `_incorrect` in their names, by
convention. It is expected that only errors for functions with this suffix appear in `.exp` files.

## Debugging Long Running Tests

By default, the prover uses a timeout of 40 seconds, which can be changed by the `-T=<seconds>`
flag. Healthy tests should never take that long to finish. To avoid flakes in continuous integration, you should test
your tests to be able to pass at least with `-T=20`. To do so use

```shell script
MVP_TEST_FLAGS="-T=20" cargo test -p move-prover
```

## Inconsistency Check

If the flag `--check-inconsistency` is given, the prover not only verifies a target, but also checks if there is any
inconsistent assumption in the verification. If the environment variable `MVP_TEST_INCONSISTENCY=1` is set, `cargo test`
will perform the inconsistency check while running the tests in `sources` (i.e., the prover will run those tests with the flag `--check-inconsistency`).

```shell script
MVP_TEST_INCONSISTENCY=1 cargo test -p move-prover
```
