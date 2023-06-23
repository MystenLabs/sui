use clap::*;
use std::path::PathBuf;
use std::sync::Arc;
use sui_adapter::adapter;
use sui_config::{Config, NodeConfig};
use sui_distributed_execution::seqn_worker;
use sui_distributed_execution::exec_worker;
use sui_distributed_execution::types::*;
use sui_move_natives;
use sui_protocol_config::ProtocolConfig;
use sui_types::epoch_data::EpochData;
use sui_types::messages::TransactionDataAPI;
use sui_types::messages::TransactionKind;
use sui_types::multiaddr::Multiaddr;
use sui_types::sui_system_state::get_sui_system_state;
use sui_types::sui_system_state::SuiSystemStateTrait;
use tokio::sync::mpsc;
use tokio::time::Instant;

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

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
#[clap(name = env!("CARGO_BIN_NAME"))]
#[clap(version = VERSION)]
struct Args {
    #[clap(long)]
    pub config_path: PathBuf,

    /// Specifies the watermark up to which I will download checkpoints
    #[clap(long)]
    download: Option<u64>,

    /// Specifies the watermark up to which I will execute checkpoints
    #[clap(long)]
    execute: Option<u64>,

    #[clap(long, help = "Specify address to listen on")]
    listen_address: Option<Multiaddr>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let config = NodeConfig::load(&args.config_path).unwrap();
    let genesis = Arc::new(config.genesis().expect("Could not load genesis"));
    let mut sw_state = seqn_worker::SequenceWorkerState::new(&config).await;
    let metrics_clone = sw_state.metrics.clone();
    let mut ew_state = exec_worker::ExecutionWorkerState::new();
    ew_state.init_store(&genesis);

    let (epoch_start_sender, mut epoch_start_receiver) = mpsc::channel(32);
    let (tx_sender, mut tx_receiver) = mpsc::channel(1000);
    let (epoch_end_sender, epoch_end_receiver) = mpsc::channel(32);

    // Run Sequence Worker
    let sw_handler = tokio::spawn(async move {
        sw_state.run(
            config.clone(), 
            args.download, 
            args.execute,
            epoch_start_sender, 
            tx_sender, 
            epoch_end_receiver
        ).await;
    });

    let mut ew_handler_opt = None;
    if let Some(watermark) = args.execute {
        // Execution Worker
        ew_handler_opt = Some(tokio::spawn(async move {
            let mut epoch_data: EpochData;
            let mut protocol_config: ProtocolConfig;
            let mut reference_gas_price: u64;
            // Wait for epoch start message
            let EpochStartMessage(
                protocol_config_,
                epoch_data_,
                reference_gas_price_,
            ) = epoch_start_receiver.recv().await.unwrap();
            println!("Got epoch start message");

            protocol_config = protocol_config_;
            epoch_data = epoch_data_;
            reference_gas_price = reference_gas_price_;

            let native_functions = sui_move_natives::all_natives(/* silent */ true);
            let mut move_vm = Arc::new(
                adapter::new_move_vm(native_functions.clone(), &protocol_config, false)
                    .expect("We defined natives to not fail here"),
            );

            // start timer for TPS computation
            let now = Instant::now();
            let mut num_tx: usize = 0;
            // receive txs
            while let Some(TransactionMessage(
                tx,
                tx_digest,
                checkpoint_seq,
            )) = tx_receiver.recv().await
            {
                ew_state
                    .execute_tx(
                        &tx,
                        &tx_digest,
                        checkpoint_seq,
                        &protocol_config,
                        &move_vm,
                        &epoch_data,
                        reference_gas_price,
                        metrics_clone.clone(),
                    )
                    .await;

                num_tx += 1;
                if checkpoint_seq % 10000 == 0 {
                    println!("Executed {}", checkpoint_seq);
                }

                if let TransactionKind::ChangeEpoch(_) = tx.data().transaction_data().kind() {
                    // First send end of epoch message to sequence worker
                    println!("END OF EPOCH at checkpoint {}", checkpoint_seq);
                    let latest_state = get_sui_system_state(&&ew_state.memory_store)
                        .expect("Read Sui System State object cannot fail");
                    let new_epoch_start_state = latest_state.into_epoch_start_state();
                    epoch_end_sender
                        .send(EpochEndMessage(
                            new_epoch_start_state,
                        ))
                        .await
                        .expect("Sending doesn't work");

                    // Then wait for start epoch message from sequence worker and update local state
                    let EpochStartMessage(
                        protocol_config_,
                        epoch_data_,
                        reference_gas_price_,
                    ) = epoch_start_receiver.recv().await.unwrap();
                    move_vm = Arc::new(
                        adapter::new_move_vm(native_functions.clone(), &protocol_config, false)
                            .expect("We defined natives to not fail here"),
                    );
                    protocol_config = protocol_config_;
                    epoch_data = epoch_data_;
                    reference_gas_price = reference_gas_price_;
                }

                // Stop executing when I hit the watermark
                if checkpoint_seq == watermark-1 {
                    break;
                }
            }

            // print TPS
            let elapsed = now.elapsed();
            println!(
                "Execution worker TPS: {}",
                1000.0 * num_tx as f64 / elapsed.as_millis() as f64
            );
            println!("Execution worker finished");
        }));
    }

    sw_handler.await.expect("sw failed");
    if let Some(ew_handler) = ew_handler_opt {
        ew_handler.await.expect("ew failed");
    }
}
