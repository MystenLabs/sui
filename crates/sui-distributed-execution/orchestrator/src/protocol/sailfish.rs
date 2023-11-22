// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt::{Debug, Display},
    net::IpAddr,
    path::PathBuf,
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use sui_distributed_execution::types::{GlobalConfig, UniqueId};

use crate::{
    benchmark::{BenchmarkParameters, BenchmarkType},
    client::Instance,
    settings::Settings,
};

use super::{ProtocolCommands, ProtocolMetrics};

const CARGO_FLAGS: &str = "--release";
const RUST_FLAGS: &str = "RUSTFLAGS=-C\\ target-cpu=native";

/// The type of benchmarks supported by protocol under test.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SailfishBenchmarkType {
    /// Number of sequence workers.
    pub sequence_workers: usize,
    /// Number of execution workers.
    pub execution_workers: usize,
}

impl Default for SailfishBenchmarkType {
    fn default() -> Self {
        Self {
            sequence_workers: 1,
            execution_workers: 4,
        }
    }
}

impl Debug for SailfishBenchmarkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.sequence_workers, self.execution_workers)
    }
}

impl Display for SailfishBenchmarkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SE{}-EW{}",
            self.sequence_workers, self.execution_workers
        )
    }
}

impl FromStr for SailfishBenchmarkType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Ok(Self::default());
        }

        let parameters = s.split("-").collect::<Vec<_>>();
        Ok(Self {
            sequence_workers: parameters[0].parse::<usize>().map_err(|e| e.to_string())?,
            execution_workers: parameters[1].parse::<usize>().map_err(|e| e.to_string())?,
        })
    }
}

impl BenchmarkType for SailfishBenchmarkType {}

/// All configurations information to run Sailfish workers.
pub struct SailfishProtocol {
    working_dir: PathBuf,
}

impl ProtocolCommands<SailfishBenchmarkType> for SailfishProtocol {
    const BIN_NAME: &'static str = "benchmark_executor";

    fn protocol_dependencies(&self) -> Vec<&'static str> {
        vec![]
    }

    fn db_directories(&self) -> Vec<PathBuf> {
        vec![]
    }

    fn cleanup_commands(&self) -> Vec<String> {
        vec![]
    }

    fn genesis_command<'a, I>(
        &self,
        instances: I,
        parameters: &BenchmarkParameters<SailfishBenchmarkType>,
    ) -> String
    where
        I: Iterator<Item = &'a Instance>,
    {
        let ips = instances
            .map(|x| x.main_ip.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let working_directory = self.working_dir.display();
        let sequence_workers = parameters.benchmark_type.sequence_workers;

        let genesis = [
            &format!("{RUST_FLAGS} cargo run {CARGO_FLAGS} --bin {} --", Self::BIN_NAME),
            "genesis",
            &format!("--ips {ips} --working-directory {working_directory} --sequence-workers {sequence_workers}"),
        ]
        .join(" ");

        ["source $HOME/.cargo/env", &genesis].join(" && ")
    }

    fn monitor_command<I>(&self, _instances: I) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        vec![]
    }

    fn node_command<I>(
        &self,
        instances: I,
        parameters: &BenchmarkParameters<SailfishBenchmarkType>,
    ) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        instances
            .into_iter()
            .enumerate()
            .map(|(i, instance)| {
                let id = (i + parameters.benchmark_type.sequence_workers) as UniqueId;
                let config_path: PathBuf =
                    [&self.working_dir, &GlobalConfig::DEFAULT_CONFIG_NAME.into()]
                        .iter()
                        .collect();

                let run = [
                    &format!(
                        "{RUST_FLAGS} cargo run {CARGO_FLAGS} --bin {} --",
                        Self::BIN_NAME
                    ),
                    "run",
                    &format!(
                        "--id {id} --config-path {} --tx-count {}",
                        config_path.display(),
                        parameters.load
                    ),
                ]
                .join(" ");

                let command = ["#!/bin/bash -e", "source $HOME/.cargo/env", &run].join("\\n");
                let command = format!(
                    "echo -e '{command}' > ew-start.sh && chmod +x ew-start.sh && ./ew-start.sh"
                );

                (instance, command)
            })
            .collect()
    }

    fn client_command<I>(
        &self,
        instances: I,
        _parameters: &BenchmarkParameters<SailfishBenchmarkType>,
    ) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        instances
            .into_iter()
            .enumerate()
            .map(|(i, instance)| {
                let id = i as UniqueId;
                let config_path: PathBuf =
                    [&self.working_dir, &GlobalConfig::DEFAULT_CONFIG_NAME.into()]
                        .iter()
                        .collect();

                let run = [
                    &format!(
                        "{RUST_FLAGS} cargo run {CARGO_FLAGS} --bin {} --",
                        Self::BIN_NAME
                    ),
                    "run",
                    &format!("--id {id} --config-path {}", config_path.display()),
                ]
                .join(" ");

                let command = ["#!/bin/bash -e", "source $HOME/.cargo/env", &run].join("\\n");
                let command = format!(
                    "echo -e '{command}' > sw-start.sh && chmod +x sw-start.sh && ./sw-start.sh"
                );

                (instance, command)
            })
            .collect()
    }
}

impl SailfishProtocol {
    /// Make a new instance of the Mysticeti protocol commands generator.
    pub fn new(settings: &Settings) -> Self {
        Self {
            working_dir: settings.working_dir.clone(),
        }
    }
}

impl ProtocolMetrics<SailfishBenchmarkType> for SailfishProtocol {
    const BENCHMARK_DURATION: &'static str = sui_distributed_execution::metrics::BENCHMARK_DURATION;
    const TOTAL_TRANSACTIONS: &'static str = "latency_s_count";
    const LATENCY_BUCKETS: &'static str = sui_distributed_execution::metrics::LATENCY_S;
    const LATENCY_SUM: &'static str = "latency_s_sum";
    const LATENCY_SQUARED_SUM: &'static str =
        sui_distributed_execution::metrics::LATENCY_SQUARED_SUM;

    fn nodes_metrics_path<I>(
        &self,
        instances: I,
        parameters: &BenchmarkParameters<SailfishBenchmarkType>,
    ) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        let (ips, instances): (_, Vec<_>) = instances
            .into_iter()
            .map(|x| (IpAddr::V4(x.main_ip), x))
            .unzip();
        let sequence_workers = parameters.benchmark_type.sequence_workers;
        let metrics_paths = GlobalConfig::new_for_benchmark_ew_only(ips, sequence_workers)
            .execution_workers_metric_addresses()
            .into_iter()
            .map(|x| {
                format!(
                    "{x}{}",
                    sui_distributed_execution::prometheus::METRICS_ROUTE
                )
            });

        instances.into_iter().zip(metrics_paths).collect()
    }

    fn clients_metrics_path<I>(
        &self,
        _instances: I,
        _parameters: &BenchmarkParameters<SailfishBenchmarkType>,
    ) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        // TODO: hack until we have benchmark clients.
        // self.nodes_metrics_path(instances, parameters)
        vec![]
    }
}
