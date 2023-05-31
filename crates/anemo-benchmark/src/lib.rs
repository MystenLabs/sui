// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod generated {
    include!(concat!(env!("OUT_DIR"), "/anemo_benchmark.Benchmark.rs"));
}
pub mod server;

pub use generated::{
    benchmark_client::BenchmarkClient,
    benchmark_server::{Benchmark, BenchmarkServer},
};
