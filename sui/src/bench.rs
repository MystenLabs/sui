// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;

use sui::benchmark::{bench_types, run_benchmark};

fn main() {
    let benchmark = bench_types::Benchmark::parse();

    let r = run_benchmark(benchmark);
    println!("{}", r);
}
