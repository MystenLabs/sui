use clap::*;
use prometheus::Registry;
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
};
use std::{fs, net::SocketAddr};
use std::{path::PathBuf, sync::Arc};
use sui_distributed_execution::sw_agent::*;
use sui_distributed_execution::types::*;
use sui_distributed_execution::{ew_agent::*, prometheus::start_prometheus_server};
use sui_distributed_execution::{metrics::Metrics, server::*};
use tokio::task::{JoinError, JoinHandle};

const FILE_PATH: &str = "crates/sui-distributed-execution/src/configs/1sw4ew.json";

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
struct Args {
    #[clap(long)]
    pub my_id: UniqueId,

    #[arg(
        long,
        default_value_t = 1_000,
        help = "Number of transactions to submit"
    )]
    pub tx_count: u64,

    #[arg(long, default_value=FILE_PATH, help="Path to json config file")]
    pub config_path: PathBuf,
}

#[tokio::main()]
async fn main() {
    // Parse command line
    let args = Args::parse();
    let my_id = args.my_id;
    let tx_count = args.tx_count;

    // Parse config from json
    let config_json = fs::read_to_string(args.config_path).expect("Failed to read config file");
    let mut global_config: HashMap<UniqueId, ServerConfig> =
        serde_json::from_str(&config_json).unwrap();
    global_config.entry(my_id).and_modify(|e| {
        e.attrs.insert("tx_count".to_string(), tx_count.to_string());
    });

    // Spawn the executor shard (blocking).
    ExecutorShard::start(global_config, my_id)
        .await_completion()
        .await
        .expect("Failed to run executor");
}

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

#[cfg(test)]
mod test {
    use std::net::{IpAddr, Ipv4Addr};

    use futures::future;
    use sui_distributed_execution::types::{ServerConfig, UniqueId};

    use crate::ExecutorShard;

    #[tokio::test]
    async fn smoke_test() {
        let tx_count = 1000;
        let execution_workers = 4;
        let ips = vec![IpAddr::V4(Ipv4Addr::LOCALHOST); execution_workers + 1];
        let mut global_configs = ServerConfig::new_for_benchmark(ips);

        println!("global_configs: {:?}", global_configs);

        // Insert workload.
        for id in 0..execution_workers + 1 {
            global_configs.entry(id as u16).and_modify(|e| {
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
        let metrics = future::join_all(handles).await;

        // Ensure that all execution workers processed the transactions.
        assert!(metrics.iter().all(|m| m
            .latency_s
            .with_label_values(&["default"])
            .get_sample_count()
            == tx_count));
    }
}
