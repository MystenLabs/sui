// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt::{Debug, Display},
    net::IpAddr,
    path::PathBuf,
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use sui_distributed_execution::types::GlobalConfig;

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
    sequence_workers: usize,
    /// Number of execution workers.
    execution_workers: usize,
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
        // let ips = instances
        //     .map(|x| x.main_ip.to_string())
        //     .collect::<Vec<_>>()
        //     .join(" ");
        // let working_directory = self.working_dir.display();

        // let disable_pipeline = if parameters.benchmark_type.disable_pipeline {
        //     "--disable-pipeline"
        // } else {
        //     ""
        // };
        // let number_of_leaders = parameters.benchmark_type.number_of_leaders;

        // let genesis = [
        //     &format!("{RUST_FLAGS} cargo run {CARGO_FLAGS} --bin mysticeti --"),
        //     "benchmark-genesis",
        //     &format!("--ips {ips} --working-directory {working_directory} {disable_pipeline} --number-of-leaders {number_of_leaders}"),
        // ]
        // .join(" ");

        // ["source $HOME/.cargo/env", &genesis].join(" && ")

        todo!()
    }

    fn monitor_command<I>(&self, instances: I) -> Vec<(Instance, String)>
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
        // instances
        //     .into_iter()
        //     .enumerate()
        //     .map(|(i, instance)| {
        //         let authority = i as AuthorityIndex;
        //         let committee_path: PathBuf =
        //             [&self.working_dir, &Committee::DEFAULT_FILENAME.into()]
        //                 .iter()
        //                 .collect();
        //         let parameters_path: PathBuf =
        //             [&self.working_dir, &Parameters::DEFAULT_FILENAME.into()]
        //                 .iter()
        //                 .collect();
        //         let private_configs_path: PathBuf = [
        //             &self.working_dir,
        //             &PrivateConfig::default_filename(authority),
        //         ]
        //         .iter()
        //         .collect();

        //         let env = env::var("ENV").unwrap_or_default();
        //         let run = [
        //             &env,
        //             &format!("{RUST_FLAGS} cargo run {CARGO_FLAGS} --bin mysticeti --"),
        //             "run",
        //             &format!(
        //                 "--authority {authority} --committee-path {}",
        //                 committee_path.display()
        //             ),
        //             &format!(
        //                 "--parameters-path {} --private-config-path {}",
        //                 parameters_path.display(),
        //                 private_configs_path.display()
        //             ),
        //         ]
        //         .join(" ");
        //         let tps = format!("export TPS={}", parameters.load / parameters.nodes);
        //         let tx_size = format!("export TRANSACTION_SIZE={}", parameters.benchmark_type.transaction_size);
        //         let consensus_only = if parameters.benchmark_type.consensus_only {
        //             format!("export CONSENSUS_ONLY={}", 1)
        //         } else {
        //             "".to_string()
        //         };
        //         let syncer = format!("export USE_SYNCER={}", 1);
        //         let command = ["#!/bin/bash -e", "source $HOME/.cargo/env", &tps, &tx_size, &consensus_only, &syncer, &run].join("\\n");
        //         let command = format!("echo -e '{command}' > mysticeti-start.sh && chmod +x mysticeti-start.sh && ./mysticeti-start.sh");

        //         (instance, command)
        //     })
        //     .collect()
        todo!()
    }

    fn client_command<I>(
        &self,
        _instances: I,
        _parameters: &BenchmarkParameters<SailfishBenchmarkType>,
    ) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        todo!()
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
    const BENCHMARK_DURATION: &'static str = "benchmark_duration";
    const TOTAL_TRANSACTIONS: &'static str = "latency_s_count";
    const LATENCY_BUCKETS: &'static str = sui_distributed_execution::metrics::LATENCY_S;
    const LATENCY_SUM: &'static str = "latency_s_sum";
    const LATENCY_SQUARED_SUM: &'static str = "latency_squared_s";

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
        let metrics_paths = GlobalConfig::new_for_benchmark(ips, sequence_workers)
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
        instances: I,
        parameters: &BenchmarkParameters<SailfishBenchmarkType>,
    ) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        // TODO: hack until we have benchmark clients.
        // self.nodes_metrics_path(instances, parameters)
        todo!()
    }
}
