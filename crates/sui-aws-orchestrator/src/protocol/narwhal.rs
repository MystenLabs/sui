// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt::{Debug, Display},
    path::PathBuf,
    str::FromStr,
};

use crate::{
    benchmark::{BenchmarkParameters, BenchmarkType},
    client::Instance,
    settings::Settings,
};
use serde::{Deserialize, Serialize};

use super::{ProtocolCommands, ProtocolMetrics};

const NUM_WORKERS: usize = 1;
const BASE_PORT: usize = 5000;

// Narwhal default metrics port.
const DEFAULT_PORT: usize = 9184;

#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NarwhalBenchmarkType {
    /// The size of each transaction in bytes
    size: usize,
}

impl Debug for NarwhalBenchmarkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.size)
    }
}

impl Display for NarwhalBenchmarkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "tx size {}b", self.size)
    }
}

impl FromStr for NarwhalBenchmarkType {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            size: s.parse::<usize>()?.min(1000000),
        })
    }
}

impl BenchmarkType for NarwhalBenchmarkType {}

/// All configurations information to run a narwhal client or validator.
pub struct NarwhalProtocol {
    working_dir: PathBuf,
}

impl ProtocolCommands<NarwhalBenchmarkType> for NarwhalProtocol {
    fn protocol_dependencies(&self) -> Vec<&'static str> {
        vec![
            // Install typical narwhal dependencies.
            "sudo apt-get -y install curl git-all clang cmake gcc libssl-dev pkg-config libclang-dev",
            "sudo apt-get -y install libpq-dev",
        ]
    }

    fn db_directories(&self) -> Vec<PathBuf> {
        let consensus_db = [&self.working_dir, &"db-*".to_string().into()]
            .iter()
            .collect();

        let narwhal_config = [&self.working_dir].iter().collect();
        vec![consensus_db, narwhal_config]
    }

    fn genesis_command<'a, I>(&self, instances: I) -> String
    where
        I: Iterator<Item = &'a Instance>,
    {
        let working_dir = self.working_dir.display();
        let ips = instances
            .map(|x| x.main_ip.to_string())
            .collect::<Vec<_>>()
            .join(" ");

        let genesis = [
            "cargo run --release --bin narwhal-node benchmark-genesis",
            &format!(
                " --working-directory {working_dir} --ips {ips} --num-workers {NUM_WORKERS} --base-port {BASE_PORT}"
            ),
        ]
        .join(" ");

        [
            &format!("mkdir -p {working_dir}"),
            "source $HOME/.cargo/env",
            &genesis,
        ]
        .join(" && ")
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
        parameters: &BenchmarkParameters<NarwhalBenchmarkType>,
    ) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        let working_dir = self.working_dir.clone();
        let hosts: Vec<_> = instances.into_iter().collect();
        // 2 ports used per authority so add 2 * num authorities to base port
        let mut worker_base_port = BASE_PORT + (2 * hosts.len());

        let transaction_addresses: Vec<_> = hosts
            .iter()
            .map(|instance| {
                let transaction_address =
                    format!("http://{}:{}", instance.main_ip, worker_base_port);
                worker_base_port += 2;
                transaction_address
            })
            .collect();

        hosts
            .into_iter()
            .enumerate()
            .map(|(i, instance)| {
                let primary_keys: PathBuf = [&working_dir, &format!("primary-{i}-key.json").into()]
                    .iter()
                    .collect();
                let primary_network_keys: PathBuf = [
                    &working_dir,
                    &format!("primary-{i}-network-key.json").into(),
                ]
                .iter()
                .collect();
                // todo: add logic for multiple workers
                let worker_keys: PathBuf = [&working_dir, &format!("worker-{i}-key.json").into()]
                    .iter()
                    .collect();
                let committee: PathBuf = [&working_dir, &"committee.json".to_string().into()]
                    .iter()
                    .collect();
                let workers: PathBuf = [&working_dir, &"workers.json".to_string().into()]
                    .iter()
                    .collect();
                let store: PathBuf = [&working_dir, &format!("db-{i}").into()].iter().collect();
                let nw_parameters: PathBuf = [&working_dir, &"parameters.json".to_string().into()]
                    .iter()
                    .collect();

                let run = [
                    "sudo sysctl -w net.core.wmem_max=104857600 && ",
                    "sudo sysctl -w net.core.rmem_max=104857600 && ",
                    "ulimit -n 51200 && ", // required so we can scale the client
                    "RUST_LOG=debug cargo run --release --bin narwhal-node run ",
                    &format!(
                        "--primary-keys {} --primary-network-keys {} ",
                        primary_keys.display(),
                        primary_network_keys.display()
                    ),
                    &format!(
                        "--worker-keys {} --committee {} --workers {} ",
                        worker_keys.display(),
                        committee.display(),
                        workers.display()
                    ),
                    &format!(
                        "--store {} --parameters {} benchmark ",
                        store.display(),
                        nw_parameters.display()
                    ),
                    &format!(
                        "--worker-id 0 --addr {} --size {} --rate {} --nodes {}",
                        transaction_addresses[i],
                        parameters.benchmark_type.size,
                        parameters.load / parameters.nodes,
                        transaction_addresses.join(","),
                    ),
                ]
                .join(" ");
                let command = ["source $HOME/.cargo/env", &run].join(" && ");

                (instance, command)
            })
            .collect()
    }

    fn client_command<I>(
        &self,
        _instances: I,
        _parameters: &BenchmarkParameters<NarwhalBenchmarkType>,
    ) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        // client is started in process with the primary/worker via node_command,
        // so nothing to start here.
        vec![]
    }
}

impl NarwhalProtocol {
    /// Make a new instance of the Narwhal protocol commands generator.
    pub fn new(settings: &Settings) -> Self {
        Self {
            working_dir: [&settings.working_dir, &"narwhal_config".into()]
                .iter()
                .collect(),
        }
    }
}

impl ProtocolMetrics for NarwhalProtocol {
    const BENCHMARK_DURATION: &'static str = "narwhal_benchmark_duration";
    // TODO: Improve metrics used for benchmark summary.
    // Currently the only route should be `SubmitTransaction` so this should be a
    // good proxy for total tx
    const TOTAL_TRANSACTIONS: &'static str = "worker_req_latency_by_route_count";
    // Does not include the time taken for the tx to be included in the batch, only
    // from batch creation to when the batch is fetched for execution
    const LATENCY_BUCKETS: &'static str = "batch_execution_latency";
    const LATENCY_SUM: &'static str = "batch_execution_latency_sum";
    // Measuring client submit latency but this only factors in submission and
    // not time to commit
    const LATENCY_SQUARED_SUM: &'static str = "narwhal_client_latency_squared_s";

    fn nodes_metrics_path<I>(&self, instances: I) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        instances
            .into_iter()
            .map(|instance| {
                let path = format!(
                    "{}:{}{}",
                    instance.main_ip,
                    DEFAULT_PORT,
                    mysten_metrics::METRICS_ROUTE
                );
                (instance, path)
            })
            .collect()
    }

    fn clients_metrics_path<I>(&self, _instances: I) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        vec![]
    }
}
