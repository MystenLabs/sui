// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::traits::ToFromBytes;
use itertools::Itertools;
use move_core_types::ident_str;
use mysten_metrics::{get_metrics, spawn_monitored_task};
use std::collections::HashMap;
use sui_rest_api::{CheckpointData, CheckpointTransaction};
use sui_types::committee::EpochId;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::object::Owner;
use sui_types::transaction::TransactionDataAPI;
use tap::tap::TapFallible;
use tracing::{error, info, warn};

use sui_types::base_types::ObjectID;
use sui_types::messages_checkpoint::{CheckpointCommitment, CheckpointSequenceNumber};
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
use sui_types::sui_system_state::{get_sui_system_state, SuiSystemStateTrait};
use sui_types::SUI_SYSTEM_ADDRESS;

use crate::errors::IndexerError;
use crate::framework::interface::Handler;
use crate::metrics::IndexerMetrics;
use crate::models::checkpoints::Checkpoint;
use crate::models::epoch::{DBEpochInfo, SystemEpochInfoEvent};
use crate::models::events::Event;
use crate::models::objects::{DeletedObject, ObjectStatus};
use crate::models::packages::Package;
use crate::models::transaction_index::ChangedObject;
use crate::models::transaction_index::InputObject;
use crate::models::transaction_index::MoveCall;
use crate::models::transaction_index::Recipient;
use crate::models::transactions::Transaction;
use crate::store::{
    IndexerStore, TemporaryCheckpointStore, TemporaryEpochStore, TransactionObjectChanges,
};
use crate::IndexerConfig;

const CHECKPOINT_QUEUE_SIZE: usize = 1000;
const EPOCH_QUEUE_LIMIT: usize = 20;

pub fn new_handlers<S>(
    state: S,
    metrics: IndexerMetrics,
    config: &IndexerConfig,
) -> (CheckpointProcessor<S>, ObjectsProcessor<S>)
where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    let checkpoint_queue_size = std::env::var("CHECKPOINT_QUEUE_SIZE")
        .unwrap_or(CHECKPOINT_QUEUE_SIZE.to_string())
        .parse::<usize>()
        .unwrap();
    let global_metrics = get_metrics().unwrap();
    let (tx_indexing_sender, tx_indexing_receiver) = mysten_metrics::metered_channel::channel(
        checkpoint_queue_size,
        &global_metrics
            .channels
            .with_label_values(&["checkpoint_tx_indexing"]),
    );

    let (object_indexing_sender, object_indexing_receiver) =
        mysten_metrics::metered_channel::channel(
            checkpoint_queue_size,
            &global_metrics
                .channels
                .with_label_values(&["checkpoint_object_indexing"]),
        );

    let (epoch_indexing_sender, epoch_indexing_receiver) = mysten_metrics::metered_channel::channel(
        EPOCH_QUEUE_LIMIT,
        &global_metrics
            .channels
            .with_label_values(&["checkpoint_epoch_indexing"]),
    );

    let state_clone = state.clone();
    let metrics_clone = metrics.clone();
    let config_clone = config.clone();
    spawn_monitored_task!(start_tx_checkpoint_commit_task(
        state_clone,
        metrics_clone,
        config_clone,
        tx_indexing_receiver,
    ));

    let state_clone = state.clone();
    let metrics_clone = metrics.clone();
    spawn_monitored_task!(start_epoch_commit_task(
        state_clone,
        metrics_clone,
        epoch_indexing_receiver,
    ));

    let state_clone = state.clone();
    let metrics_clone = metrics.clone();
    let config_clone = config.clone();
    spawn_monitored_task!(start_object_checkpoint_commit_task(
        state_clone,
        metrics_clone,
        config_clone,
        object_indexing_receiver,
    ));

    let checkpoint_processor = CheckpointProcessor {
        state: state.clone(),
        metrics: metrics.clone(),
        epoch_indexing_sender,
        checkpoint_sender: tx_indexing_sender,
    };

    let object_processor = ObjectsProcessor {
        metrics,
        object_indexing_sender,
        state,
    };

    (checkpoint_processor, object_processor)
}

pub struct CheckpointProcessor<S> {
    state: S,
    metrics: IndexerMetrics,
    epoch_indexing_sender: mysten_metrics::metered_channel::Sender<TemporaryEpochStore>,
    checkpoint_sender: mysten_metrics::metered_channel::Sender<TemporaryCheckpointStore>,
}

#[async_trait::async_trait]
impl<S> Handler for CheckpointProcessor<S>
where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    fn name(&self) -> &str {
        "checkpoint-transaction-and-epoch-indexer"
    }
    async fn process_checkpoint(&mut self, checkpoint_data: &CheckpointData) -> anyhow::Result<()> {
        info!(
            checkpoint_seq = checkpoint_data.checkpoint_summary.sequence_number(),
            "Checkpoint received by indexing processor"
        );
        // Index checkpoint data
        let index_timer = self.metrics.checkpoint_index_latency.start_timer();

        let (checkpoint, epoch) = Self::index_checkpoint_and_epoch(&self.state, checkpoint_data)
            .await
            .tap_err(|e| {
                error!(
                    "Failed to index checkpoints {:?} with error: {}",
                    checkpoint_data,
                    e.to_string()
                );
            })?;
        let elapsed = index_timer.stop_and_record();

        // commit first epoch immediately, send other epochs to channel to be committed later.
        if let Some(epoch) = epoch {
            if epoch.last_epoch.is_none() {
                let epoch_db_guard = self.metrics.epoch_db_commit_latency.start_timer();
                info!("Persisting genesis epoch...");
                let mut persist_first_epoch_res = self.state.persist_epoch(&epoch).await;
                while persist_first_epoch_res.is_err() {
                    warn!("Failed to persist first epoch, retrying...");
                    persist_first_epoch_res = self.state.persist_epoch(&epoch).await;
                }
                epoch_db_guard.stop_and_record();
                self.metrics.total_epoch_committed.inc();
                info!("Persisted genesis epoch");
            } else {
                // NOTE: when the channel is full, epoch_sender_guard will wait until the channel has space.
                self.epoch_indexing_sender.send(epoch).await.map_err(|e| {
                    error!(
                        "Failed to send indexed epoch to epoch commit handler with error {}",
                        e.to_string()
                    );
                    IndexerError::MpscChannelError(e.to_string())
                })?;
            }
        }
        let seq = checkpoint.checkpoint.sequence_number;
        info!(
            checkpoint_seq = seq,
            elapsed, "Checkpoint indexing finished, about to sending to commit handler"
        );
        // NOTE: when the channel is full, checkpoint_sender_guard will wait until the channel has space.
        // Checkpoints are sent sequentially to stick to the order of checkpoint sequence numbers.
        self.checkpoint_sender
            .send(checkpoint)
            .await
            .tap_ok(|_| info!(checkpoint_seq = seq, "Checkpoint sent to commit handler"))
            .unwrap_or_else(|e| {
                panic!(
                    "checkpoint channel send should not fail, but got error: {:?}",
                    e
                )
            });

        Ok(())
    }
}

struct CheckpointDataObjectStore<'a> {
    objects: Vec<&'a sui_types::object::Object>,
}

impl<'a> CheckpointDataObjectStore<'a> {
    fn new(data: &'a CheckpointData) -> Self {
        let objects = data
            .transactions
            .iter()
            .flat_map(|tx| tx.output_objects.iter())
            .collect();
        Self { objects }
    }
}

impl<'a> sui_types::storage::ObjectStore for CheckpointDataObjectStore<'a> {
    fn get_object(
        &self,
        object_id: &ObjectID,
    ) -> Result<Option<sui_types::object::Object>, sui_types::storage::error::Error> {
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
    ) -> Result<Option<sui_types::object::Object>, sui_types::storage::error::Error> {
        Ok(self
            .objects
            .iter()
            .find(|o| o.id() == *object_id && o.version() == version)
            .cloned()
            .cloned())
    }
}

impl<S> CheckpointProcessor<S>
where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    async fn index_epoch(
        state: &S,
        data: &CheckpointData,
    ) -> Result<Option<TemporaryEpochStore>, IndexerError> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            checkpoint_contents: _,
        } = data;

        let checkpoint_object_store = CheckpointDataObjectStore::new(data);

        // NOTE: Index epoch when object checkpoint index has reached the same checkpoint,
        // because epoch info is based on the latest system state object by the current checkpoint.
        let epoch_index = if checkpoint_summary.epoch() == 0
            && *checkpoint_summary.sequence_number() == 0
        {
            // very first epoch
            let system_state = get_sui_system_state(&checkpoint_object_store)?;
            let system_state: SuiSystemStateSummary = system_state.into_sui_system_state_summary();
            let validators = system_state
                .active_validators
                .iter()
                .map(|v| (system_state.epoch, v.clone()).into())
                .collect();

            Some(TemporaryEpochStore {
                last_epoch: None,
                new_epoch: DBEpochInfo {
                    epoch: 0,
                    first_checkpoint_id: 0,
                    epoch_start_timestamp: system_state.epoch_start_timestamp_ms as i64,
                    ..Default::default()
                },
                system_state: system_state.into(),
                validators,
            })
        } else if let Some(end_of_epoch_data) = &checkpoint_summary.end_of_epoch_data {
            let system_state = get_sui_system_state(&checkpoint_object_store)?;
            let system_state: SuiSystemStateSummary = system_state.into_sui_system_state_summary();

            let epoch_event = transactions
                .iter()
                .flat_map(|tx| tx.events.as_ref().map(|e| &e.data))
                .flatten()
                .find(|ev| {
                    ev.type_.address == SUI_SYSTEM_ADDRESS
                        && ev.type_.module.as_ident_str() == ident_str!("sui_system_state_inner")
                        && ev.type_.name.as_ident_str() == ident_str!("SystemEpochInfoEvent")
                });

            let event = epoch_event
                .map(|e| bcs::from_bytes::<SystemEpochInfoEvent>(&e.contents))
                .transpose()?;

            let validators = system_state
                .active_validators
                .iter()
                .map(|v| (system_state.epoch, v.clone()).into())
                .collect();

            let epoch_commitments = end_of_epoch_data
                .epoch_commitments
                .iter()
                .map(|c| match c {
                    CheckpointCommitment::ECMHLiveObjectSetDigest(d) => {
                        Some(d.digest.into_inner().to_vec())
                    }
                })
                .collect();

            let (next_epoch_committee, next_epoch_committee_stake) =
                end_of_epoch_data.next_epoch_committee.iter().fold(
                    (vec![], vec![]),
                    |(mut names, mut stakes), (name, stake)| {
                        names.push(Some(name.as_bytes().to_vec()));
                        stakes.push(Some(*stake as i64));
                        (names, stakes)
                    },
                );

            let event = event.as_ref();

            let last_epoch = system_state.epoch as i64 - 1;
            let network_tx_count_prev_epoch = state
                .get_network_total_transactions_previous_epoch(last_epoch)
                .await?;
            Some(TemporaryEpochStore {
                last_epoch: Some(DBEpochInfo {
                    epoch: last_epoch,
                    first_checkpoint_id: 0,
                    last_checkpoint_id: Some(*checkpoint_summary.sequence_number() as i64),
                    epoch_start_timestamp: 0,
                    epoch_end_timestamp: Some(checkpoint_summary.timestamp_ms as i64),
                    epoch_total_transactions: checkpoint_summary.network_total_transactions as i64
                        - network_tx_count_prev_epoch,
                    next_epoch_version: Some(
                        end_of_epoch_data.next_epoch_protocol_version.as_u64() as i64,
                    ),
                    next_epoch_committee,
                    next_epoch_committee_stake,
                    stake_subsidy_amount: event.map(|e| e.stake_subsidy_amount),
                    reference_gas_price: event.map(|e| e.reference_gas_price),
                    storage_fund_balance: event.map(|e| e.storage_fund_balance),
                    total_gas_fees: event.map(|e| e.total_gas_fees),
                    total_stake_rewards_distributed: event
                        .map(|e| e.total_stake_rewards_distributed),
                    total_stake: event.map(|e| e.total_stake),
                    storage_fund_reinvestment: event.map(|e| e.storage_fund_reinvestment),
                    storage_charge: event.map(|e| e.storage_charge),
                    protocol_version: event.map(|e| e.protocol_version),
                    storage_rebate: event.map(|e| e.storage_rebate),
                    leftover_storage_fund_inflow: event.map(|e| e.leftover_storage_fund_inflow),
                    epoch_commitments,
                }),
                new_epoch: DBEpochInfo {
                    epoch: system_state.epoch as i64,
                    first_checkpoint_id: checkpoint_summary.sequence_number as i64 + 1,
                    epoch_start_timestamp: system_state.epoch_start_timestamp_ms as i64,
                    ..Default::default()
                },
                system_state: system_state.into(),
                validators,
            })
        } else {
            None
        };

        Ok(epoch_index)
    }

    async fn index_checkpoint_and_epoch(
        state: &S,
        data: &CheckpointData,
    ) -> Result<(TemporaryCheckpointStore, Option<TemporaryEpochStore>), IndexerError> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            checkpoint_contents,
        } = data;

        let mut db_transactions = Vec::new();
        let mut db_events = Vec::new();
        let mut db_input_objects = Vec::new();
        let mut db_changed_objects = Vec::new();
        let mut db_move_calls = Vec::new();
        let mut db_recipients = Vec::new();

        for CheckpointTransaction {
            transaction: tx,
            effects: fx,
            events,
            input_objects: _,
            output_objects: _,
        } in transactions
        {
            let transaction_digest = tx.digest();
            let tx = tx.transaction_data();

            let db_txn = Transaction {
                id: None,
                transaction_digest: transaction_digest.base58_encode(),
                sender: tx.sender().to_string(),
                checkpoint_sequence_number: Some(*checkpoint_summary.sequence_number() as i64),
                timestamp_ms: Some(checkpoint_summary.timestamp_ms as i64),
                transaction_kind: tx.kind().name().to_owned(),
                transaction_count: tx.kind().tx_count() as i64,
                execution_success: fx.status().is_ok(),
                gas_object_id: fx.gas_object().0 .0.to_string(),
                gas_object_sequence: fx.gas_object().0 .1.value() as i64,
                gas_object_digest: fx.gas_object().0 .2.to_string(),
                gas_budget: tx.gas_budget() as i64,
                total_gas_cost: fx.gas_cost_summary().net_gas_usage(),
                computation_cost: fx.gas_cost_summary().computation_cost as i64,
                storage_cost: fx.gas_cost_summary().storage_cost as i64,
                storage_rebate: fx.gas_cost_summary().storage_rebate as i64,
                non_refundable_storage_fee: fx.gas_cost_summary().non_refundable_storage_fee as i64,
                gas_price: tx.gas_price() as i64,
                raw_transaction: bcs::to_bytes(&tx).unwrap(),
                transaction_effects_content: serde_json::to_string(&fx).unwrap(),
                confirmed_local_execution: None,
            };

            db_transactions.push(db_txn);

            db_events.extend(events.iter().flat_map(|events| &events.data).map(|event| {
                Event::from_sui_event(event, transaction_digest, checkpoint_summary.timestamp_ms)
            }));

            // Input Objects
            db_input_objects.extend(
                tx.input_objects()
                    .expect("committed txns have been validated")
                    .into_iter()
                    .map(|obj_kind| InputObject {
                        id: None,
                        transaction_digest: transaction_digest.to_string(),
                        checkpoint_sequence_number: *checkpoint_summary.sequence_number() as i64,
                        epoch: checkpoint_summary.epoch() as i64,
                        object_id: obj_kind.object_id().to_string(),
                        object_version: obj_kind.version().map(|v| v.value() as i64),
                    }),
            );

            // Changed Objects
            db_changed_objects.extend(fx.all_changed_objects().into_iter().map(
                |(object_ref, _owner, write_kind)| ChangedObject {
                    id: None,
                    transaction_digest: transaction_digest.to_string(),
                    checkpoint_sequence_number: *checkpoint_summary.sequence_number() as i64,
                    epoch: checkpoint_summary.epoch() as i64,
                    object_id: object_ref.0.to_string(),
                    object_change_type: crate::types::write_kind_to_str(write_kind).to_string(),
                    object_version: object_ref.1.value() as i64,
                },
            ));

            // Move Calls
            if let sui_types::transaction::TransactionKind::ProgrammableTransaction(pt) = tx.kind()
            {
                db_move_calls.extend(pt.commands.clone().into_iter().filter_map(move |command| {
                    match command {
                        sui_types::transaction::Command::MoveCall(m) => Some(MoveCall {
                            id: None,
                            transaction_digest: transaction_digest.to_string(),
                            checkpoint_sequence_number: *checkpoint_summary.sequence_number()
                                as i64,
                            epoch: checkpoint_summary.epoch() as i64,
                            sender: tx.sender().to_string(),
                            move_package: m.package.to_string(),
                            move_module: m.module.to_string(),
                            move_function: m.function.to_string(),
                        }),
                        _ => None,
                    }
                }));
            }

            // Recipients
            db_recipients.extend(
                fx.all_changed_objects()
                    .into_iter()
                    .filter_map(|(_object_ref, owner, _write_kind)| match owner {
                        Owner::AddressOwner(address) => Some(address.to_string()),
                        _ => None,
                    })
                    .unique()
                    .map(|recipient| Recipient {
                        id: None,
                        transaction_digest: transaction_digest.to_string(),
                        checkpoint_sequence_number: *checkpoint_summary.sequence_number() as i64,
                        epoch: checkpoint_summary.epoch() as i64,
                        sender: tx.sender().to_string(),
                        recipient,
                    }),
            );
        }

        let epoch_index = Self::index_epoch(state, data).await?;

        let total_transactions = db_transactions.iter().map(|t| t.transaction_count).sum();
        let total_successful_transaction_blocks = db_transactions
            .iter()
            .filter(|t| t.execution_success)
            .count();
        let total_successful_transactions = db_transactions
            .iter()
            .filter(|t| t.execution_success)
            .map(|t| t.transaction_count)
            .sum();

        Ok((
            TemporaryCheckpointStore {
                checkpoint: Checkpoint::from_sui_checkpoint(
                    checkpoint_summary,
                    checkpoint_contents,
                    total_transactions,
                    total_successful_transactions,
                    total_successful_transaction_blocks as i64,
                ),
                transactions: db_transactions,
                events: db_events,
                input_objects: db_input_objects,
                changed_objects: db_changed_objects,
                move_calls: db_move_calls,
                recipients: db_recipients,
            },
            epoch_index,
        ))
    }
}

const DB_COMMIT_RETRY_INTERVAL_IN_MILLIS: u64 = 100;

pub async fn start_tx_checkpoint_commit_task<S>(
    state: S,
    metrics: IndexerMetrics,
    config: IndexerConfig,
    tx_indexing_receiver: mysten_metrics::metered_channel::Receiver<TemporaryCheckpointStore>,
) where
    S: IndexerStore + Clone + Sync + Send + 'static,
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

        if config.skip_db_commit {
            info!(
                "[Checkpoint/Tx] Downloaded and indexed checkpoint {:?} - {:?} successfully, skipping DB commit...",
                indexed_checkpoint_batch.first().map(|c| c.checkpoint.sequence_number),
                indexed_checkpoint_batch.last().map(|c| c.checkpoint.sequence_number),
            );
            continue;
        }

        for indexed_checkpoint in indexed_checkpoint_batch {
            // Write checkpoint to DB
            let TemporaryCheckpointStore {
                checkpoint,
                transactions,
                events,
                input_objects,
                changed_objects,
                move_calls,
                recipients,
            } = indexed_checkpoint;
            checkpoint_batch.push(checkpoint);
            tx_batch.push(transactions);

            // NOTE: retrials are necessary here, otherwise results can be popped and discarded.
            let events_handler = state.clone();
            spawn_monitored_task!(async move {
                let mut event_commit_res = events_handler.persist_events(&events).await;
                while let Err(e) = event_commit_res {
                    warn!(
                        "Indexer event commit failed with error: {:?}, retrying after {:?} milli-secs...",
                        e, DB_COMMIT_RETRY_INTERVAL_IN_MILLIS
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(
                        DB_COMMIT_RETRY_INTERVAL_IN_MILLIS,
                    ))
                    .await;
                    event_commit_res = events_handler.persist_events(&events).await;
                }
            });

            let tx_index_table_handler = state.clone();
            spawn_monitored_task!(async move {
                let mut transaction_index_tables_commit_res = tx_index_table_handler
                    .persist_transaction_index_tables(
                        &input_objects,
                        &changed_objects,
                        &move_calls,
                        &recipients,
                    )
                    .await;
                while let Err(e) = transaction_index_tables_commit_res {
                    warn!(
                        "Indexer transaction index tables commit failed with error: {:?}, retrying after {:?} milli-secs...",
                        e, DB_COMMIT_RETRY_INTERVAL_IN_MILLIS
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(
                        DB_COMMIT_RETRY_INTERVAL_IN_MILLIS,
                    ))
                    .await;
                    transaction_index_tables_commit_res = tx_index_table_handler
                        .persist_transaction_index_tables(
                            &input_objects,
                            &changed_objects,
                            &move_calls,
                            &recipients,
                        )
                        .await;
                }
            });
        }

        // now commit batched data
        let tx_batch = tx_batch.into_iter().flatten().collect::<Vec<_>>();
        let checkpoint_tx_db_guard = metrics.checkpoint_db_commit_latency.start_timer();
        let mut checkpoint_tx_commit_res = state
            .persist_checkpoint_transactions(
                &checkpoint_batch,
                &tx_batch,
                metrics.total_transaction_chunk_committed.clone(),
            )
            .await;
        while let Err(e) = checkpoint_tx_commit_res {
            warn!(
                "Indexer checkpoint & transaction commit failed with error: {:?}, retrying after {:?} milli-secs...",
                e, DB_COMMIT_RETRY_INTERVAL_IN_MILLIS
            );
            tokio::time::sleep(std::time::Duration::from_millis(
                DB_COMMIT_RETRY_INTERVAL_IN_MILLIS,
            ))
            .await;
            checkpoint_tx_commit_res = state
                .persist_checkpoint_transactions(
                    &checkpoint_batch,
                    &tx_batch,
                    metrics.total_transaction_chunk_committed.clone(),
                )
                .await;
        }
        let elapsed = checkpoint_tx_db_guard.stop_and_record();
        // unwrap: batch must not be empty at this point
        let first_checkpoint_seq = checkpoint_batch.first().as_ref().unwrap().sequence_number;
        let last_checkpoint_seq = checkpoint_batch.last().as_ref().unwrap().sequence_number;
        metrics
            .latest_tx_checkpoint_sequence_number
            .set(last_checkpoint_seq);

        metrics
            .total_tx_checkpoint_committed
            .inc_by(checkpoint_batch.len() as u64);
        let tx_count = tx_batch.len();
        metrics.total_transaction_committed.inc_by(tx_count as u64);
        info!(
            elapsed,
            "Tx Checkpoint {}-{} committed with {} transactions.",
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

pub async fn start_epoch_commit_task<S>(
    state: S,
    metrics: IndexerMetrics,
    epoch_indexing_receiver: mysten_metrics::metered_channel::Receiver<TemporaryEpochStore>,
) where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    use futures::StreamExt;

    info!("Indexer epoch commit task started...");
    let mut stream = mysten_metrics::metered_channel::ReceiverStream::new(epoch_indexing_receiver);

    while let Some(indexed_epoch) = stream.next().await {
        if indexed_epoch.last_epoch.is_some() {
            let epoch_db_guard = metrics.epoch_db_commit_latency.start_timer();
            let mut epoch_commit_res = state.persist_epoch(&indexed_epoch).await;
            // NOTE: retrials are necessary here, otherwise indexed_epoch can be popped and discarded.
            // TODO: use macro to replace this pattern in this file.
            while let Err(e) = epoch_commit_res {
                warn!(
                    "Indexer epoch commit failed with error: {:?}, retrying after {:?} milli-secs...",
                    e, DB_COMMIT_RETRY_INTERVAL_IN_MILLIS
                );
                tokio::time::sleep(std::time::Duration::from_millis(
                    DB_COMMIT_RETRY_INTERVAL_IN_MILLIS,
                ))
                .await;
                epoch_commit_res = state.persist_epoch(&indexed_epoch).await;
            }
            epoch_db_guard.stop_and_record();
            metrics.total_epoch_committed.inc();
        }
    }
}

pub async fn start_object_checkpoint_commit_task<S>(
    state: S,
    metrics: IndexerMetrics,
    config: IndexerConfig,
    object_indexing_receiver: mysten_metrics::metered_channel::Receiver<(
        sui_types::messages_checkpoint::CheckpointSequenceNumber,
        Vec<crate::store::TransactionObjectChanges>,
    )>,
) where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    use futures::StreamExt;

    info!("Indexer object checkpoint commit task started...");
    let checkpoint_commit_batch_size = std::env::var("CHECKPOINT_COMMIT_BATCH_SIZE")
        .unwrap_or(5.to_string())
        .parse::<usize>()
        .unwrap();

    let mut stream = mysten_metrics::metered_channel::ReceiverStream::new(object_indexing_receiver)
        .ready_chunks(checkpoint_commit_batch_size);

    while let Some(object_change_batch) = stream.next().await {
        let last_checkpoint_seq = object_change_batch.last().map(|b| b.0).unwrap();
        let first_checkpoint_seq = object_change_batch.first().map(|b| b.0).unwrap();

        if config.skip_db_commit {
            info!(
                "[Object] Downloaded and indexed checkpoint {:?} - {:?} successfully, skipping DB commit...",
                last_checkpoint_seq,
                first_checkpoint_seq,
            );
            continue;
        }

        // NOTE: commit object changes in the current task to stick to the original order,
        // spawned tasks are possible to be executed in a different order.
        let object_changes = object_change_batch
            .into_iter()
            .flat_map(|(_, o)| o)
            .collect::<Vec<_>>();
        let object_commit_timer = metrics.object_db_commit_latency.start_timer();
        let mut object_changes_commit_res = state
            .persist_object_changes(
                &object_changes,
                metrics.object_mutation_db_commit_latency.clone(),
                metrics.object_deletion_db_commit_latency.clone(),
                metrics.total_object_change_chunk_committed.clone(),
            )
            .await;
        while let Err(e) = object_changes_commit_res {
            warn!(
                "Indexer object changes commit failed (checkpoints [{:?}, {:?}]) with error: {:?}, retrying after {:?} milli-secs...",
                first_checkpoint_seq, last_checkpoint_seq, e, DB_COMMIT_RETRY_INTERVAL_IN_MILLIS
            );
            tokio::time::sleep(std::time::Duration::from_millis(
                DB_COMMIT_RETRY_INTERVAL_IN_MILLIS,
            ))
            .await;
            object_changes_commit_res = state
                .persist_object_changes(
                    &object_changes,
                    metrics.object_mutation_db_commit_latency.clone(),
                    metrics.object_deletion_db_commit_latency.clone(),
                    metrics.total_object_change_chunk_committed.clone(),
                )
                .await;
        }
        let elapsed = object_commit_timer.stop_and_record();
        metrics.total_object_checkpoint_committed.inc();
        metrics
            .total_object_change_committed
            .inc_by(object_changes.len() as u64);
        metrics
            .latest_indexer_object_checkpoint_sequence_number
            .set(last_checkpoint_seq as i64);
        info!(
            elapsed,
            "Object Checkpoint {}-{} committed with {} object changes",
            first_checkpoint_seq,
            last_checkpoint_seq,
            object_changes.len(),
        );
    }
}

pub struct ObjectsProcessor<S> {
    metrics: IndexerMetrics,
    object_indexing_sender: mysten_metrics::metered_channel::Sender<(
        sui_types::messages_checkpoint::CheckpointSequenceNumber,
        Vec<TransactionObjectChanges>,
    )>,
    state: S,
}

#[async_trait::async_trait]
impl<S> Handler for ObjectsProcessor<S>
where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    fn name(&self) -> &str {
        "objects-indexer"
    }
    async fn process_checkpoint(&mut self, checkpoint_data: &CheckpointData) -> anyhow::Result<()> {
        let checkpoint_seq = *checkpoint_data.checkpoint_summary.sequence_number();
        info!(checkpoint_seq, "Objects received by indexing processor");
        // Index checkpoint data
        let index_timer = self.metrics.checkpoint_index_latency.start_timer();

        let object_changes =
            Self::index_checkpoint_objects(self.state.clone(), checkpoint_data).await;
        index_timer.stop_and_record();

        self.object_indexing_sender
            .send((checkpoint_seq, object_changes))
            .await
            .tap_ok(|_| info!(checkpoint_seq, "Objects sent to commit handler"))
            .unwrap_or_else(|e| {
                panic!(
                    "checkpoint channel send should not fail, but got error: {:?}",
                    e
                )
            });
        Ok(())
    }
}

impl<S> ObjectsProcessor<S>
where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    async fn index_checkpoint_objects(
        packages_handler: S,
        data: &CheckpointData,
    ) -> Vec<TransactionObjectChanges> {
        // Index packages
        let packages = Self::index_packages(data);
        spawn_monitored_task!(async move {
            let mut package_commit_res = packages_handler.persist_packages(&packages).await;
            while let Err(e) = package_commit_res {
                warn!(
                    "Indexer package commit failed with error: {:?}, retrying after {:?} milli-secs...",
                    e, DB_COMMIT_RETRY_INTERVAL_IN_MILLIS
                );
                tokio::time::sleep(std::time::Duration::from_millis(
                    DB_COMMIT_RETRY_INTERVAL_IN_MILLIS,
                ))
                .await;
                package_commit_res = packages_handler.persist_packages(&packages).await;
            }
        });

        // Index objects
        let epoch = data.checkpoint_summary.epoch();
        let checkpoint = *data.checkpoint_summary.sequence_number();
        let objects: HashMap<_, _> = data
            .transactions
            .iter()
            .flat_map(|tx| tx.output_objects.iter())
            .map(|o| ((o.id(), o.version()), o))
            .collect();

        data.transactions
            .iter()
            .map(|tx| {
                let changed_objects = tx
                    .effects
                    .all_changed_objects()
                    .into_iter()
                    .map(|(oref, _owner, kind)| {
                        let object = objects.get(&(oref.0, oref.1)).unwrap();
                        crate::models::objects::Object::new(epoch, checkpoint, kind, object)
                    })
                    .collect();

                let deleted_objects = get_deleted_db_objects(&tx.effects, epoch, checkpoint);

                TransactionObjectChanges {
                    changed_objects,
                    deleted_objects,
                }
            })
            .collect()
    }

    fn index_packages(checkpoint_data: &CheckpointData) -> Vec<Package> {
        let senders: HashMap<_, _> = checkpoint_data
            .transactions
            .iter()
            .map(|tx| (tx.transaction.digest(), tx.transaction.sender_address()))
            .collect();

        checkpoint_data
            .transactions
            .iter()
            .flat_map(|tx| tx.output_objects.iter())
            .filter_map(|o| {
                if let sui_types::object::Data::Package(p) = &o.data {
                    let sender = senders
                        .get(&o.previous_transaction)
                        .expect("transaction for this object should be present");
                    Some(Package::new(*sender, p))
                } else {
                    None
                }
            })
            .collect()
    }
}

pub fn get_deleted_db_objects(
    effects: &TransactionEffects,
    epoch: EpochId,
    checkpoint: CheckpointSequenceNumber,
) -> Vec<DeletedObject> {
    let deleted = effects
        .deleted()
        .into_iter()
        .map(|o| (ObjectStatus::Deleted, o));
    let wrapped = effects
        .wrapped()
        .into_iter()
        .map(|o| (ObjectStatus::Wrapped, o));
    let unwrapped_then_deleted = effects
        .unwrapped_then_deleted()
        .into_iter()
        .map(|o| (ObjectStatus::UnwrappedThenDeleted, o));
    deleted
        .chain(wrapped)
        .chain(unwrapped_then_deleted)
        .map(|(status, oref)| {
            DeletedObject::from(
                epoch,
                Some(checkpoint),
                &sui_json_rpc_types::SuiObjectRef::from(oref.to_owned()),
                effects.transaction_digest(),
                &status,
            )
        })
        .collect::<Vec<_>>()
}
