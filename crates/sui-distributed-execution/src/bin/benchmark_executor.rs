use clap::*;
use prometheus::Registry;
use std::net::{IpAddr, Ipv4Addr};
use std::{fs, net::SocketAddr};
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

        // Signal that the server is up.
        metrics.up.inc();

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
    /// Generate a parameters files.
    Genesis {
        /// The list of ip addresses of the all validators.
        #[clap(long, value_name = "ADDR", value_delimiter = ' ', num_args(4..))]
        ips: Vec<IpAddr>,
        /// Number of sequence workers.
        #[clap(long, default_value_t = 1)]
        sequence_workers: usize,
        /// The working directory where the files will be generated.
        #[clap(long, value_name = "FILE", default_value = "genesis")]
        working_directory: PathBuf,
    },
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args = Args::parse();
    let tx_count = args.tx_count;

    match args.operation {
        Operation::Run { id, config_path } => {
            // Parse config from json
            let mut global_config = GlobalConfig::from_path(config_path);
            global_config.0.entry(id).and_modify(|e| {
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
        Operation::Genesis {
            ips,
            sequence_workers,
            working_directory,
        } => {
            tracing::info!("Generating benchmark genesis files");
            fs::create_dir_all(&working_directory).expect(&format!(
                "Failed to create directory '{}'",
                working_directory.display()
            ));
            let path = working_directory.join(GlobalConfig::DEFAULT_CONFIG_NAME);
            GlobalConfig::new_for_benchmark(ips, sequence_workers).export(path);
            tracing::info!("Generated configs.json");
        }
    }
}

/// Deploy a local testbed of executor shards.
async fn deploy_testbed(tx_count: u64, execution_workers: usize) -> GlobalConfig {
    let sequence_workers = 1;
    let ips = vec![IpAddr::V4(Ipv4Addr::LOCALHOST); execution_workers + 1];
    let mut global_configs = GlobalConfig::new_for_benchmark(ips, sequence_workers);

    // Insert workload.
    for id in 0..execution_workers + 1 {
        global_configs.0.entry(id as UniqueId).and_modify(|e| {
            e.attrs.insert("tx_count".to_string(), tx_count.to_string());
        });
    }

    // Spawn sequence worker.
    let configs = global_configs.clone();
    let id = 0;
    let _sequence_worker = ExecutorShard::start(configs, id);

    // Spawn execution workers.
    for id in 1..execution_workers + 1 {
        let configs = global_configs.clone();
        let _worker = ExecutorShard::start(configs, id as UniqueId);
    }

    global_configs
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use sui_distributed_execution::sw_agent::SWAgent;
    use tokio::time::sleep;

    use crate::deploy_testbed;

    #[tokio::test]
    async fn smoke_test() {
        let tx_count = 300;
        let execution_workers = 4;
        let workload = "default";
        let configs = deploy_testbed(tx_count, execution_workers).await;

        loop {
            sleep(Duration::from_secs(1)).await;
            let summary = SWAgent::summarize_metrics(&configs, workload).await;
            if !summary.unwrap().is_empty() {
                break;
            }
        }
    }
}
