use clap::*;
use futures::future;
use prometheus::Registry;
use std::net::SocketAddr;
use std::net::{IpAddr, Ipv4Addr};
use std::{path::PathBuf, sync::Arc};
use sui_distributed_execution::sw_agent::*;
use sui_distributed_execution::types::*;
use sui_distributed_execution::{ew_agent::*, prometheus::start_prometheus_server};
use sui_distributed_execution::{metrics::Metrics, server::*};
use tokio::task::{JoinError, JoinHandle};

/// Top-level executor shard structure.
pub struct ExecutorShard {
    pub metrics: Arc<Metrics>,
    main_handle: JoinHandle<()>,
    _metrics_handle: JoinHandle<Result<(), hyper::Error>>,
}

impl ExecutorShard {
    /// Run an executor shard (non blocking).
    pub fn start(global_configs: GlobalConfig, id: UniqueId) -> Self {
        let configs = global_configs.get(&id).expect("Unknown agent id");

        // Run Prometheus server.
        let registry = Registry::new();
        let metrics = Arc::new(Metrics::new(&registry));
        let mut binding_metrics_address: SocketAddr = configs.metrics_address;
        binding_metrics_address.set_ip(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        let metrics_handle = start_prometheus_server(binding_metrics_address, &registry);

        // Initialize and run the worker server.
        let kind = configs.kind.as_str();
        let cloned_metrics = metrics.clone();
        let main_handle = if kind == "SW" {
            let mut server = Server::<SWAgent, SailfishMessage>::new(global_configs, id);
            tokio::spawn(async move { server.run(cloned_metrics).await })
        } else if kind == "EW" {
            let mut server = Server::<EWAgent, SailfishMessage>::new(global_configs, id);
            tokio::spawn(async move { server.run(cloned_metrics).await })
        } else {
            panic!("Unexpected agent kind: {kind}");
        };

        Self {
            metrics,
            main_handle,
            _metrics_handle: metrics_handle,
        }
    }

    /// Await completion of the executor shard.
    pub async fn await_completion(self) -> Result<Arc<Metrics>, JoinError> {
        self.main_handle.await?;
        Ok(self.metrics)
    }
}

/// Example config path.
const DEFAULT_CONFIG_PATH: &str = "crates/sui-distributed-execution/src/configs/1sw4ew.json";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Number of transactions to submit.
    #[arg(long, default_value_t = 1_000, global = true)]
    pub tx_count: u64,

    #[clap(subcommand)]
    operation: Operation,
}

#[derive(Parser)]
enum Operation {
    /// Deploy a single executor shard.
    Run {
        /// The id of this executor shard.
        #[clap(long)]
        id: UniqueId,

        /// Path to json config file.
        #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
        config_path: PathBuf,
    },

    /// Deploy a local testbed of executor shards.
    Testbed {
        /// Number of execution workers.
        #[clap(long, default_value_t = 4)]
        execution_workers: usize,
    },
}

#[tokio::main()]
async fn main() {
    let args = Args::parse();
    let tx_count = args.tx_count;

    match args.operation {
        Operation::Run { id, config_path } => {
            // Parse config from json
            let mut global_config = ServerConfig::from_path(config_path);
            global_config.entry(id).and_modify(|e| {
                e.attrs.insert("tx_count".to_string(), tx_count.to_string());
            });

            // Spawn the executor shard (blocking).
            ExecutorShard::start(global_config, id)
                .await_completion()
                .await
                .expect("Failed to run executor");
        }
        Operation::Testbed { execution_workers } => {
            deploy_testbed(tx_count, execution_workers).await;
        }
    }
}

/// Deploy a local testbed of executor shards.
async fn deploy_testbed(tx_count: u64, execution_workers: usize) -> Vec<Arc<Metrics>> {
    let ips = vec![IpAddr::V4(Ipv4Addr::LOCALHOST); execution_workers + 1];
    let mut global_configs = ServerConfig::new_for_benchmark(ips);

    // Insert workload.
    for id in 0..execution_workers + 1 {
        global_configs.entry(id as UniqueId).and_modify(|e| {
            e.attrs.insert("tx_count".to_string(), tx_count.to_string());
        });
    }

    // Spawn sequence worker.
    let configs = global_configs.clone();
    let id = 0;
    let _sequence_worker = ExecutorShard::start(configs, id);

    // Spawn execution workers.
    let handles = (1..execution_workers + 1).map(|id| {
        let configs = global_configs.clone();
        async move {
            let worker = ExecutorShard::start(configs, id as UniqueId);
            worker.await_completion().await.unwrap()
        }
    });
    future::join_all(handles).await
}

#[cfg(test)]
mod test {
    use crate::deploy_testbed;

    #[tokio::test]
    async fn smoke_test() {
        let tx_count = 300;
        let execution_workers = 4;
        let metrics = deploy_testbed(tx_count, execution_workers).await;

        assert!(metrics.iter().all(|m| m
            .latency_s
            .with_label_values(&["default"])
            .get_sample_count()
            == tx_count));
    }
}
