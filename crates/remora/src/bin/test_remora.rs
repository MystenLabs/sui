use std::{
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    time::Duration,
};

use clap::Parser;
use futures::future;
use remora::{
    pre_exec_agent::PreExecAgent,
    primary_agent::PrimaryAgent,
    server::Server,
    tx_gen_agent::TxnGenAgent,
    types::{GlobalConfig, RemoraMessage, UniqueId},
};
use tokio::task::{JoinError, JoinHandle};

/// Top-level executor shard structure.
pub struct ExecutorShard {
    main_handle: JoinHandle<()>,
}

impl ExecutorShard {
    /// Run an executor shard (non blocking).
    pub fn start(global_configs: GlobalConfig, id: UniqueId) -> Self {
        let configs = global_configs.get(&id).expect("Unknown agent id");

        // Initialize and run the worker server.
        let kind = configs.kind.as_str();
        let main_handle = if kind == "GEN" {
            let mut server = Server::<TxnGenAgent, RemoraMessage>::new(global_configs, id);
            tokio::spawn(async move { server.run().await })
        } else if kind == "PRI" {
            let mut server = Server::<PrimaryAgent, RemoraMessage>::new(global_configs, id);
            tokio::spawn(async move { server.run().await })
        } else if kind == "PRE" {
            let mut server = Server::<PreExecAgent, RemoraMessage>::new(global_configs, id);
            tokio::spawn(async move { server.run().await })
        } else {
            panic!("Unexpected agent kind: {kind}");
        };

        Self { main_handle }
    }

    /// Await completion of the executor shard.
    pub async fn await_completion(self) -> Option<JoinError> {
        self.main_handle.await.ok()?;
        None
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Number of transactions to submit.
    #[arg(long, default_value_t = 1_000, global = true)]
    pub tx_count: u64,

    /// The minimum duration of the benchmark in seconds.
    #[clap(long, value_parser = parse_duration, default_value = "10", global = true)]
    duration: Duration,

    /// The working directory where the files will be generated.
    #[clap(
        long,
        value_name = "FILE",
        default_value = "~/working_dir",
        global = true
    )]
    working_directory: PathBuf,

    #[clap(subcommand)]
    operation: Operation,
}

fn parse_duration(arg: &str) -> Result<Duration, std::num::ParseIntError> {
    let seconds = arg.parse()?;
    Ok(Duration::from_secs(seconds))
}
#[derive(Parser)]
enum Operation {
    /// Deploy a local testbed
    Testbed {
        /// Number of pre_exec workers.
        #[clap(long, default_value_t = 1)]
        pre_exec_workers: usize,
    },
}

/// Deploy a local testbed of executor shards.
async fn deploy_testbed(tx_count: u64, duration: u64, pre_exec_workers: usize) -> GlobalConfig {
    let ips = vec![IpAddr::V4(Ipv4Addr::LOCALHOST); pre_exec_workers + 2];
    let mut global_configs = GlobalConfig::new_for_benchmark(ips, pre_exec_workers);

    // Insert workload.
    for id in 0..pre_exec_workers + 2 {
        global_configs.0.entry(id as UniqueId).and_modify(|e| {
            e.attrs.insert("tx_count".to_string(), tx_count.to_string());
            e.attrs.insert("duration".to_string(), duration.to_string());
        });
    }

    println!("Global configs: {:?}", global_configs);

    // Spawn txn generator.
    let configs = global_configs.clone();
    let id = 0;
    let _txn_generator = ExecutorShard::start(configs, id);
    // txn_generator.await_completion().await.unwrap();

    // FIXME: testing
    let handles = (1..pre_exec_workers + 2).map(|id| {
        let configs = global_configs.clone();
        async move {
            let worker = ExecutorShard::start(configs, id as UniqueId);
            worker.await_completion().await.unwrap()
        }
    });
    future::join_all(handles).await;
    global_configs
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args = Args::parse();
    let tx_count = args.tx_count;
    let duration = args.duration;

    match args.operation {
        Operation::Testbed { pre_exec_workers } => {
            deploy_testbed(tx_count, duration.as_secs(), pre_exec_workers).await;
        }
    }
}
