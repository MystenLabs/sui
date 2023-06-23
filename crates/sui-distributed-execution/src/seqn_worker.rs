use std::sync::Arc;
use std::cmp;

use sui_config::NodeConfig;
use prometheus::Registry;
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
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::messages::{TransactionDataAPI, TransactionKind};
use tokio::sync::{watch, mpsc};
use tokio::time::Duration;
use typed_store::rocks::default_db_options;

use super::types::*;

pub struct SequenceWorkerState {
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

    pub async fn run(&mut self, 
        config: NodeConfig, 
        download: Option<u64>, 
        exeucte: Option<u64>,
        sw_sender: mpsc::Sender<SailfishMessage>,
        mut ew_receiver: mpsc::Receiver<SailfishMessage>,
    ){
        let genesis = Arc::new(config.genesis().expect("Could not load genesis"));
        let genesis_seq = genesis.checkpoint().into_summary_and_sequence().0;

        let (highest_synced_seq, highest_executed_seq) = self.get_watermarks();
        println!("Highest synced {}", highest_synced_seq);
        println!("Highest executed {}", highest_executed_seq);

        let protocol_config = self.epoch_store.protocol_config();
        let epoch_start_config = self.epoch_store.epoch_start_config();
        let reference_gas_price = self.epoch_store.reference_gas_price();

        // Epoch Start
        sw_sender
            .send(SailfishMessage::EpochStart{
                conf: protocol_config.clone(),
                data: epoch_start_config.epoch_data(),
                ref_gas_price: reference_gas_price,
            })
            .await
            .expect("Sending doesn't work");

        if let Some(watermark) = download {
            self.handle_download(watermark, &config).await;
        }
        

        if let Some(watermark) = exeucte {
            for checkpoint_seq in genesis_seq..cmp::min(watermark, highest_synced_seq) {
                let checkpoint_summary = self
                    .checkpoint_store
                    .get_checkpoint_by_sequence_number(checkpoint_seq)
                    .expect("Cannot get checkpoint")
                    .expect("Checkpoint is None");

                if checkpoint_seq % 10000 == 0 {
                    println!("Sending checkpoint {}", checkpoint_seq);
                }

                let (_seq, summary) = checkpoint_summary.into_summary_and_sequence();
                let contents = self
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
                    let tx = self
                        .store
                        .get_transaction_block(&tx_digest.transaction)
                        .expect("Transaction exists")
                        .expect("Transaction exists");

                    sw_sender
                        .send(SailfishMessage::Transaction{
                            tx: tx.clone(),
                            digest: tx_digest.clone(),
                            checkpoint_seq,
                        })
                        .await
                        .expect("Sending doesn't work");

                    if let TransactionKind::ChangeEpoch(_) = tx.data().transaction_data().kind() {
                        // wait for epoch end message from execution worker
                        println!(
                            "Waiting for epoch end message. Checkpoint_seq: {}",
                            checkpoint_seq
                        );

                        let SailfishMessage::EpochEnd{new_epoch_start_state} = ew_receiver
                            .recv()
                            .await
                            .expect("Receiving doesn't work")
                        else {
                            panic!("unexpected message")
                        };
                        let next_epoch_committee = new_epoch_start_state.get_sui_committee();
                        let next_epoch = next_epoch_committee.epoch();
                        let last_checkpoint = self
                            .checkpoint_store
                            .get_epoch_last_checkpoint(self.epoch_store.epoch())
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
                        assert_eq!(self.epoch_store.epoch() + 1, next_epoch);
                        self.epoch_store = self.epoch_store.new_at_next_epoch(
                            config.protocol_public_key(),
                            next_epoch_committee,
                            epoch_start_configuration,
                            self.store.clone(),
                            &config.expensive_safety_check_config,
                        );
                        println!("New epoch store has epoch {}", self.epoch_store.epoch());
                        let protocol_config = self.epoch_store.protocol_config();
                        let epoch_start_config = self.epoch_store.epoch_start_config();
                        let reference_gas_price = self.epoch_store.reference_gas_price();
                        sw_sender
                            .send(SailfishMessage::EpochStart{
                                conf: protocol_config.clone(),
                                data: epoch_start_config.epoch_data(),
                                ref_gas_price: reference_gas_price,
                            })
                            .await
                            .expect("Sending doesn't work");
                    }
                }
            }
        }
        println!("Sequence worker finished");
    }
}