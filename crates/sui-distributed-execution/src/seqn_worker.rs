use std::cmp;
use std::sync::Arc;

use prometheus::Registry;
use std::collections::HashMap;
use std::time::Instant;
use sui_archival::reader::ArchiveReaderBalancer;
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
use sui_node::metrics;
use sui_single_node_benchmark::benchmark_context::BenchmarkContext;
use sui_single_node_benchmark::command::{Component, WorkloadKind};
use sui_single_node_benchmark::workload::Workload;
use sui_types::digests::ChainIdentifier;
use sui_types::metrics::LimitsMetrics;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::transaction::{TransactionDataAPI, TransactionKind};
use tokio::time::{sleep, Duration};
use tokio::{
    sync::{mpsc, watch},
    time::MissedTickBehavior,
};
use typed_store::rocks::default_db_options;

use crate::metrics::Metrics;

use super::types::*;

pub const WORKLOAD: WorkloadKind = WorkloadKind::NoMove;
pub const COMPONENT: Component = Component::PipeTxsToChannel;

pub struct SequenceWorkerState {
    pub config: NodeConfig,
    pub store: Arc<AuthorityStore>,
    pub epoch_store: Arc<AuthorityPerEpochStore>,
    pub checkpoint_store: Arc<CheckpointStore>,
    pub committee_store: Arc<CommitteeStore>,
    pub prometheus_registry: Registry,
    pub metrics: Arc<LimitsMetrics>,
    pub download: Option<u64>,
    pub execute: Option<u64>,
}

impl SequenceWorkerState {
    pub async fn new(_id: UniqueId, attrs: &HashMap<String, String>) -> Self {
        let config_path = attrs.get("config").unwrap();
        let config = NodeConfig::load(config_path).unwrap();

        let genesis = config.genesis().expect("Could not load genesis");
        let metrics_address = attrs.get("metrics-address").unwrap().parse().unwrap();
        let registry_service = { metrics::start_prometheus_server(metrics_address) };
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
        let perpetual_tables = Arc::new(AuthorityPerpetualTables::open(
            &config.db_path().join("store"),
            Some(perpetual_options.options),
        ));
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
            ChainIdentifier::from(*genesis.checkpoint().digest()),
        );
        checkpoint_store.insert_genesis_checkpoint(
            genesis.checkpoint(),
            genesis.checkpoint_contents().clone(),
            &epoch_store,
        );
        let download = match attrs.get("download") {
            Some(watermark) => Some(watermark.parse().unwrap()),
            None => None,
        };
        let execute = match attrs.get("execute") {
            Some(watermark) => Some(watermark.parse().unwrap()),
            None => None,
        };
        Self {
            config,
            store,
            epoch_store,
            checkpoint_store,
            committee_store,
            prometheus_registry,
            metrics,
            download,
            execute,
        }
    }

    pub async fn new_from_config(config: &NodeConfig) -> Self {
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
        let perpetual_tables = Arc::new(AuthorityPerpetualTables::open(
            &config.db_path().join("store"),
            Some(perpetual_options.options),
        ));
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
            ChainIdentifier::from(*genesis.checkpoint().digest()),
        );
        checkpoint_store.insert_genesis_checkpoint(
            genesis.checkpoint(),
            genesis.checkpoint_contents().clone(),
            &epoch_store,
        );
        Self {
            config: config.clone(),
            store,
            epoch_store,
            checkpoint_store,
            committee_store,
            prometheus_registry,
            metrics,
            download: None,
            execute: None,
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
            let genesis = config.genesis().expect("Could not load genesis");
            let chain_identifier = ChainIdentifier::from(*genesis.checkpoint().digest());
            let archive_readers = ArchiveReaderBalancer::new(
                config.archive_reader_config(),
                &self.prometheus_registry,
            )
            .expect("Can't construct archive reader");
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
                    chain_identifier,
                    trusted_peer_change_rx,
                    archive_readers.clone(),
                    &self.prometheus_registry,
                )
                .expect("could not create p2p network");

            let mut old_highest = highest_synced_checkpoint_seq;
            while watermark > highest_synced_checkpoint_seq {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let new_highest = self
                    .checkpoint_store
                    .get_highest_synced_checkpoint_seq_number()
                    .expect("Could not get highest checkpoint")
                    .expect("Could not get highest checkpoint");
                if (new_highest - old_highest) > 10000 {
                    println!("Downloaded up to checkpoint {}", new_highest);
                    old_highest = new_highest;
                }
                highest_synced_checkpoint_seq = new_highest;
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

    pub async fn run(
        &mut self,
        in_channel: &mut mpsc::Receiver<NetworkMessage>,
        out_channel: &mpsc::Sender<NetworkMessage>,
        ew_ids: Vec<UniqueId>,
    ) {
        let genesis = Arc::new(self.config.genesis().expect("Could not load genesis"));
        let genesis_seq = genesis.checkpoint().into_summary_and_sequence().0;

        let (highest_synced_seq, highest_executed_seq) = self.get_watermarks();
        println!("Highest synced {}", highest_synced_seq);
        println!("Highest executed {}", highest_executed_seq);

        let protocol_config = self.epoch_store.protocol_config();
        let epoch_start_config = self.epoch_store.epoch_start_config();
        let reference_gas_price = self.epoch_store.reference_gas_price();

        // Download txs
        if let Some(watermark) = self.download {
            println!("SW downloading up to {}", watermark);
            self.handle_download(watermark, &self.config).await;
        }

        // init stats and timer for per-epoch TPS computation
        let mut num_tx: usize = 0;
        let mut now = Instant::now();

        // Epoch Start
        for ew_id in &ew_ids {
            println!("SW sending epoch start to {}", ew_id);
            out_channel
                .send(NetworkMessage {
                    src: 0,
                    dst: *ew_id,
                    payload: SailfishMessage::EpochStart {
                        version: protocol_config.version,
                        data: epoch_start_config.epoch_data(),
                        ref_gas_price: reference_gas_price,
                    },
                })
                .await
                .expect("Sending doesn't work");
        }

        if let Some(watermark) = self.execute {
            for checkpoint_seq in genesis_seq..cmp::min(watermark, highest_synced_seq) {
                let checkpoint_summary = self
                    .checkpoint_store
                    .get_checkpoint_by_sequence_number(checkpoint_seq)
                    .expect("Cannot get checkpoint")
                    .expect("Checkpoint is None");

                if checkpoint_seq % 10_000 == 0 {
                    println!("SW sending checkpoint {}", checkpoint_seq);
                }

                let (_seq, summary) = checkpoint_summary.into_summary_and_sequence();
                let contents = self
                    .checkpoint_store
                    .get_checkpoint_contents(&summary.content_digest)
                    .expect("Contents must exist")
                    .expect("Contents must exist");

                num_tx += contents.size();

                for tx_digest in contents.iter() {
                    let tx = self
                        .store
                        .get_transaction_block(&tx_digest.transaction)
                        .expect("Transaction exists")
                        .expect("Transaction exists");

                    let tx_effects = self
                        .store
                        .get_effects(&tx_digest.effects)
                        .expect("Transaction effects exist")
                        .expect("Transaction effects exist");

                    let _ = tx.digest();
                    let full_tx = TransactionWithEffects {
                        tx: tx.data().clone(),
                        ground_truth_effects: Some(tx_effects.clone()),
                        child_inputs: None,
                        checkpoint_seq: Some(checkpoint_seq),
                        timestamp: Metrics::now().as_secs_f64(),
                    };

                    for ew_id in &ew_ids {
                        out_channel
                            .send(NetworkMessage {
                                src: 0,
                                dst: *ew_id,
                                payload: SailfishMessage::ProposeExec(full_tx.clone()),
                            })
                            .await
                            .expect("sending failed");
                    }

                    if let TransactionKind::ChangeEpoch(_) = tx.data().transaction_data().kind() {
                        // wait for epoch end message from execution worker
                        println!(
                            "SW waiting for epoch end message. Change epoch tx {} at checkpoint {}",
                            tx.digest(),
                            checkpoint_seq
                        );

                        let msg = in_channel.recv().await.expect("Receiving doesn't work");

                        let SailfishMessage::EpochEnd {new_epoch_start_state } = msg.payload
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
                            "SW last checkpoint sequence number: {}",
                            last_checkpoint.sequence_number(),
                        );
                        let epoch_start_configuration = EpochStartConfiguration::new(
                            new_epoch_start_state,
                            *last_checkpoint.digest(),
                        );
                        assert_eq!(self.epoch_store.epoch() + 1, next_epoch);
                        self.epoch_store = self.epoch_store.new_at_next_epoch(
                            self.config.protocol_public_key(),
                            next_epoch_committee,
                            epoch_start_configuration,
                            self.store.clone(),
                            &self.config.expensive_safety_check_config,
                            self.epoch_store.get_chain_identifier(),
                        );
                        println!("SW new epoch store has epoch {}", self.epoch_store.epoch());
                        let protocol_config = self.epoch_store.protocol_config();
                        let epoch_start_config = self.epoch_store.epoch_start_config();
                        let reference_gas_price = self.epoch_store.reference_gas_price();

                        // print TPS just before starting new epoch
                        let elapsed = now.elapsed();
                        println!(
                            "#epoch TPS:{},{}",
                            next_epoch - 1,
                            1000.0 * num_tx as f64 / elapsed.as_millis() as f64
                        );
                        now = Instant::now();
                        num_tx = 0;

                        // send EpochStart message to start next epoch
                        for ew_id in &ew_ids {
                            out_channel
                                .send(NetworkMessage {
                                    src: 0,
                                    dst: *ew_id,
                                    payload: SailfishMessage::EpochStart {
                                        version: protocol_config.version,
                                        data: epoch_start_config.epoch_data(),
                                        ref_gas_price: reference_gas_price,
                                    },
                                })
                                .await
                                .expect("Sending doesn't work");
                        }
                    }
                }
            }
        }
        println!("Sequence worker finished");
        sleep(Duration::from_millis(100_000)).await;
    }

    pub async fn run_with_channel(
        out_to_network: &mpsc::Sender<NetworkMessage>,
        ew_ids: Vec<UniqueId>,
        tx_count: u64,
        duration: Duration,
    ) {
        let workload = Workload::new(tx_count * duration.as_secs(), WORKLOAD);
        println!("Setting up benchmark...");
        let start_time = std::time::Instant::now();
        let mut ctx = BenchmarkContext::new(workload, COMPONENT, 0).await;
        let elapsed = start_time.elapsed().as_millis() as f64;
        println!(
            "Benchmark setup finished in {}ms at a rate of {} accounts/s",
            elapsed,
            1000f64 * workload.num_accounts() as f64 / elapsed
        );
        // first send genesis objects to all EWs
        // let genesis_objects = ctx.get_genesis_objects();
        // for ew_id in &ew_ids {
        //     println!("SW sending genesis objects to {}", ew_id);
        //     out_to_network
        //         .send(NetworkMessage {
        //             src: 0,
        //             dst: *ew_id,
        //             payload: SailfishMessage::GenesisObjects(genesis_objects.clone()),
        //         })
        //         .await
        //         .expect("sending failed");
        // }

        // then generate transactions and send them to all EWs
        let start_time = std::time::Instant::now();
        let tx_generator = workload.create_tx_generator(&mut ctx).await;
        let transactions = ctx.generate_transactions(tx_generator).await;
        let elapsed = start_time.elapsed().as_millis() as f64;
        println!(
            "Tx generation finished in {}ms at a rate of {} TPS",
            elapsed,
            1000f64 * workload.tx_count as f64 / elapsed,
        );

        const PRECISION: u64 = 20;
        let burst_duration = 1000 / PRECISION;
        let chunks_size = (tx_count / PRECISION) as usize;
        let mut counter = 0;
        let mut interval = tokio::time::interval(Duration::from_millis(burst_duration));
        interval.set_missed_tick_behavior(MissedTickBehavior::Burst);

        // Ugly - wait for EWs to finish generating genesis objects.
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Send transactions.
        println!("Starting benchmark");
        for chunk in transactions.chunks(chunks_size) {
            if counter % 1000 == 0 && counter != 0 {
                tracing::debug!("Submitted {} txs", counter * chunks_size);
            }
            let now = Metrics::now().as_secs_f64();
            for tx in chunk {
                let full_tx = TransactionWithEffects {
                    tx: tx.data().clone(),
                    ground_truth_effects: None,
                    child_inputs: None,
                    checkpoint_seq: None,
                    timestamp: now,
                };
                for ew_id in &ew_ids {
                    out_to_network
                        .send(NetworkMessage {
                            src: 0,
                            dst: *ew_id,
                            payload: SailfishMessage::ProposeExec(full_tx.clone()),
                        })
                        .await
                        .expect("sending failed");
                }
            }
            counter += 1;
            interval.tick().await;
        }
        println!("[SW] Benchmark terminated");

        // for tx in iterator.take(BURST_SIZE) {
        //     let full_tx = TransactionWithEffects {
        //         tx: tx.data().clone(),
        //         ground_truth_effects: None,
        //         child_inputs: None,
        //         checkpoint_seq: None,
        //     };
        //     for ew_id in &ew_ids {
        //         out_to_network
        //             .send(NetworkMessage {
        //                 src: 0,
        //                 dst: *ew_id,
        //                 payload: SailfishMessage::ProposeExec(full_tx.clone()),
        //             })
        //             .await
        //             .expect("sending failed");
        //     }
        // }
    }
}
