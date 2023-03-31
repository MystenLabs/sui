// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{benchmark::BenchmarkParameters, client::Instance};

pub mod sui;

/// The minimum interface that the protocol should implement to allow benchmarks from
/// the orchestrator.
pub trait ProtocolCommands {
    /// The port where the node exposes prometheus metrics.
    const NODE_METRICS_PORT: u16;
    /// The port where the client exposes prometheus metrics.
    const CLIENT_METRICS_PORT: u16;

    /// The list of dependencies to install (e.g., through apt-get).
    fn protocol_dependencies() -> Vec<&'static str>;

    /// The directories of all databases (that should be erased before each run).
    fn db_directories(&self) -> Vec<PathBuf>;

    /// The command to generate the genesis and all configuration files. This command
    /// is run on each remote machine.
    fn genesis_command<'a, I>(&self, instances: I) -> String
    where
        I: Iterator<Item = &'a Instance>;

    /// The command to run a node. This command is a function producing a string based
    /// on an input index (that should be node's unique index).
    fn node_command<'a, I>(&self, instances: I) -> Box<dyn Fn(usize) -> String>
    where
        I: Iterator<Item = &'a Instance>;

    /// The command to run a client. This command is a function producing a string based
    /// on an input index (that should be client's unique index).
    fn client_command<'a, I>(
        &self,
        instances: I,
        parameters: &BenchmarkParameters,
    ) -> Box<dyn Fn(usize) -> String>
    where
        I: Iterator<Item = &'a Instance>;
}

/// The names of the minimum metrics exposed by the load generators that are required to
/// compute performance.
pub trait ProtocolMetrics {
    /// The name of the metric reporting the total duration of the benchmark (in seconds).
    const BENCHMARK_DURATION: &'static str;
    /// The name of the metric reporting the total number of finalized transactions/
    const TOTAL_TRANSACTIONS: &'static str;
    /// The name of the metric reporting the latency buckets.
    const LATENCY_BUCKETS: &'static str;
    /// The name of the metric reporting the sum of the end-to-end latency of all finalized
    /// transactions.
    const LATENCY_SUM: &'static str;
    /// The name of the metric reporting the square of the sum of the end-to-end latency of all
    /// finalized transactions.
    const LATENCY_SQUARED_SUM: &'static str;
}

#[cfg(test)]
pub mod test_protocol_metrics {
    use super::ProtocolMetrics;

    pub struct TestProtocolMetrics;

    impl ProtocolMetrics for TestProtocolMetrics {
        const BENCHMARK_DURATION: &'static str = "benchmark_duration";
        const TOTAL_TRANSACTIONS: &'static str = "latency_s_count";
        const LATENCY_BUCKETS: &'static str = "latency_s";
        const LATENCY_SUM: &'static str = "latency_s_sum";
        const LATENCY_SQUARED_SUM: &'static str = "latency_squared_s";
    }
}
