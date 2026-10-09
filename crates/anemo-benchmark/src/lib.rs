// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(clippy::result_large_err)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/anemo_benchmark.Benchmark.rs"));
}
pub mod server;

pub use generated::{
    benchmark_client::BenchmarkClient,
    benchmark_server::{Benchmark, BenchmarkServer},
};
