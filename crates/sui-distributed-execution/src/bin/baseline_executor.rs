use clap::*;
use std::cmp;
use std::path::PathBuf;
use std::time::Instant;
use sui_config::{Config, NodeConfig};
use sui_core::authority::epoch_start_configuration::EpochStartConfiguration;
use sui_distributed_execution::{exec_worker, seqn_worker, simple_store::MemoryBackedStore};
use sui_types::multiaddr::Multiaddr;
use sui_types::sui_system_state::{
    epoch_start_sui_system_state::EpochStartSystemStateTrait, get_sui_system_state,
    SuiSystemStateTrait,
};

use sui_distributed_execution::types::*;

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

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = Args::parse();
    let config = NodeConfig::load(&args.config_path).unwrap();
    let genesis = config.genesis().expect("Could not load genesis");
    let mut sw_state = seqn_worker::SequenceWorkerState::new_from_config(&config).await;

    if let Some(watermark) = args.download {
        sw_state.handle_download(watermark, &config).await;
    }

    if let Some(watermark) = args.execute {
        let store = MemoryBackedStore::new(); // use the simple store
        let mut ew_state = exec_worker::ExecutionWorkerState::new(store);
        ew_state.init_store(genesis);

        let mut protocol_config = sw_state.epoch_store.protocol_config();
        let mut move_vm = sw_state.epoch_store.move_vm();
        let mut epoch_start_config = sw_state.epoch_store.epoch_start_config();
        let mut reference_gas_price = sw_state.epoch_store.reference_gas_price();

        let genesis_seq = genesis.checkpoint().into_summary_and_sequence().0;

        let (highest_synced_seq, highest_executed_seq) = sw_state.get_watermarks();
        println!("Highest synced {}", highest_synced_seq);
        println!("Highest executed {}", highest_executed_seq);

        let mut num_tx: usize = 0;
        let now = Instant::now();
        for checkpoint_seq in genesis_seq..cmp::min(watermark, highest_synced_seq) {
            let checkpoint_summary = sw_state
                .checkpoint_store
                .get_checkpoint_by_sequence_number(checkpoint_seq)
                .expect("Cannot get checkpoint")
                .expect("Checkpoint is None");

            let (_seq, summary) = checkpoint_summary.into_summary_and_sequence();
            let contents = sw_state
                .checkpoint_store
                .get_checkpoint_contents(&summary.content_digest)
                .expect("Contents must exist")
                .expect("Contents must exist");
            num_tx += contents.size();
            for tx_digest in contents.iter() {
                let tx = sw_state
                    .store
                    .get_transaction_block(&tx_digest.transaction)
                    .expect("Transaction exists")
                    .expect("Transaction exists");

                let ground_truth_effects = sw_state
                    .store
                    .get_effects(&tx_digest.effects)
                    .expect("Transaction effects exist")
                    .expect("Transaction effects exist");

                let full_tx = Transaction {
                    tx,
                    ground_truth_effects,
                    checkpoint_seq,
                };
                ew_state
                    .execute_tx(
                        &full_tx,
                        &protocol_config,
                        &move_vm,
                        &epoch_start_config.epoch_data(),
                        reference_gas_price,
                        sw_state.metrics.clone(),
                    )
                    .await;

                if checkpoint_seq % 10000 == 0 {
                    println!("Executed {}", checkpoint_seq);
                }
            }

            if summary.end_of_epoch_data.is_some() {
                println!("END OF EPOCH at checkpoint {}", checkpoint_seq);
                let latest_state = get_sui_system_state(&ew_state.memory_store.clone())
                    .expect("Read Sui System State object cannot fail");
                let new_epoch_start_state = latest_state.into_epoch_start_state();
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
                let epoch_start_configuration =
                    EpochStartConfiguration::new(new_epoch_start_state, *last_checkpoint.digest());
                assert_eq!(sw_state.epoch_store.epoch() + 1, next_epoch);
                sw_state.epoch_store = sw_state.epoch_store.new_at_next_epoch(
                    config.protocol_public_key(),
                    next_epoch_committee,
                    epoch_start_configuration,
                    sw_state.store.clone(),
                    &config.expensive_safety_check_config,
                    sw_state.epoch_store.get_chain_identifier(),
                );
                println!("New epoch store has epoch {}", sw_state.epoch_store.epoch());
                protocol_config = sw_state.epoch_store.protocol_config();
                move_vm = sw_state.epoch_store.move_vm();
                epoch_start_config = sw_state.epoch_store.epoch_start_config();
                reference_gas_price = sw_state.epoch_store.reference_gas_price();
            }
        } // for loop over checkpoints

        // print TPS
        let elapsed = now.elapsed();
        println!(
            "TPS: {}",
            1000.0 * num_tx as f64 / elapsed.as_millis() as f64
        );
    } // if args.execute
}
