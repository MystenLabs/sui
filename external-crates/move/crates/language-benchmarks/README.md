# Language Benchmarks

This crate contains benchmarks for the Move language, designed to evaluate performance and efficiency of various language features and constructs.

## Overview

The benchmarks in this crate help developers and contributors understand the runtime characteristics of Move programs. They are useful for profiling, regression testing, and optimizing the Move VM and standard library.


This [move_vm.rs](src/move_vm.rs) provides benchmarking utilities for Move VM modules using the Criterion crate.

It compiles Move source files, locates benchmark functions (functions whose names start with `bench`), and executes them in a Move VM runtime environment. The benchmarking is performed via Criterion, allowing for performance measurement of Move code.

Key components:
- Compilation of Move modules and dependencies, including the Move standard library.
- Setup of an in-memory Move VM test adapter with native functions.
- Discovery of benchmark functions in compiled modules.
- Execution and benchmarking of these functions using Criterion.

## Structure

- `benches/`: Contains individual benchmark files.
- `Cargo.toml`: Benchmark dependencies and configuration.

## Running Benchmarks

To run the benchmarks, use:

```bash
$ cargo bench
# To get a quick run reduce the warm up time and measurement time.
$ cargo bench -- --warm-up-time=1 --measurement-time=3
```

## Resources

- [Criterion](https://bheisler.github.io/criterion.rs/book/user_guide/command_line_options.html)
