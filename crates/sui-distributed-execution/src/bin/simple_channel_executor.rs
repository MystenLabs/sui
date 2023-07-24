use clap::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use sui_config::{Config, NodeConfig};
use sui_distributed_execution::{dash_store::DashMemoryBackedStore, exec_worker, seqn_worker};
use tokio::sync::mpsc;

const GIT_REVISION: &str = {
    if let Some(revision) = option_env!("GIT_REVISION") {
        revision
    } else {
        let version = git_version::git_version!(
            args = ["--always", "--dirty", "--exclude", "*"],
            fallback = ""
        );

        if version.is_empty() {
            panic!("unable to query git revision");
        }
        version
    }
};
const VERSION: &str = const_str::concat!(env!("CARGO_PKG_VERSION"), "-", GIT_REVISION);

const DEFAULT_CHANNEL_SIZE: usize = 1024;
const NUM_EXECUTION_WORKERS: usize = 4;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
#[clap(name = env!("CARGO_BIN_NAME"))]
#[clap(version = VERSION)]
struct Args {
    #[clap(long)]
    pub config_path: PathBuf,

    /// Specifies the watermark up to which I will download checkpoints
    #[clap(long)]
    download: u64,

    /// Specifies the watermark up to which I will execute checkpoints
    #[clap(long)]
    execute: u64,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let args = Args::parse();
    let config = NodeConfig::load(&args.config_path).unwrap();
    let genesis = Arc::new(config.genesis().expect("Could not load genesis"));
    let mut sw_state = seqn_worker::SequenceWorkerState::new(&config).await;

    // Channels from SW to EWs
    let mut sw_senders = Vec::with_capacity(NUM_EXECUTION_WORKERS);
    // Channel from EWs to SW
    let (ew_sender, ew_receiver) = mpsc::channel(DEFAULT_CHANNEL_SIZE);

    // Run Execution Workers
    let mut ew_handlers = Vec::new();
    if let Some(watermark) = args.execute {
        for i in 0..NUM_EXECUTION_WORKERS {
            let store = DashMemoryBackedStore::new();
            let mut ew_state = exec_worker::ExecutionWorkerState::new(store);
            ew_state.init_store(&genesis);
            let metrics = sw_state.metrics.clone();
            let ew_sender = ew_sender.clone();

            let (sw_sender, sw_receiver) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
            sw_senders.push(sw_sender);

            ew_handlers.push(tokio::spawn(async move {
                ew_state
                    .run(metrics, watermark, sw_receiver, ew_sender, i)
                    .await;
            }));
        }
    }

    // Run Sequence Worker asynchronously
    let sw_handler = tokio::spawn(async move {
        sw_state
            .run(
                config.clone(),
                args.download,
                args.execute,
                sw_senders,
                ew_receiver,
            )
            .await;
    });

    // Await for workers (EWs and SW) to finish.
    sw_handler.await.expect("sw failed");

    for (i, ew_handler) in ew_handlers.into_iter().enumerate() {
        ew_handler.await.expect(&format!("ew {} failed", i));
    }
}
