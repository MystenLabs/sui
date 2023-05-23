use crate::syncoexec::MemoryBackedStore;
use clap::Parser;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use sui_adapter::execution_engine;
use sui_adapter::execution_mode;
use sui_config::{Config, NodeConfig};
use sui_core::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use sui_core::authority::authority_store_tables::AuthorityPerpetualTables;
use sui_core::authority::epoch_start_configuration::EpochStartConfiguration;
use sui_core::authority::AuthorityStore;
use sui_core::checkpoints::CheckpointStore;
use sui_core::epoch::committee_store::CommitteeStore;
use sui_core::epoch::epoch_metrics::EpochMetrics;
use sui_core::module_cache_metrics::ResolverMetrics;
use sui_core::signature_verifier::SignatureVerifierMetrics;
use sui_core::storage::RocksDbStore;
use sui_core::transaction_input_checker::get_gas_status;
use sui_node;
use sui_node::metrics;
use sui_types::message_envelope::Message;
use sui_types::metrics::LimitsMetrics;
use sui_types::multiaddr::Multiaddr;
use sui_types::software_version::VERSION;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use sui_types::sui_system_state::get_sui_system_state;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::temporary_store::TemporaryStore;
use sui_types::transaction::InputObjectKind;
use sui_types::transaction::InputObjects;
use sui_types::transaction::TransactionDataAPI;
use tokio::sync::watch;
use tokio::time::Duration;
use typed_store::rocks::default_db_options;

pub mod syncoexec;

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
    let genesis = config.genesis().expect("Could not load genesis");
    let registry_service = { metrics::start_prometheus_server(config.metrics_address) };
    let prometheus_registry = registry_service.default_registry();
    let metrics = Arc::new(LimitsMetrics::new(&prometheus_registry));
    let checkpoint_store = CheckpointStore::new(&config.db_path().join("checkpoints"));

    // stores
    let genesis_committee = genesis.committee().expect("Could not get committee");
    // committee store
    let committee_store = Arc::new(CommitteeStore::new(
        config.db_path().join("epochs"),
        &genesis_committee,
        None,
    ));
    // checkpoint store
    // authority store
    let perpetual_options = default_db_options().optimize_db_for_write_throughput(4);
    let perpetual_tables = Arc::new(AuthorityPerpetualTables::open(
        &config.db_path().join("store"),
        Some(perpetual_options.options),
    ));
    let epoch_start_configuration = {
        let epoch_start_configuration = EpochStartConfiguration::new(
            genesis.sui_system_object().into_epoch_start_state(),
            *genesis.checkpoint().digest(),
        );
        perpetual_tables
            .set_epoch_start_configuration(&epoch_start_configuration)
            .await
            .expect("Could not set epoch start configuration");
        epoch_start_configuration
    };
    let store = AuthorityStore::open(
        perpetual_tables,
        genesis,
        &committee_store,
        config.indirect_objects_threshold,
        config
            .expensive_safety_check_config
            .enable_epoch_sui_conservation_check(),
        &prometheus_registry,
    )
    .await
    .expect("Could not create AuthorityStore");
    // epoch store
    let cur_epoch = 0; // always start from epoch 0
    let committee = committee_store
        .get_committee(&cur_epoch)
        .expect("Could not get committee")
        .expect("Committee of the current epoch must exist");
    let cache_metrics = Arc::new(ResolverMetrics::new(&prometheus_registry));
    let signature_verifier_metrics = SignatureVerifierMetrics::new(&prometheus_registry);
    let epoch_options = default_db_options().optimize_db_for_write_throughput(4);
    let mut epoch_store = AuthorityPerEpochStore::new(
        config.protocol_public_key(),
        committee.clone(),
        &config.db_path().join("store"),
        Some(epoch_options.options),
        EpochMetrics::new(&registry_service.default_registry()),
        epoch_start_configuration,
        store.clone(),
        cache_metrics,
        signature_verifier_metrics,
        &config.expensive_safety_check_config,
    );
    checkpoint_store.insert_genesis_checkpoint(
        genesis.checkpoint(),
        genesis.checkpoint_contents().clone(),
        &epoch_store,
    );

    if let Some(watermark) = args.download {
        let mut highest_synced_checkpoint_seq = 0;
        if let Some(highest) = checkpoint_store
            .get_highest_synced_checkpoint_seq_number()
            .expect("Could not get highest checkpoint")
        {
            highest_synced_checkpoint_seq = highest;
        }
        println!(
            "Requested watermark = {}, current highest checkpoint = {}",
            watermark, highest_synced_checkpoint_seq
        );
        if watermark > highest_synced_checkpoint_seq {
            // we have already downloaded all the checkpoints up to the watermark -> nothing to do
            let state_sync_store = RocksDbStore::new(
                store.clone(),
                committee_store.clone(),
                checkpoint_store.clone(),
            );
            let (_trusted_peer_change_tx, trusted_peer_change_rx) =
                watch::channel(Default::default());
            let (_p2p_network, _discovery_handle, _state_sync_handle) =
                sui_node::SuiNode::create_p2p_network(
                    &config,
                    state_sync_store,
                    trusted_peer_change_rx,
                    &prometheus_registry,
                )
                .expect("could not create p2p network");

            while watermark > highest_synced_checkpoint_seq {
                tokio::time::sleep(Duration::from_secs(1)).await;
                highest_synced_checkpoint_seq = checkpoint_store
                    .get_highest_synced_checkpoint_seq_number()
                    .expect("Could not get highest checkpoint")
                    .expect("Could not get highest checkpoint");
            }
            println!("Done downloading");
        }
    }

    if args.execute {
        let mut memory_store = MemoryBackedStore::new();
        for obj in genesis.objects() {
            memory_store
                .objects
                .insert(obj.id(), (obj.compute_object_reference(), obj.clone()));
        }

        let genesis_seq = genesis.checkpoint().into_summary_and_sequence().0;
        // let mut num_tx : usize = 0;
        // let mut num_tx_prev = num_tx;
        // let mut now = Instant::now();

        let highest_synced_seq = match checkpoint_store
            .get_highest_synced_checkpoint_seq_number()
            .expect("error")
        {
            Some(highest) => highest,
            None => 0,
        };
        let highest_executed_seq = match checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .expect("error")
        {
            Some(highest) => highest,
            None => 0,
        };
        println!("Highest synced {}", highest_synced_seq);
        println!("Highest executed {}", highest_executed_seq);
        // quick sanity check
        // while let Some(checkpoint_summary) = checkpoint_store
        //     .get_checkpoint_by_sequence_number(checkpoint_seq)
        //     .expect("Cannot get checkpoint")
        // {
        //     if checkpoint_seq % 50000 == 0 {
        //         println!("{}", checkpoint_seq);
        //     }
        //     checkpoint_seq += 1;

        //     let (seq, _summary) = checkpoint_summary.into_summary_and_sequence();
        //     let contents = checkpoint_store
        //         .get_checkpoint_contents(&_summary.content_digest)
        //         .expect("Contents must exist");
        //     if contents == None {
        //         println!(
        //             "Reached end of available checkpoints at seq nr {}",
        //             checkpoint_seq
        //         );
        //         return;
        //     }

        //     let full_contents = checkpoint_store
        //         .get_full_checkpoint_contents_by_sequence_number(checkpoint_seq)
        //         .expect("Cannot get full checkpoint contents");
        //     if full_contents == None {
        //         print!("reached end of full checkpoints at seq {}", checkpoint_seq);
        //         return;
        //     }
        //     for tx_digest in contents.unwrap().iter() {
        //         // println!("Digest: {:?}", tx_digest);
        //         let tx = store
        //             .get_transaction_block(&tx_digest.transaction)
        //             .expect("Transaction exists")
        //             .expect("Transaction exists");
        //     }
        // }

        let now = Instant::now();
        let mut num_tx: usize = 0;
        for checkpoint_seq in genesis_seq..highest_synced_seq {
            let checkpoint_summary = checkpoint_store
                .get_checkpoint_by_sequence_number(checkpoint_seq)
                .expect("Cannot get checkpoint")
                .expect("Checkpoint is None");

            if checkpoint_seq % 1000 == 0 {
                println!("{}", checkpoint_seq);
            }

            let (_seq, summary) = checkpoint_summary.into_summary_and_sequence();
            let contents = checkpoint_store
                .get_checkpoint_contents(&summary.content_digest)
                .expect("Contents must exist")
                .expect("Contents must exist");
            num_tx += contents.size();
            for tx_digest in contents.iter() {
                let tx = store
                    .get_transaction_block(&tx_digest.transaction)
                    .expect("Transaction exists")
                    .expect("Transaction exists");
                let tx_data = tx.data().transaction_data();
                let input_object_kinds = tx_data
                    .input_objects()
                    .expect("Cannot get input object kinds");
                // println!("Digest: {:?}", tx_digest);

                let mut input_object_data = Vec::new();
                for kind in &input_object_kinds {
                    let obj = match kind {
                        InputObjectKind::MovePackage(id)
                        | InputObjectKind::SharedMoveObject { id, .. }
                        | InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => {
                            memory_store.objects.get(&id).expect("Object missing?")
                        }
                    };
                    input_object_data.push(obj.1.clone());
                }

                let gas_status =
                    get_gas_status(&input_object_data, tx_data.gas(), &epoch_store, &tx_data)
                        .await
                        .expect("Could not get gas");

                let input_objects = InputObjects::new(
                    input_object_kinds
                        .into_iter()
                        .zip(input_object_data.into_iter())
                        .collect(),
                );
                let shared_object_refs = input_objects.filter_shared_objects();
                let transaction_dependencies = input_objects.transaction_dependencies();

                let temporary_store = TemporaryStore::new(
                    &memory_store,
                    input_objects,
                    *tx.digest(),
                    epoch_store.protocol_config(),
                );

                let (kind, signer, gas) = tx_data.execution_parts();

                let (inner_temp_store, effects, _execution_error) =
                    execution_engine::execute_transaction_to_effects::<execution_mode::Normal, _>(
                        shared_object_refs,
                        temporary_store,
                        kind,
                        signer,
                        &gas,
                        *tx.digest(),
                        transaction_dependencies,
                        epoch_store.move_vm(),
                        gas_status,
                        &epoch_store.epoch_start_config().epoch_data(),
                        epoch_store.protocol_config(),
                        metrics.clone(),
                        false,
                        &HashSet::new(),
                    );

                // Critical check: are the effects the same?
                if effects.digest() != tx_digest.effects {
                    println!("Effects mismatch at checkpoint {}", checkpoint_seq);
                    let old_effects = tx_digest.effects;
                    println!("Past effects: {:?}", old_effects);
                    println!("New effects: {:?}", effects);
                }
                assert!(
                    effects.digest() == tx_digest.effects,
                    "Effects digest mismatch"
                );

                // And now we mutate the store.
                // First delete:
                for obj_del in &inner_temp_store.deleted {
                    memory_store.objects.remove(obj_del.0);
                }
                for (obj_add_id, (oref, obj, _)) in inner_temp_store.written {
                    memory_store.objects.insert(obj_add_id, (oref, obj));
                }
            }

            if summary.end_of_epoch_data.is_some() {
                println!("END OF EPOCH at checkpoint {}", checkpoint_seq);
                let latest_state = get_sui_system_state(&&memory_store)
                    .expect("Read Sui System State object cannot fail");
                let new_epoch_start_state = latest_state.into_epoch_start_state();
                let next_epoch_committee = new_epoch_start_state.get_sui_committee();
                let next_epoch = next_epoch_committee.epoch();
                let last_checkpoint = checkpoint_store
                    .get_epoch_last_checkpoint(epoch_store.epoch())
                    .expect("Error loading last checkpoint for current epoch")
                    .expect("Could not load last checkpoint for current epoch");
                let epoch_start_configuration =
                    EpochStartConfiguration::new(new_epoch_start_state, *last_checkpoint.digest());
                assert_eq!(epoch_store.epoch() + 1, next_epoch);
                epoch_store = epoch_store.new_at_next_epoch(
                    config.protocol_public_key(),
                    next_epoch_committee,
                    epoch_start_configuration,
                    store.clone(),
                    &config.expensive_safety_check_config,
                );
                println!("New epoch store has epoch {}", epoch_store.epoch());
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
