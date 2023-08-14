use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use clap::*;
use sui_adapter_latest::programmable_transactions;
use sui_config::{Config, NodeConfig};
use sui_distributed_execution::{
    seqn_worker,
    exec_worker,
    dash_store::DashMemoryBackedStore,
    types::SailfishMessage,
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

const DEFAULT_CHANNEL_SIZE: usize = 1024;

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

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Initialize SW
    let sw_attrs = HashMap::from([
        ("config".to_string(), args.config_path.to_string_lossy().into_owned()),
        ("download".to_string(), args.download.to_string()),
        ("execute".to_string(), args.download.to_string()),
    ]);
    
    let mut sw_state = seqn_worker::SequenceWorkerState::new(0, sw_attrs).await;    
    let metrics1 = sw_state.metrics.clone();
    let metrics2 = sw_state.metrics.clone();

    // Initialize EW
    let config = NodeConfig::load(&args.config_path).unwrap();
    let genesis = Arc::new(config.genesis().expect("Could not load genesis"));
    
    let store1 = DashMemoryBackedStore::new(); // use the mutexed store for concurrency control
    let store2 = DashMemoryBackedStore::new(); // use the mutexed store for concurrency control
    let mut ew_state1 = exec_worker::ExecutionWorkerState::new(store1);
    let mut ew_state2 = exec_worker::ExecutionWorkerState::new(store2);
    ew_state1.init_store(&genesis);
    ew_state2.init_store(&genesis);

    // ==== Run Both (EW + SW) ====
    // Results from here are used by the EW below to substitute the missing SW.

    // Channel from sw to ew
    let (sw_sender, mut cs_receiver) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
    let (cs_sender1, sw_receiver1) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
    let (cs_sender2, mut sw_receiver2) = mpsc::unbounded_channel();
    // Channel from ew to sw
    let (ew_sender1, ew_receiver1) = mpsc::channel(DEFAULT_CHANNEL_SIZE);

    // Run Sequence Worker
    let sw_handler = tokio::spawn(async move {
        sw_state.run(
            sw_sender,
            ew_receiver1,
        ).await;
    });

    // This task copies each message to a channel for the 2nd Execution Worker
    // before passing it on to the channel for the 1st Execution Worker.
    let channel_splitter = tokio::spawn(async move {
        while let Some(msg) = cs_receiver.recv().await {
            cs_sender1.send(msg.clone()).await.expect("send failed");
            //let permit = cs_sender2.try_reserve().expect("channel full");
            cs_sender2.send(msg).expect("send failed");
        }
    });

    // Run Execution Worker
    ew_state1.run(
        metrics1,
        args.execute,
        sw_receiver1,
        ew_sender1
    ).await;

    // Wait for workers to terminate
    sw_handler.await.expect("sw failed");
    channel_splitter.await.expect("splitter failed");

    programmable_transactions::context::LOOKUP_TABLE.write().unwrap().clear();

    // ==== Measure Execution Worker (w/o SW) ====
    // This uses the results from the exeuction above (SW + EW) stored in an mpsc channel.
    // Run 2nd Execution Worker

    let (fw_sender, fw_receiver) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
    let (ew_sender, mut ew_receiver) = mpsc::channel(DEFAULT_CHANNEL_SIZE);
    
    let forwarder = tokio::spawn(async move {
        // start epoch 0 manually
        let msg = sw_receiver2.recv().await.expect("EpochStart for epoch 0 should exist");
        fw_sender.send(msg).await.unwrap();

        while let Some(msg) = sw_receiver2.recv().await {
            if let &SailfishMessage::EpochStart { .. } = &msg {
                if let SailfishMessage::EpochEnd { .. } = ew_receiver.recv().await.unwrap() {
                    // do nothing
                } else {
                    panic!("unexpected msg");
                }
            }
            fw_sender.send(msg).await.unwrap();
        }
    });

    ew_state2.run(
        metrics2,
        args.execute,
        fw_receiver,
        ew_sender
    ).await;
    forwarder.await.unwrap();
}
