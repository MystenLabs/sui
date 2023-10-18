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

#[tokio::main(flavor = "multi_thread", worker_threads = 12)]
async fn main() {
    let args = Args::parse();
    let config = NodeConfig::load(&args.config_path).unwrap();
    let genesis = Arc::new(config.genesis().expect("Could not load genesis"));
    let sw_attrs = HashMap::from([
        (
            "config".to_string(),
            args.config_path.to_string_lossy().into_owned(),
        ),
        ("download".to_string(), args.download.to_string()),
        ("execute".to_string(), args.execute.to_string()),
    ]);
    let mut sw_state = seqn_worker::SequenceWorkerState::new(0, &sw_attrs).await;
    println!("Download watermark: {:?}", sw_state.download);
    println!("Execute watermark: {:?}", sw_state.execute);

    // Channels from SW to EWs
    let mut sw2ew_senders = Vec::with_capacity(NUM_EXECUTION_WORKERS);
    // Channel from EWs to SW
    let (ew2sw_sender, ew2sw_receiver) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
    // Channels from EWs to other EWs
    let mut ew2ew_senders = Vec::new();
    let mut ew2ew_receivers = Vec::new();
    for _ in 0..NUM_EXECUTION_WORKERS {
        let (snd, rcv) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
        ew2ew_senders.push(snd);
        ew2ew_receivers.push(Some(rcv));
    }

    // Run Execution Workers
    let mut ew_handlers = Vec::new();

    for i in 0..NUM_EXECUTION_WORKERS {
        let store = DashMemoryBackedStore::new();
        let mut ew_state = exec_worker::ExecutionWorkerState::new(store);
        ew_state.init_store(&genesis);
        let metrics = sw_state.metrics.clone();

        let ew2sw_sender = ew2sw_sender.clone();
        let ew2ew_receiver = ew2ew_receivers[i].take().unwrap();
        let ew2ew_senders = ew2ew_senders.clone();
        let (sender, receiver) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
        sw2ew_senders.push(sender);

        ew_handlers.push(tokio::spawn(async move {
            ew_state
                .run(
                    metrics,
                    args.execute,
                    receiver,
                    ew2sw_sender,
                    ew2ew_receiver,
                    ew2ew_senders,
                    i as u8,
                )
                .await;
        }));
    }

    // Run Sequence Worker asynchronously
    let sw_handler = tokio::spawn(async move {
        sw_state.run(sw2ew_senders, ew2sw_receiver).await;
    });

    // Await for workers (EWs and SW) to finish.
    sw_handler.await.expect("sw failed");

    for (i, ew_handler) in ew_handlers.into_iter().enumerate() {
        ew_handler.await.expect(&format!("ew {} failed", i));
    }
}
