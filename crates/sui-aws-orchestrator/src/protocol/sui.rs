// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt::{Debug, Display},
    path::PathBuf,
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use sui_swarm_config::genesis_config::GenesisConfig;
use sui_types::{base_types::SuiAddress, multiaddr::Multiaddr};

use crate::{
    benchmark::{BenchmarkParameters, BenchmarkType},
    client::Instance,
    settings::Settings,
};

use super::{ProtocolCommands, ProtocolMetrics};

#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SuiBenchmarkType {
    /// Percentage of shared vs owned objects; 0 means only owned objects and 100 means
    /// only shared objects.
    shared_objects_ratio: u16,
}

impl Debug for SuiBenchmarkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.shared_objects_ratio)
    }
}

impl Display for SuiBenchmarkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}% shared objects", self.shared_objects_ratio)
    }
}

impl FromStr for SuiBenchmarkType {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            shared_objects_ratio: s.parse::<u16>()?.min(100),
        })
    }
}

impl BenchmarkType for SuiBenchmarkType {}

/// All configurations information to run a Sui client or validator.
pub struct SuiProtocol {
    working_dir: PathBuf,
}

impl ProtocolCommands<SuiBenchmarkType> for SuiProtocol {
    fn protocol_dependencies(&self) -> Vec<&'static str> {
        vec![
            // Install typical sui dependencies.
            "sudo apt-get -y install curl git-all clang cmake gcc libssl-dev pkg-config libclang-dev",
            // This dependency is missing from the Sui docs.
            "sudo apt-get -y install libpq-dev",
        ]
    }

    fn db_directories(&self) -> Vec<PathBuf> {
        let authorities_db = [&self.working_dir, &sui_config::AUTHORITIES_DB_NAME.into()]
            .iter()
            .collect();
        let consensus_db = [&self.working_dir, &sui_config::CONSENSUS_DB_NAME.into()]
            .iter()
            .collect();
        vec![authorities_db, consensus_db]
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
            "cargo run --release --bin sui --",
            "genesis",
            &format!("-f --working-dir {working_dir} --benchmark-ips {ips}"),
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
        // instances
        //     .into_iter()
        //     .map(|i| {
        //         (
        //             i,
        //             "tail -f --pid=$(pidof sui) -f /dev/null; tail -100 node.log".to_string(),
        //         )
        //     })
        //     .collect()
        vec![]
    }

    fn node_command<I>(
        &self,
        instances: I,
        _parameters: &BenchmarkParameters<SuiBenchmarkType>,
    ) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        let working_dir = self.working_dir.clone();
        let network_addresses = Self::resolve_network_addresses(instances);

        network_addresses
            .into_iter()
            .enumerate()
            .map(|(i, (instance, network_address))| {
                let validator_config =
                    sui_config::validator_config_file(network_address.clone(), i);
                let config_path: PathBuf = working_dir.join(validator_config);

                let run = [
                    "cargo run --release --bin sui-node --",
                    &format!(
                        "--config-path {} --listen-address {}",
                        config_path.display(),
                        network_address.with_zero_ip()
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
        instances: I,
        parameters: &BenchmarkParameters<SuiBenchmarkType>,
    ) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        let genesis_path: PathBuf = [&self.working_dir, &sui_config::SUI_GENESIS_FILENAME.into()]
            .iter()
            .collect();
        let keystore_path: PathBuf = [
            &self.working_dir,
            &sui_config::SUI_BENCHMARK_GENESIS_GAS_KEYSTORE_FILENAME.into(),
        ]
        .iter()
        .collect();

        let committee_size = parameters.nodes;
        let clients: Vec<_> = instances.into_iter().collect();
        let load_share = parameters.load / clients.len();
        let shared_counter = parameters.benchmark_type.shared_objects_ratio;
        let transfer_objects = 100 - shared_counter;
        let metrics_port = Self::CLIENT_METRICS_PORT;
        let gas_keys = GenesisConfig::benchmark_gas_keys(committee_size);

        clients
            .into_iter()
            .enumerate()
            .map(|(i, instance)| {
                let genesis = genesis_path.display();
                let keystore = keystore_path.display();
                let gas_key = &gas_keys[i % committee_size];
                let gas_address = SuiAddress::from(&gas_key.public());

                let run = [
                    "cargo run --release --bin stress --",
                    "--num-client-threads 24 --num-server-threads 1",
                    "--local false --num-transfer-accounts 2",
                    &format!("--genesis-blob-path {genesis} --keystore-path {keystore}",),
                    &format!("--primary-gas-owner-id {gas_address}"),
                    "bench",
                    &format!("--in-flight-ratio 30 --num-workers 24 --target-qps {load_share}"),
                    &format!(
                        "--shared-counter {shared_counter} --transfer-object {transfer_objects}"
                    ),
                    "--shared-counter-hotness-factor 50",
                    &format!("--client-metric-host 0.0.0.0 --client-metric-port {metrics_port}"),
                ]
                .join(" ");
                let command = ["source $HOME/.cargo/env", &run].join(" && ");

                (instance, command)
            })
            .collect()
    }
}

impl SuiProtocol {
    const CLIENT_METRICS_PORT: u16 = GenesisConfig::BENCHMARKS_PORT_OFFSET + 2000;

    /// Make a new instance of the Sui protocol commands generator.
    pub fn new(settings: &Settings) -> Self {
        Self {
            working_dir: [&settings.working_dir, &sui_config::SUI_CONFIG_DIR.into()]
                .iter()
                .collect(),
        }
    }

    /// Creates the network addresses in multi address format for the instances. It returns the
    /// Instance and the corresponding address.
    pub fn resolve_network_addresses(
        instances: impl IntoIterator<Item = Instance>,
    ) -> Vec<(Instance, Multiaddr)> {
        let instances: Vec<Instance> = instances.into_iter().collect();
        let ips: Vec<_> = instances.iter().map(|x| x.main_ip.to_string()).collect();
        let genesis_config = GenesisConfig::new_for_benchmarks(&ips);
        let mut addresses = Vec::new();
        if let Some(validator_configs) = genesis_config.validator_config_info.as_ref() {
            for (i, validator_info) in validator_configs.iter().enumerate() {
                let address = &validator_info.network_address;
                addresses.push((instances[i].clone(), address.clone()));
            }
        }
        addresses
    }
}

impl ProtocolMetrics for SuiProtocol {
    const BENCHMARK_DURATION: &'static str = "benchmark_duration";
    const TOTAL_TRANSACTIONS: &'static str = "latency_s_count";
    const LATENCY_BUCKETS: &'static str = "latency_s";
    const LATENCY_SUM: &'static str = "latency_s_sum";
    const LATENCY_SQUARED_SUM: &'static str = "latency_squared_s";

    fn nodes_metrics_path<I>(&self, instances: I) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        let (ips, instances): (Vec<_>, Vec<_>) = instances
            .into_iter()
            .map(|x| (x.main_ip.to_string(), x))
            .unzip();

        GenesisConfig::new_for_benchmarks(&ips)
            .validator_config_info
            .expect("No validator in genesis")
            .iter()
            .zip(instances)
            .map(|(config, instance)| {
                let path = format!(
                    "{}:{}{}",
                    instance.main_ip,
                    config.metrics_address.port(),
                    mysten_metrics::METRICS_ROUTE
                );
                (instance, path)
            })
            .collect()
    }

    fn clients_metrics_path<I>(&self, instances: I) -> Vec<(Instance, String)>
    where
        I: IntoIterator<Item = Instance>,
    {
        instances
            .into_iter()
            .map(|instance| {
                let path = format!(
                    "{}:{}{}",
                    instance.main_ip,
                    Self::CLIENT_METRICS_PORT,
                    mysten_metrics::METRICS_ROUTE
                );
                (instance, path)
            })
            .collect()
    }
}
