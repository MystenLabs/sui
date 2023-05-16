use crate::syncoexec::MemoryBackedStore;
use clap::Parser;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
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
use sui_core::transaction_input_checker::get_gas_status;
use sui_node::metrics;
use sui_types::metrics::LimitsMetrics;
use sui_types::multiaddr::Multiaddr;
use sui_types::software_version::VERSION;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::temporary_store::TemporaryStore;
use sui_types::transaction::InputObjectKind;
use sui_types::transaction::InputObjects;
use sui_types::transaction::TransactionDataAPI;
use typed_store::rocks::default_db_options;

pub mod syncoexec;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
#[clap(name = env!("CARGO_BIN_NAME"))]
#[clap(version = VERSION)]
struct Args {
    #[clap(long)]
    pub config_path: PathBuf,

    #[clap(long, help = "Specify address to listen on")]
    listen_address: Option<Multiaddr>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let mut config = NodeConfig::load(&args.config_path).unwrap();
    let genesis = config.genesis().expect("Could not load genesis");
    // let runtimes = SuiRuntimes::new(&config);

    // stores
    let registry_service = { metrics::start_prometheus_server(config.metrics_address) };
    let prometheus_registry = registry_service.default_registry();
    let metrics = Arc::new(LimitsMetrics::new(&prometheus_registry));
    let genesis_committee = genesis.committee().expect("Could not get committee");
    // committee store
    let committee_store = Arc::new(CommitteeStore::new(
        config.db_path().join("epochs"),
        &genesis_committee,
        None,
    ));
    // checkpoint store
    let checkpoint_store = CheckpointStore::new(&config.db_path().join("checkpoints"));
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
    let epoch_store = AuthorityPerEpochStore::new(
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

    let mut memory_store = MemoryBackedStore::new();
    for obj in genesis.objects() {
        memory_store
            .objects
            .insert(obj.id(), (obj.compute_object_reference(), obj.clone()));
    }

    let mut checkpoint_seq = genesis.checkpoint().into_summary_and_sequence().0;
    let mut epoch = 0;
    // let mut num_tx : usize = 0;
    // let mut num_tx_prev = num_tx;
    // let mut now = Instant::now();

    while let Some(checkpoint_summary) = checkpoint_store
        .get_checkpoint_by_sequence_number(checkpoint_seq)
        .expect("Cannot get checkpoint")
    {
        println!("{}", checkpoint_seq);
        checkpoint_seq += 1;

        let (seq, _summary) = checkpoint_summary.into_summary_and_sequence();
        let contents = checkpoint_store
            .get_checkpoint_contents(&_summary.content_digest)
            .expect("Contents must exist")
            .expect("Contents must exist");
        for tx_digest in contents.iter() {
            println!("Digest: {:?}", tx_digest);
            let tx = store
                .get_transaction_block(&tx_digest.transaction)
                .expect("Transaction exists")
                .expect("Transaction exists");
            let input_object_kinds = tx
                .data()
                .intent_message()
                .value
                .input_objects()
                .expect("Cannot get input object kinds");
            let tx_data = &tx.data().intent_message().value;

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
                tx_digest.transaction,
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
                    tx_digest.transaction,
                    transaction_dependencies,
                    epoch_store.move_vm(),
                    gas_status,
                    &epoch_store.epoch_start_config().epoch_data(),
                    epoch_store.protocol_config(),
                    metrics.clone(),
                    false,
                    &HashSet::new(),
                );
        }
    }
}
