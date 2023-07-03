use clap::*;
use std::path::PathBuf;
use std::sync::Arc;
use sui_adapter::adapter;
use sui_config::{Config, NodeConfig};
use sui_core::authority::epoch_start_configuration::EpochStartConfiguration;
use sui_distributed_execution::EpochEndMessage;
use sui_distributed_execution::ExecutionWorkerState;
use sui_distributed_execution::SequenceWorkerState;
use sui_move_natives;
use sui_protocol_config::ProtocolConfig;
use sui_types::epoch_data::EpochData;
use sui_types::messages::TransactionDataAPI;
use sui_types::messages::TransactionKind;
use sui_types::multiaddr::Multiaddr;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use sui_types::sui_system_state::get_sui_system_state;
use sui_types::sui_system_state::SuiSystemStateTrait;
use tokio::signal;
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

    /// Specifies whether I will execute or not
    #[clap(long)]
    execute: bool,

    #[clap(long, help = "Specify address to listen on")]
    listen_address: Option<Multiaddr>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let config = NodeConfig::load(&args.config_path).unwrap();
    let genesis = Arc::new(config.genesis().expect("Could not load genesis"));
    let mut sw_state = SequenceWorkerState::new(&config).await;
    let metrics_clone = sw_state.metrics.clone();
    let mut ew_state = ExecutionWorkerState::new();
    ew_state.init_store(&genesis);

    let (epoch_start_sender, mut epoch_start_receiver) = mpsc::channel(32);
    let (tx_sender, mut tx_receiver) = mpsc::channel(1000);
    let (epoch_end_sender, mut epoch_end_receiver) = mpsc::channel(32);

    // Sequence Worker
    tokio::spawn(async move {
        let config = NodeConfig::load(&args.config_path).unwrap();
        let genesis = Arc::new(config.genesis().expect("Could not load genesis"));
        let genesis_seq = genesis.checkpoint().into_summary_and_sequence().0;

        let (highest_synced_seq, highest_executed_seq) = sw_state.get_watermarks();
        println!("Highest synced {}", highest_synced_seq);
        println!("Highest executed {}", highest_executed_seq);

        let protocol_config = sw_state.epoch_store.protocol_config();
        let epoch_start_config = sw_state.epoch_store.epoch_start_config();
        let reference_gas_price = sw_state.epoch_store.reference_gas_price();

        // Epoch Start
        epoch_start_sender
            .send(sui_distributed_execution::EpochStartMessage(
                protocol_config.clone(),
                epoch_start_config.epoch_data(),
                reference_gas_price,
            ))
            .await
            .expect("Sending doesn't work");

        for checkpoint_seq in genesis_seq..highest_synced_seq {
            let checkpoint_summary = sw_state
                .checkpoint_store
                .get_checkpoint_by_sequence_number(checkpoint_seq)
                .expect("Cannot get checkpoint")
                .expect("Checkpoint is None");

            if checkpoint_seq % 10000 == 0 {
                println!("Sending checkpoint {}", checkpoint_seq);
            }

            let (_seq, summary) = checkpoint_summary.into_summary_and_sequence();
            let contents = sw_state
                .checkpoint_store
                .get_checkpoint_contents(&summary.content_digest)
                .expect("Contents must exist")
                .expect("Contents must exist");

            if contents.size() > 1 {
                println!(
                    "Checkpoint {} has {} transactions",
                    checkpoint_seq,
                    contents.size()
                );
            }

            for tx_digest in contents.iter() {
                let tx = sw_state
                    .store
                    .get_transaction_block(&tx_digest.transaction)
                    .expect("Transaction exists")
                    .expect("Transaction exists");

                let tx_effects = sw_state
                    .store
                    .get_effects(&tx_digest.effects)
                    .expect("Transaction effects exist")
                    .expect("Transaction effects exist");

                tx_sender
                    .send(sui_distributed_execution::TransactionMessage(
                        tx.clone(),
                        tx_effects.clone(),
                        checkpoint_seq,
                    ))
                    .await
                    .expect("Sending doesn't work");

                if let TransactionKind::ChangeEpoch(_) = tx.data().transaction_data().kind() {
                    // wait for epoch end message from execution worker
                    println!(
                        "Waiting for epoch end message. Checkpoint_seq: {}",
                        checkpoint_seq
                    );

                    let EpochEndMessage(new_epoch_start_state) = epoch_end_receiver
                        .recv()
                        .await
                        .expect("Receiving doesn't work");
                    let next_epoch_committee = new_epoch_start_state.get_sui_committee();
                    let next_epoch = next_epoch_committee.epoch();
                    let last_checkpoint = sw_state
                        .checkpoint_store
                        .get_epoch_last_checkpoint(sw_state.epoch_store.epoch())
                        .expect("Error loading last checkpoint for current epoch")
                        .expect("Could not load last checkpoint for current epoch");
                    println!(
                        "Last checkpoint sequence number: {}",
                        last_checkpoint.sequence_number(),
                    );
                    let epoch_start_configuration = EpochStartConfiguration::new(
                        new_epoch_start_state,
                        *last_checkpoint.digest(),
                    );
                    assert_eq!(sw_state.epoch_store.epoch() + 1, next_epoch);
                    sw_state.epoch_store = sw_state.epoch_store.new_at_next_epoch(
                        config.protocol_public_key(),
                        next_epoch_committee,
                        epoch_start_configuration,
                        sw_state.store.clone(),
                        &config.expensive_safety_check_config,
                    );
                    println!("New epoch store has epoch {}", sw_state.epoch_store.epoch());
                    let protocol_config = sw_state.epoch_store.protocol_config();
                    let epoch_start_config = sw_state.epoch_store.epoch_start_config();
                    let reference_gas_price = sw_state.epoch_store.reference_gas_price();
                    epoch_start_sender
                        .send(sui_distributed_execution::EpochStartMessage(
                            protocol_config.clone(),
                            epoch_start_config.epoch_data(),
                            reference_gas_price,
                        ))
                        .await
                        .expect("Sending doesn't work");
                }
            }
        }
        println!("Sequence worker finished");
    });

    // Execution Worker
    tokio::spawn(async move {
        let mut epoch_data: EpochData;
        let mut protocol_config: ProtocolConfig;
        let mut reference_gas_price: u64;
        // Wait for epoch start message
        let sui_distributed_execution::EpochStartMessage(
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
        while let Some(sui_distributed_execution::TransactionMessage(
            tx,
            tx_effects,
            checkpoint_seq,
        )) = tx_receiver.recv().await
        {
            ew_state
                .execute_tx(
                    &tx,
                    &tx_effects,
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
                    .send(sui_distributed_execution::EpochEndMessage(
                        new_epoch_start_state,
                    ))
                    .await
                    .expect("Sending doesn't work");

                // Then wait for start epoch message from sequence worker and update local state
                let sui_distributed_execution::EpochStartMessage(
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
        }

        // print TPS
        let elapsed = now.elapsed();
        println!(
            "Execution worker TPS: {}",
            1000.0 * num_tx as f64 / elapsed.as_millis() as f64
        );
        println!("Execution worker finished");
    });

    // wait for SIGINT on the main thread
    match signal::ctrl_c().await {
        Ok(()) => {}
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {}", err);
            // we also shut down in case of error
        }
    }
}
