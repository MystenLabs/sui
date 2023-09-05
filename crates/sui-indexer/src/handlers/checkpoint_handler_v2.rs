// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use itertools::Itertools;
use move_bytecode_utils::module_cache::GetModule;
use crate::handlers::tx_processor::InMemPackageCache;
use mysten_metrics::{get_metrics, spawn_monitored_task};
use sui_rest_api::CheckpointData;
use sui_rest_api::CheckpointTransaction;
use sui_types::base_types::ObjectRef;
use sui_types::dynamic_field::DynamicFieldInfo;
use sui_types::dynamic_field::DynamicFieldName;
use sui_types::object::ObjectFormatOptions;
use tokio::sync::watch;
use tracing::debug;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::object::Object;

use std::collections::hash_map::Entry;
use std::collections::HashSet;
use sui_json_rpc_types::EndOfEpochInfo;
use sui_json_rpc_types::SuiMoveValue;
use sui_types::base_types::SequenceNumber;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::event::SystemEpochInfoEvent;
use sui_types::object::Owner;
use sui_types::transaction::TransactionDataAPI;
use tap::tap::TapFallible;
use tracing::{error, info, warn};

use sui_types::base_types::ObjectID;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
use sui_types::sui_system_state::{get_sui_system_state, SuiSystemStateTrait};

use crate::errors::IndexerError;
use crate::framework::interface::Handler;
use crate::metrics::IndexerMetrics;

use crate::store::{IndexerStoreV2, TemporaryCheckpointStoreV2};
use crate::store::{InterimModuleResolver, TemporaryEpochStoreV2, TransactionObjectChangesV2};
use crate::types_v2::IndexedEpochInfo;
use crate::types_v2::{
    IndexedCheckpoint, IndexedEvent, IndexedTransaction, IndexerResult, TransactionKind, TxIndex,
};
use crate::types_v2::{IndexedEndOfEpochInfo, IndexedObject, IndexedPackage};
use crate::IndexerConfig;

use super::tx_processor::InMemObjectCache;
use super::tx_processor::TxChangesProcessor;

const CHECKPOINT_QUEUE_SIZE: usize = 1000;

pub async fn new_handlers<S>(
    state: S,
    metrics: IndexerMetrics,
    config: &IndexerConfig,
) -> Result<CheckpointHandler<S>, IndexerError>
where
    S: IndexerStoreV2 + Clone + Sync + Send + 'static,
{
    let checkpoint_queue_size = std::env::var("CHECKPOINT_QUEUE_SIZE")
        .unwrap_or(CHECKPOINT_QUEUE_SIZE.to_string())
        .parse::<usize>()
        .unwrap();
    let global_metrics = get_metrics().unwrap();
    let (indexed_checkpoint_sender, indexed_checkpoint_receiver) =
        mysten_metrics::metered_channel::channel(
            checkpoint_queue_size,
            &global_metrics
                .channels
                .with_label_values(&["checkpoint_indexing"]),
        );

    let state_clone = state.clone();
    let metrics_clone = metrics.clone();
    let config_clone = config.clone();
    let (tx, rx) = watch::channel(None);
    spawn_monitored_task!(start_tx_checkpoint_commit_task(
        state_clone,
        metrics_clone,
        config_clone,
        indexed_checkpoint_receiver,
        tx,
    ));

    // let sui_client = SuiClientBuilder::default()
    //     .build(config.rpc_client_url.clone())
    //     .await
    //     .map_err(|e| IndexerError::FullNodeReadingError(e.to_string()))?;

    let checkpoint_processor = CheckpointHandler {
        state: state.clone(),
        metrics: metrics.clone(),
        indexed_checkpoint_sender,
        checkpoint_starting_tx_seq_numbers: HashMap::new(),
        // object_cache: InMemObjectCache::start(rx),
        package_cache: InMemPackageCache::start(rx),
        // sui_client: Arc::new(sui_client),
    };

    Ok(checkpoint_processor)
}

pub struct CheckpointHandler<S> {
    state: S,
    metrics: IndexerMetrics,
    indexed_checkpoint_sender: mysten_metrics::metered_channel::Sender<TemporaryCheckpointStoreV2>,
    // Map from checkpoint sequence number and its starting transaction sequence number
    checkpoint_starting_tx_seq_numbers: HashMap<CheckpointSequenceNumber, u64>,
    // object_cache: Arc<Mutex<InMemObjectCache>>,
    package_cache: Arc<Mutex<InMemPackageCache>>,
    // sui_client: Arc<SuiClient>,
}

#[async_trait]
impl<S> Handler for CheckpointHandler<S>
where
    S: IndexerStoreV2 + Clone + Sync + Send + 'static,
{
    fn name(&self) -> &str {
        "checkpoint-handler"
    }

    async fn process_checkpoint(&mut self, checkpoint_data: &CheckpointData) -> anyhow::Result<()> {
        let checkpoint_seq = checkpoint_data.checkpoint_summary.sequence_number();
        info!(checkpoint_seq, "Checkpoint received by CheckpointHandler");

        // update next checkpoint starting tx seq number
        self.checkpoint_starting_tx_seq_numbers.insert(
            *checkpoint_seq + 1,
            checkpoint_data
                .checkpoint_summary
                .network_total_transactions
                + 1,
        );
        let current_checkpoint_starting_tx_seq = if checkpoint_seq == &0 {
            0
        } else if self
            .checkpoint_starting_tx_seq_numbers
            .contains_key(checkpoint_seq)
        {
            self.checkpoint_starting_tx_seq_numbers[checkpoint_seq]
        } else {
            self.state.get_checkpoint_ending_tx_sequence_number(checkpoint_seq - 1).await?
            .unwrap_or_else(|| {
                panic!("While processing checkpoint {}, we failed to find the starting tx seq both in mem and DB.", checkpoint_seq)
            }) + 1
        };

        debug!(
            checkpoint_seq,
            "Checkpoint starting tx sequence number: {current_checkpoint_starting_tx_seq}"
        );

        // Index checkpoint data
        let index_timer = self.metrics.checkpoint_index_latency.start_timer();
        let checkpoint = Self::index_checkpoint_and_epoch(
            &self.state,
            current_checkpoint_starting_tx_seq,
            checkpoint_data.clone(),
            self.package_cache.clone(),
            // self.sui_client.clone(),
            &self.metrics,
        )
        .await
        .tap_err(|e| {
            error!(
                checkpoint_seq,
                "Failed to index checkpoints with error: {}",
                e.to_string()
            );
        })?;
        let elapsed = index_timer.stop_and_record();

        info!(
            checkpoint_seq,
            elapsed, "Checkpoint indexing finished, about to sending to commit handler"
        );
        // NOTE: when the channel is full, checkpoint_sender_guard will wait until the channel has space.
        // Checkpoints are sent sequentially to stick to the order of checkpoint sequence numbers.
        self.indexed_checkpoint_sender
            .send(checkpoint)
            .await
            .tap_ok(|_| info!(checkpoint_seq, "Checkpoint sent to commit handler"))
            .unwrap_or_else(|e| {
                panic!(
                    "checkpoint channel send should not fail, but got error: {:?}",
                    e
                )
            });

        Ok(())
    }
}

// This is a struct that is used to extract SuiSystemState and its dynamic children
// for end-of-epoch indexing.
struct EpochEndIndexingDataStore<'a> {
    // objects: &'a [Object],
    objects: Vec<&'a Object>,
}

impl<'a> EpochEndIndexingDataStore<'a> {
    pub fn new(data: &'a CheckpointData) -> Self {
        // We only care about output objects for end-of-epoch indexing
        Self {
            objects: data.output_objects(),
        }
    }
}

impl<'a> sui_types::storage::ObjectStore for EpochEndIndexingDataStore<'a> {
    fn get_object(
        &self,
        object_id: &ObjectID,
    ) -> Result<Option<Object>, sui_types::error::SuiError> {
        Ok(self
            .objects
            .iter()
            .find(|o| o.id() == *object_id)
            .cloned()
            .cloned())
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Result<Option<Object>, sui_types::error::SuiError> {
        Ok(self
            .objects
            .iter()
            .find(|o| o.id() == *object_id && o.version() == version)
            .cloned()
            .cloned())
    }
}

impl<S> CheckpointHandler<S>
where
    S: IndexerStoreV2 + Clone + Sync + Send + 'static,
{
    // FIXME: This function is problematic:
    // `get_sui_system_state` always returns the latest state
    async fn index_epoch(
        state: &S,
        data: &CheckpointData,
    ) -> Result<Option<TemporaryEpochStoreV2>, IndexerError> {
        let checkpoint_object_store = EpochEndIndexingDataStore::new(data);

        let CheckpointData {
            transactions,
            checkpoint_summary,
            checkpoint_contents: _,
        } = data;

        // Genesis epoch
        if *checkpoint_summary.sequence_number() == 0 {
            info!("Processing genesis epoch");
            let system_state: SuiSystemStateSummary =
                get_sui_system_state(&checkpoint_object_store)?.into_sui_system_state_summary();
            return Ok(Some(TemporaryEpochStoreV2 {
                last_epoch: None,
                new_epoch: IndexedEpochInfo {
                    epoch: 0,
                    first_checkpoint_id: 0,
                    epoch_start_timestamp: system_state.epoch_start_timestamp_ms,
                    validators: system_state.active_validators,
                    reference_gas_price: system_state.reference_gas_price,
                    protocol_version: system_state.protocol_version,
                    // Below is to be filled by end of epoch
                    epoch_total_transactions: 0,
                    end_of_epoch_info: None,
                    end_of_epoch_data: None,
                },
            }));
        }

        // If not end of epoch, return
        if checkpoint_summary.end_of_epoch_data.is_none() {
            return Ok(None);
        }

        let system_state: SuiSystemStateSummary =
            get_sui_system_state(&checkpoint_object_store)?.into_sui_system_state_summary();

        let epoch_event = transactions
            .iter()
            .flat_map(|t| t.events.as_ref().map(|e| &e.data))
            .flatten()
            .find(|ev| ev.is_system_epoch_info_event())
            .unwrap_or_else(|| {
                panic!(
                    "Can't find SystemEpochInfoEvent in epoch end checkpoint {}",
                    checkpoint_summary.sequence_number()
                )
            });

        let event = bcs::from_bytes::<SystemEpochInfoEvent>(&epoch_event.contents)?;

        let validators = system_state.active_validators;

        let last_epoch = system_state.epoch - 1;
        let network_tx_count_prev_epoch = state
            .get_network_total_transactions_previous_epoch(last_epoch)
            .await?;

        let last_end_of_epoch_info = EndOfEpochInfo {
            last_checkpoint_id: *checkpoint_summary.sequence_number(),
            epoch_end_timestamp: checkpoint_summary.timestamp_ms,
            protocol_version: event.protocol_version,
            reference_gas_price: event.reference_gas_price,
            total_stake: event.total_stake,
            storage_fund_reinvestment: event.storage_fund_reinvestment,
            storage_charge: event.storage_charge,
            storage_rebate: event.storage_rebate,
            leftover_storage_fund_inflow: event.leftover_storage_fund_inflow,
            stake_subsidy_amount: event.stake_subsidy_amount,
            storage_fund_balance: event.storage_fund_balance,
            total_gas_fees: event.total_gas_fees,
            total_stake_rewards_distributed: event.total_stake_rewards_distributed,
        };
        Ok(Some(TemporaryEpochStoreV2 {
            last_epoch: Some(IndexedEndOfEpochInfo {
                epoch: system_state.epoch - 1,
                end_of_epoch_info: last_end_of_epoch_info,
                end_of_epoch_data: checkpoint_summary
                    .end_of_epoch_data
                    .as_ref()
                    .unwrap()
                    .clone(),
                epoch_total_transactions: checkpoint_summary.network_total_transactions
                    - network_tx_count_prev_epoch,
            }),
            new_epoch: IndexedEpochInfo {
                epoch: system_state.epoch,
                validators,
                first_checkpoint_id: checkpoint_summary.sequence_number + 1,
                epoch_start_timestamp: system_state.epoch_start_timestamp_ms,
                protocol_version: system_state.protocol_version,
                reference_gas_price: system_state.reference_gas_price,
                // Below is to be filled by end of epoch
                end_of_epoch_info: None,
                end_of_epoch_data: None,
                epoch_total_transactions: 0,
            },
        }))
    }

    async fn index_checkpoint_and_epoch(
        state: &S,
        starting_tx_sequence_number: u64,
        data: CheckpointData,
        // object_cache: Arc<Mutex<InMemObjectCache>>,
        package_cache: Arc<Mutex<InMemPackageCache>>,
        // sui_client: Arc<SuiClient>,
        metrics: &IndexerMetrics,
    ) -> Result<TemporaryCheckpointStoreV2, IndexerError> {
        // error!("Input Objects: {:?}", data.input_objects().iter().map(|o| (o.id(), o.version())).collect::<Vec<_>>());
        // error!("Output Objects: {:?}", data.output_objects().iter().map(|o| (o.id(), o.version())).collect::<Vec<_>>());
        let (checkpoint, db_transactions, db_events, db_indices) = {
            let CheckpointData {
                transactions,
                checkpoint_summary,
                checkpoint_contents,
                // objects,
            } = &data;
            let checkpoint_seq = checkpoint_summary.sequence_number();
            let mut db_transactions = Vec::new();
            let mut db_events = Vec::new();
            let mut db_indices = Vec::new();

            // for (idx, (sender_signed_data, fx, events)) in transactions.iter().enumerate() {
            for (idx, tx) in transactions.iter().enumerate() {
                let CheckpointTransaction {
                    transaction: sender_signed_data,
                    effects: fx,
                    events,
                    input_objects,
                    output_objects,
                } = tx;
                let tx_sequence_number = starting_tx_sequence_number + idx as u64;
                let tx_digest = sender_signed_data.digest();
                let tx = sender_signed_data.transaction_data();
                let events = events
                    .as_ref()
                    .map(|events| events.data.clone())
                    .unwrap_or_default();

                let transaction_kind = if tx.is_system_tx() {
                    TransactionKind::SystemTransaction
                } else {
                    TransactionKind::ProgrammableTransaction
                };

                db_events.extend(events.iter().enumerate().map(|(idx, event)| {
                    IndexedEvent::from_event(
                        tx_sequence_number,
                        idx as u64,
                        *tx_digest,
                        event,
                        checkpoint_summary.timestamp_ms,
                    )
                }));

                let objects = input_objects
                    .iter()
                    .chain(output_objects.iter())
                    .collect::<Vec<_>>();

                let (balance_change, object_changes) = TxChangesProcessor::new(
                    // state,
                    &objects,
                    // object_cache.clone(),
                    // sui_client.clone(),
                    *checkpoint_seq,
                    metrics.clone(),
                )
                .get_changes(tx, fx, tx_digest)
                .await?;

                let db_txn = IndexedTransaction {
                    tx_sequence_number,
                    tx_digest: *tx_digest,
                    checkpoint_sequence_number: *checkpoint_summary.sequence_number(),
                    timestamp_ms: checkpoint_summary.timestamp_ms,
                    sender_signed_data: sender_signed_data.data().clone(),
                    effects: fx.clone(),
                    object_changes,
                    balance_change,
                    events,
                    transaction_kind,
                    successful_tx_num: if fx.status().is_ok() {
                        tx.kind().num_commands() as u64
                    } else {
                        0
                    },
                };

                db_transactions.push(db_txn);

                // Input Objects
                let input_objects = tx
                    .input_objects()
                    .expect("committed txns have been validated")
                    .into_iter()
                    .map(|obj_kind| obj_kind.object_id())
                    .collect::<Vec<_>>();

                // Changed Objects
                let changed_objects = fx
                    .all_changed_objects()
                    .into_iter()
                    .map(|(object_ref, _owner, _write_kind)| object_ref.0)
                    .collect::<Vec<_>>();

                // Senders
                let senders = vec![tx.sender()];

                // Recipients
                let recipients = fx
                    .all_changed_objects()
                    .into_iter()
                    .filter_map(|(_object_ref, owner, _write_kind)| match owner {
                        Owner::AddressOwner(address) => Some(address),
                        _ => None,
                    })
                    .unique()
                    .collect::<Vec<_>>();

                // Move Calls
                let move_calls = tx
                    .move_calls()
                    .iter()
                    .map(|(p, m, f)| (*<&ObjectID>::clone(p), m.to_string(), f.to_string()))
                    .collect();

                db_indices.push(TxIndex {
                    tx_sequence_number,
                    transaction_digest: *tx_digest,
                    input_objects,
                    changed_objects,
                    senders,
                    recipients,
                    move_calls,
                });
            }
            let successful_tx_num: u64 = db_transactions.iter().map(|t| t.successful_tx_num).sum();
            (
                IndexedCheckpoint::from_sui_checkpoint(
                    checkpoint_summary,
                    checkpoint_contents,
                    successful_tx_num as usize,
                ),
                db_transactions,
                db_events,
                db_indices,
            )
        };

        let epoch = Self::index_epoch(state, &data).await?;

        // Index Objects
        let (object_changes, packages) =
            Self::index_checkpoint(state, data, package_cache, metrics).await;

        Ok(TemporaryCheckpointStoreV2 {
            checkpoint,
            transactions: db_transactions,
            events: db_events,
            tx_indices: db_indices,
            object_changes,
            packages,
            epoch,
        })
    }

    async fn index_checkpoint(
        state: &S,
        data: CheckpointData,
        package_cache: Arc<Mutex<InMemPackageCache>>,
        metrics: &IndexerMetrics,
    ) -> (TransactionObjectChangesV2, Vec<IndexedPackage>) {
        let checkpoint_seq = data.checkpoint_summary.sequence_number;
        info!(checkpoint_seq, "Indexing checkpoint");
        let packages = Self::index_packages(&data, metrics);

        let object_changes = Self::index_objects(state, data, &packages, package_cache, metrics);

        (object_changes, packages)
    }

    fn index_objects(
        state: &S,
        data: CheckpointData,
        packages: &[IndexedPackage],
        package_cache: Arc<Mutex<InMemPackageCache>>,
        metrics: &IndexerMetrics,
    ) -> TransactionObjectChangesV2 {
        let _timer = metrics.indexing_objects_latency.start_timer();
        let checkpoint_seq = data.checkpoint_summary.sequence_number;
        let module_resolver = InterimModuleResolver::new(
            state.module_cache(),
            package_cache,
            packages,
            checkpoint_seq,
            metrics.clone(),
        );
        let deleted_objects = data
            .transactions
            .iter()
            .flat_map(|tx| get_deleted_objects(&tx.effects))
            .collect::<Vec<_>>();

        let deleted_object_ids = deleted_objects
            .iter()
            .map(|o| (o.0, o.1))
            .collect::<HashSet<_>>();

        let (objects, intermediate_versions) = get_latest_objects(data.output_objects());

        let changed_objects = data
            .transactions
            .iter()
            .flat_map(|tx| {
                let CheckpointTransaction {
                    transaction: tx,
                    effects: fx,
                    ..
                } = tx;
                fx.all_changed_objects()
                    .into_iter()
                    .filter_map(|(oref, _owner, _kind)| {
                        // We don't care about objects that are deleted or updated more than once
                        if intermediate_versions.contains(&(oref.0, oref.1))
                            || deleted_object_ids.contains(&(oref.0, oref.1))
                        {
                            return None;
                        }
                        let object = objects.get(&(oref.0)).unwrap_or_else(|| {
                            panic!(
                                "object {:?} not found in CheckpointData (tx_digest: {})",
                                oref.0,
                                tx.digest()
                            )
                        });
                        assert_eq!(oref.1, object.version());
                        let df_info =
                            try_create_dynamic_field_info(object, &objects, &module_resolver)
                                .expect("failed to create dynamic field info");
                        Some(IndexedObject::from_object(
                            checkpoint_seq,
                            object.clone(),
                            df_info,
                        ))
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        TransactionObjectChangesV2 {
            changed_objects,
            deleted_objects,
        }
    }

    fn index_packages(
        checkpoint_data: &CheckpointData,
        metrics: &IndexerMetrics,
    ) -> Vec<IndexedPackage> {
        let _timer = metrics.indexing_packages_latency.start_timer();
        checkpoint_data
            .output_objects()
            .iter()
            .filter_map(|o| {
                if let sui_types::object::Data::Package(p) = &o.data {
                    Some(IndexedPackage {
                        package_id: o.id(),
                        move_package: p.clone(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }
}

pub async fn start_tx_checkpoint_commit_task<S>(
    state: S,
    metrics: IndexerMetrics,
    config: IndexerConfig,
    tx_indexing_receiver: mysten_metrics::metered_channel::Receiver<TemporaryCheckpointStoreV2>,
    commit_notifier: watch::Sender<Option<CheckpointSequenceNumber>>,
) where
    S: IndexerStoreV2 + Clone + Sync + Send + 'static,
{
    use futures::StreamExt;

    info!("Indexer checkpoint commit task started...");
    let checkpoint_commit_batch_size = std::env::var("CHECKPOINT_COMMIT_BATCH_SIZE")
        .unwrap_or(5.to_string())
        .parse::<usize>()
        .unwrap();
    info!("Using checkpoint commit batch size {checkpoint_commit_batch_size}");

    let mut stream = mysten_metrics::metered_channel::ReceiverStream::new(tx_indexing_receiver)
        .ready_chunks(checkpoint_commit_batch_size);

    while let Some(indexed_checkpoint_batch) = stream.next().await {
        let mut checkpoint_batch = vec![];
        let mut tx_batch = vec![];
        let mut events_batch = vec![];
        let mut tx_indices_batch = vec![];
        let mut object_changes_batch = vec![];
        let mut packages_batch = vec![];
        let mut _epoch_batch = None;

        if config.skip_db_commit {
            info!(
                "[Checkpoint/Tx] Downloaded and indexed checkpoint {:?} - {:?} successfully, skipping DB commit...",
                indexed_checkpoint_batch.first().map(|c| c.checkpoint.sequence_number),
                indexed_checkpoint_batch.last().map(|c| c.checkpoint.sequence_number),
            );
            continue;
        }

        for indexed_checkpoint in indexed_checkpoint_batch {
            let TemporaryCheckpointStoreV2 {
                checkpoint,
                transactions,
                events,
                tx_indices,
                object_changes,
                packages,
                epoch,
            } = indexed_checkpoint;
            checkpoint_batch.push(checkpoint);
            tx_batch.push(transactions);
            events_batch.push(events);
            tx_indices_batch.push(tx_indices);
            object_changes_batch.push(object_changes);
            packages_batch.push(packages);
            _epoch_batch = epoch;
        }

        let first_checkpoint_seq = checkpoint_batch.first().as_ref().unwrap().sequence_number;
        let last_checkpoint_seq = checkpoint_batch.last().as_ref().unwrap().sequence_number;

        let guard = metrics.checkpoint_db_commit_latency.start_timer();
        let tx_batch = tx_batch.into_iter().flatten().collect::<Vec<_>>();
        let tx_indices_batch = tx_indices_batch.into_iter().flatten().collect::<Vec<_>>();
        let events_batch = events_batch.into_iter().flatten().collect::<Vec<_>>();
        let packages_batch = packages_batch.into_iter().flatten().collect::<Vec<_>>();
        let checkpoint_num = checkpoint_batch.len();
        let tx_count = tx_batch.len();

        // TODO: persist epoch

        futures::future::join_all(vec![
            state.persist_transactions(tx_batch),
            // state.persist_tx_indices(tx_indices_batch),
            state.persist_events(events_batch),
            state.persist_packages(packages_batch),
        ])
        .await
        .into_iter()
        .map(|res| {
            if res.is_err() {
                error!("Failed to persist data with error: {:?}", res);
            }
            res
        })
        .collect::<IndexerResult<Vec<_>>>()
        .expect("Persisting data into DB should not fail.");

        // Note: the reason that we persist object changes with checkpoint
        // atomically is that when we batch process checkpoints, some hot
        // objects may be updated multiple times in the batch. Because we
        // only store the latest objects, it's possible that the intermediate
        // verisons will be nowhere to be found unless we ask the the data
        // source again. Then if the idnexer restarts and picks up checkpoints
        // that is halfway through (objects changes persisted but not checkpoints)
        // it will have difficulty in getting the depended objects and fail.
        // When we switch to also getting input objects from the data source,
        // we can largely remove this atomicity requirement.
        state
            .persist_objects_and_checkpoints(object_changes_batch, checkpoint_batch)
            .await
            .tap_err(|e| {
                error!(
                    "Failed to persist checkpoint data with error: {}",
                    e.to_string()
                );
            })
            .expect("Persisting data into DB should not fail.");
        let elapsed = guard.stop_and_record();

        commit_notifier
            .send(Some(last_checkpoint_seq))
            .expect("Commit watcher should not be closed");

        metrics
            .latest_tx_checkpoint_sequence_number
            .set(last_checkpoint_seq as i64);

        metrics
            .total_tx_checkpoint_committed
            .inc_by(checkpoint_num as u64);
        metrics.total_transaction_committed.inc_by(tx_count as u64);
        info!(
            elapsed,
            "Checkpoint {}-{} committed with {} transactions.",
            first_checkpoint_seq,
            last_checkpoint_seq,
            tx_count,
        );
        metrics
            .transaction_per_checkpoint
            .observe(tx_count as f64 / (last_checkpoint_seq - first_checkpoint_seq + 1) as f64);
        // 1000.0 is not necessarily the batch size, it's to roughly map average tx commit latency to [0.1, 1] seconds,
        // which is well covered by DB_COMMIT_LATENCY_SEC_BUCKETS.
        metrics
            .thousand_transaction_avg_db_commit_latency
            .observe(elapsed * 1000.0 / tx_count as f64);
    }
}

pub fn get_deleted_objects(effects: &TransactionEffects) -> Vec<ObjectRef> {
    let deleted = effects.deleted().into_iter();
    let wrapped = effects.wrapped().into_iter();
    let unwrapped_then_deleted = effects.unwrapped_then_deleted().into_iter();
    deleted
        .chain(wrapped)
        .chain(unwrapped_then_deleted)
        .collect::<Vec<_>>()
}

pub fn get_latest_objects(
    objects: Vec<&Object>,
) -> (
    HashMap<ObjectID, Object>,
    HashSet<(ObjectID, SequenceNumber)>,
) {
    let mut latest_objects = HashMap::new();
    let mut discarded_versions = HashSet::new();
    for object in objects {
        match latest_objects.entry(object.id()) {
            Entry::Vacant(e) => {
                e.insert(object.clone());
            }
            Entry::Occupied(mut e) => {
                if object.version() > e.get().version() {
                    discarded_versions.insert((e.get().id(), e.get().version()));
                    e.insert(object.clone());
                }
            }
        }
    }
    (latest_objects, discarded_versions)
}

fn try_create_dynamic_field_info(
    o: &Object,
    written: &HashMap<ObjectID, Object>,
    resolver: &impl GetModule,
) -> IndexerResult<Option<DynamicFieldInfo>> {
    // Skip if not a move object
    let Some(move_object) = o.data.try_as_move().cloned() else {
        return Ok(None);
    };

    if !move_object.type_().is_dynamic_field() {
        return Ok(None);
    }

    let move_struct =
        move_object.to_move_struct_with_resolver(ObjectFormatOptions::default(), resolver)?;

    let (name_value, type_, object_id) =
        DynamicFieldInfo::parse_move_object(&move_struct).tap_err(|e| warn!("{e}"))?;

    let name_type = move_object.type_().try_extract_field_name(&type_)?;

    let bcs_name = bcs::to_bytes(&name_value.clone().undecorate()).map_err(|e| {
        IndexerError::SerdeError(format!(
            "Failed to serialize dynamic field name {:?}: {e}",
            name_value
        ))
    })?;

    let name = DynamicFieldName {
        type_: name_type,
        value: SuiMoveValue::from(name_value).to_json_value(),
    };
    Ok(Some(match type_ {
        DynamicFieldType::DynamicObject => {
            let object = written
                .get(&object_id)
                .ok_or(IndexerError::UncategorizedError(anyhow::anyhow!(
                    "Failed to find object_id {:?} when trying to create dynamic field info",
                    object_id
                )))?;
            let version = object.version();
            let digest = object.digest();
            let object_type = object.data.type_().unwrap().clone();
            DynamicFieldInfo {
                name,
                bcs_name,
                type_,
                object_type: object_type.to_string(),
                object_id,
                version,
                digest,
            }
        }
        DynamicFieldType::DynamicField => DynamicFieldInfo {
            name,
            bcs_name,
            type_,
            object_type: move_object.into_type().into_type_params()[1].to_string(),
            object_id: o.id(),
            version: o.version(),
            digest: o.digest(),
        },
    }))
}
