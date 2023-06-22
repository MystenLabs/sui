use sui_config::NodeConfig;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::ExecutionDigests;
use sui_types::epoch_data::EpochData;
use sui_types::messages::VerifiedTransaction;

use prometheus::Registry;
use std::sync::Arc;
use sui_core::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use sui_core::authority::epoch_start_configuration::EpochStartConfiguration;
use sui_core::authority::AuthorityStore;
use sui_core::checkpoints::CheckpointStore;
use sui_core::epoch::committee_store::CommitteeStore;
use sui_core::epoch::epoch_metrics::EpochMetrics;
use sui_core::module_cache_metrics::ResolverMetrics;
use sui_core::signature_verifier::SignatureVerifierMetrics;
use sui_core::storage::RocksDbStore;
use sui_node::metrics;
use sui_types::metrics::LimitsMetrics;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemState;
use sui_types::sui_system_state::SuiSystemStateTrait;
use tokio::sync::watch;
use tokio::time::Duration;
use typed_store::rocks::default_db_options;

#[derive(Debug)]
pub struct EpochStartMessage(pub ProtocolConfig, pub EpochData, pub u64);
#[derive(Debug)]
pub struct EpochEndMessage(pub EpochStartSystemState);
#[derive(Debug)]
pub struct TransactionMessage(pub VerifiedTransaction, pub ExecutionDigests, pub u64);

pub struct SequenceWorkerState {
    // config: NodeConfig,
    // genesis: &Genesis,
    // registry_service: RegistryService,
    // prometheus_registry: Registry,
    // metrics: Arc<LimitsMetrics>,
    // checkpoint_store: Arc<CheckpointStore>,
    pub store: Arc<AuthorityStore>,
    pub epoch_store: Arc<AuthorityPerEpochStore>,
    pub checkpoint_store: Arc<CheckpointStore>,
    pub committee_store: Arc<CommitteeStore>,
    pub prometheus_registry: Registry,
    pub metrics: Arc<LimitsMetrics>,
}

impl SequenceWorkerState {
    pub async fn new(config: &NodeConfig) -> Self {
        let genesis = config.genesis().expect("Could not load genesis");
        let registry_service = { metrics::start_prometheus_server(config.metrics_address) };
        let prometheus_registry = registry_service.default_registry();
        let metrics = Arc::new(LimitsMetrics::new(&prometheus_registry));
        let checkpoint_store = CheckpointStore::new(&config.db_path().join("checkpoints"));
        let genesis_committee = genesis.committee().expect("Could not get committee");
        // committee store
        let committee_store = Arc::new(CommitteeStore::new(
            config.db_path().join("epochs"),
            &genesis_committee,
            None,
        ));
        let perpetual_options = default_db_options().optimize_db_for_write_throughput(4);
        let store = AuthorityStore::open(
            &config.db_path().join("store"),
            Some(perpetual_options.options),
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
        let epoch_start_configuration = {
            let epoch_start_configuration = EpochStartConfiguration::new(
                genesis.sui_system_object().into_epoch_start_state(),
                *genesis.checkpoint().digest(),
            );
            store
                .set_epoch_start_configuration(&epoch_start_configuration)
                .await
                .expect("Could not set epoch start configuration");
            epoch_start_configuration
        };
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
        checkpoint_store.insert_genesis_checkpoint(
            genesis.checkpoint(),
            genesis.checkpoint_contents().clone(),
            &epoch_store,
        );
        Self {
            store,
            epoch_store,
            checkpoint_store,
            committee_store,
            prometheus_registry,
            metrics,
        }
    }

    pub async fn handle_download(&self, watermark: u64, config: &NodeConfig) {
        let mut highest_synced_checkpoint_seq = 0;
        if let Some(highest) = self
            .checkpoint_store
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
                self.store.clone(),
                self.committee_store.clone(),
                self.checkpoint_store.clone(),
            );
            let (_trusted_peer_change_tx, trusted_peer_change_rx) =
                watch::channel(Default::default());
            let (_p2p_network, _discovery_handle, _state_sync_handle) =
                sui_node::SuiNode::create_p2p_network(
                    &config,
                    state_sync_store,
                    trusted_peer_change_rx,
                    &self.prometheus_registry,
                )
                .expect("could not create p2p network");

            while watermark > highest_synced_checkpoint_seq {
                tokio::time::sleep(Duration::from_secs(1)).await;
                highest_synced_checkpoint_seq = self
                    .checkpoint_store
                    .get_highest_synced_checkpoint_seq_number()
                    .expect("Could not get highest checkpoint")
                    .expect("Could not get highest checkpoint");
            }
            println!("Done downloading");
        }
    }

    pub fn get_watermarks(&self) -> (u64, u64) {
        let highest_synced_seq = match self
            .checkpoint_store
            .get_highest_synced_checkpoint_seq_number()
            .expect("error")
        {
            Some(highest) => highest,
            None => 0,
        };
        let highest_executed_seq = match self
            .checkpoint_store
            .get_highest_executed_checkpoint_seq_number()
            .expect("error")
        {
            Some(highest) => highest,
            None => 0,
        };
        (highest_synced_seq, highest_executed_seq)
    }
}