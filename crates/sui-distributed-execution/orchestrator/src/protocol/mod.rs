// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    benchmark::{BenchmarkParameters, BenchmarkType},
    client::Instance,
};

pub mod sailfish;

/// The minimum interface that the protocol should implement to allow benchmarks from
/// the orchestrator.
pub trait ProtocolCommands<T: BenchmarkType> {
    /// The list of dependencies to install (e.g., through apt-get).
    fn protocol_dependencies(&self) -> Vec<&'static str>;

    /// The directories of all databases (that should be erased before each run).
    fn db_directories(&self) -> Vec<PathBuf>;

    fn cleanup_commands(&self) -> Vec<String>;

    /// The command to generate the genesis and all configuration files. This command
    /// is run on each remote machine.
    fn genesis_command<'a, I>(&self, instances: I, parameters: &BenchmarkParameters<T>) -> String
    where
        I: Iterator<Item = &'a Instance>;

    /// The command to run a node. The function returns a vector of commands along with the
    /// associated instance on which to run the command.
    fn node_command<I>(
        &self,
        instances: I,
        parameters: &BenchmarkParameters<T>,
    ) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>;

    fn monitor_command<I>(&self, instances: I) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>;

    /// The command to run a client. The function returns a vector of commands along with the
    /// associated instance on which to run the command.
    fn client_command<I>(
        &self,
        instances: I,
        parameters: &BenchmarkParameters<T>,
    ) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>;
}

/// The names of the minimum metrics exposed by the load generators that are required to
/// compute performance.
pub trait ProtocolMetrics<T: BenchmarkType> {
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

    /// The network path where the nodes expose prometheus metrics.
    fn nodes_metrics_path<I>(
        &self,
        instances: I,
        parameters: &BenchmarkParameters<T>,
    ) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>;
    /// The command to retrieve the metrics from the nodes.
    fn nodes_metrics_command<I>(
        &self,
        instances: I,
        parameters: &BenchmarkParameters<T>,
    ) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        self.nodes_metrics_path(instances, parameters)
            .into_iter()
            .map(|(instance, path)| (instance, format!("curl {path}")))
            .collect()
    }

    /// The network path where the clients expose prometheus metrics.
    fn clients_metrics_path<I>(
        &self,
        instances: I,
        parameters: &BenchmarkParameters<T>,
    ) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>;
    /// The command to retrieve the metrics from the clients.
    fn clients_metrics_command<I>(
        &self,
        instances: I,
        parameters: &BenchmarkParameters<T>,
    ) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        self.clients_metrics_path(instances, parameters)
            .into_iter()
            .map(|(instance, path)| (instance, format!("curl {path}")))
            .collect()
    }
}

#[cfg(test)]
pub mod test_protocol_metrics {
    use std::{fmt::Display, str::FromStr};

    use serde::{Deserialize, Serialize};

    use crate::{
        benchmark::{BenchmarkParameters, BenchmarkType},
        client::Instance,
    };

    use super::ProtocolMetrics;

    /// Mock benchmark type for unit tests.
    #[derive(
        Serialize, Deserialize, Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash, Default,
    )]
    pub struct TestBenchmarkType;

    impl Display for TestBenchmarkType {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "TestBenchmarkType")
        }
    }

    impl FromStr for TestBenchmarkType {
        type Err = ();

        fn from_str(_s: &str) -> Result<Self, Self::Err> {
            Ok(Self {})
        }
    }

    impl BenchmarkType for TestBenchmarkType {}

    /// Mock protocol metrics for unit tests.
    pub struct TestProtocolMetrics;

    impl ProtocolMetrics<TestBenchmarkType> for TestProtocolMetrics {
        const BENCHMARK_DURATION: &'static str = "benchmark_duration";
        const TOTAL_TRANSACTIONS: &'static str = "latency_s_count";
        const LATENCY_BUCKETS: &'static str = "latency_s";
        const LATENCY_SUM: &'static str = "latency_s_sum";
        const LATENCY_SQUARED_SUM: &'static str = "latency_squared_s";

        fn nodes_metrics_path<I>(
            &self,
            instances: I,
            _parameters: &BenchmarkParameters<TestBenchmarkType>,
        ) -> Vec<(Instance, String)>
        where
            I: IntoIterator<Item = Instance>,
        {
            instances
                .into_iter()
                .enumerate()
                .map(|(i, instance)| (instance, format!("localhost:{}/metrics", 8000 + i as u16)))
                .collect()
        }

        fn clients_metrics_path<I>(
            &self,
            instances: I,
            _parameters: &BenchmarkParameters<TestBenchmarkType>,
        ) -> Vec<(Instance, String)>
        where
            I: IntoIterator<Item = Instance>,
        {
            instances
                .into_iter()
                .enumerate()
                .map(|(i, instance)| (instance, format!("localhost:{}/metrics", 9000 + i as u16)))
                .collect()
        }
    }
}
