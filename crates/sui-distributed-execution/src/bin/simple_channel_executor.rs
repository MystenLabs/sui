use clap::*;
use std::collections::HashMap;
use sui_config::{Config, NodeConfig};
use std::path::PathBuf;
use std::sync::Arc;
use sui_distributed_execution::{
    seqn_worker,
    exec_worker,
    dash_store::DashMemoryBackedStore,
};
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

const DEFAULT_CHANNEL_SIZE:usize = 1024;


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

    // Initialize SW
    let sw_attrs = HashMap::from([
        ("config".to_string(), args.config_path.to_string_lossy().into_owned()),
        ("download".to_string(), args.download.to_string()),
        ("execute".to_string(), args.download.to_string()),
    ]);
    
    let mut sw_state = seqn_worker::SequenceWorkerState::new(0, sw_attrs).await;    

    let metrics = sw_state.metrics.clone();

    // Channel from sw to ew
    let (sw_sender, sw_receiver) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
    // Channel from ew to sw
    let (ew_sender, ew_receiver) = mpsc::channel(DEFAULT_CHANNEL_SIZE);

    // Run Sequence Worker asynchronously
    let sw_handler = tokio::spawn(async move {
        sw_state.run(
            sw_sender, 
            ew_receiver, 
        ).await;
    });

    // Initialize EW
    let config = NodeConfig::load(&args.config_path).unwrap();
    let genesis = Arc::new(config.genesis().expect("Could not load genesis"));
    
    let store = DashMemoryBackedStore::new(); // use the mutexed store for concurrency control
    let mut ew_state = exec_worker::ExecutionWorkerState::new(store);
    ew_state.init_store(&genesis);

    // Run Execution Worker
    let ew_handler = tokio::spawn(async move {
        ew_state.run(
            metrics,
            args.execute,
            sw_receiver,
            ew_sender
        ).await;
    });

    // Wait for workers to terminate
    sw_handler.await.expect("sw failed");
    ew_handler.await.expect("ew failed")
}
