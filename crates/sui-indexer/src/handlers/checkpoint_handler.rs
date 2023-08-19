// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::object::Object;
use fastcrypto::traits::ToFromBytes;
use itertools::Itertools;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::ident_str;
use mysten_metrics::{get_metrics, spawn_monitored_task};
use sui_json_rpc_types::SuiMoveValue;
use sui_types::dynamic_field::DynamicFieldInfo;
use sui_types::dynamic_field::DynamicFieldName;
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::object::ObjectFormatOptions;
use std::collections::BTreeMap;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use sui_json_rpc::get_balance_changes_from_effect;
use sui_json_rpc::get_object_changes;
use sui_json_rpc::ObjectProvider;
use sui_json_rpc_types::BalanceChange;
use sui_json_rpc_types::EndOfEpochInfo;
use sui_json_rpc_types::ObjectChange;
use sui_json_rpc_types::TransactionFilter::InputObject;
use sui_rest_api::CheckpointData;
use sui_types::base_types::SequenceNumber;
use sui_types::committee::EpochId;
use sui_types::digests::TransactionDigest;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::object::ObjectRead;
use sui_types::object::Owner;
use sui_types::transaction::TransactionData;
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
use crate::models::epoch::{StoredEpochInfo, SystemEpochInfoEvent, IndexedEpochInfo};
use crate::models::events::Event;
use crate::models::events::IndexedEvent;
use crate::models::objects::{ObjectStatus, IndexedObject};
// use crate::models::packages::Package;
// use crate::models::transaction_index::ChangedObject;
// use crate::models::transaction_index::InputObject;
// use crate::models::transaction_index::MoveCall;
// use crate::models::transaction_index::Recipient;
use crate::models::transactions::IndexedTransaction;
use crate::models::tx_indices::TxIndex;
// use crate::models::transactions::Transaction;
use crate::models::transactions::TransactionKind;
use crate::schema::events::package;
use crate::schema::objects::checkpoint_sequence_number;
use crate::schema::transactions::balance_changes;
use crate::store::{
    IndexerStore, TemporaryCheckpointStore, TemporaryEpochStore, TransactionObjectChanges,
};
use crate::types::IndexerResult;
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
        checkpoint_starting_tx_seq_numbers: HashMap::new(),
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
    // Map from checkpoint sequence number and its starting transaction sequence number
    checkpoint_starting_tx_seq_numbers: HashMap<CheckpointSequenceNumber, u64>,
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
        let checkpoint_seq = checkpoint_data.checkpoint_summary.sequence_number();
        info!(checkpoint_seq, "Checkpoint received by indexing processor");

        self.checkpoint_starting_tx_seq_numbers.insert(
            *checkpoint_seq,
            checkpoint_data
                .checkpoint_summary
                .network_total_transactions
                + 1,
        );

        let current_checkpoint_starting_tx_seq = if self.checkpoint_starting_tx_seq_numbers.contains_key(checkpoint_seq) {
            *self.checkpoint_starting_tx_seq_numbers.get(checkpoint_seq)
        } else {
            self.state.get_checkpoint_ending_tx_sequence_number(checkpoint_seq - 1).await?
            .unwrap_or_else(|| {
                panic!("While processing checkpoint {}, we failed to find the starting tx seq both in mem and DB.", checkpoint_seq)
            }) as u64 + 1
        };

        // Index checkpoint data
        let index_timer = self.metrics.checkpoint_index_latency.start_timer();

        let (checkpoint, epoch) = Self::index_checkpoint_and_epoch(
            &self.state,
            current_checkpoint_starting_tx_seq,
            checkpoint_data,
        )
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
        info!(
            checkpoint_seq,
            elapsed, "Checkpoint indexing finished, about to sending to commit handler"
        );
        // NOTE: when the channel is full, checkpoint_sender_guard will wait until the channel has space.
        // Checkpoints are sent sequentially to stick to the order of checkpoint sequence numbers.
        self.checkpoint_sender
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

struct CheckpointDataObjectStore<'a> {
    objects: &'a [sui_types::object::Object],
}

impl<'a> sui_types::storage::ObjectStore for CheckpointDataObjectStore<'a> {
    fn get_object(
        &self,
        object_id: &ObjectID,
    ) -> Result<Option<sui_types::object::Object>, sui_types::error::SuiError> {
        Ok(self.objects.iter().find(|o| o.id() == *object_id).cloned())
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Result<Option<sui_types::object::Object>, sui_types::error::SuiError> {
        Ok(self
            .objects
            .iter()
            .find(|o| o.id() == *object_id && o.version() == version)
            .cloned())
    }
}

impl<S> CheckpointProcessor<S>
where
    S: IndexerStore + Clone + Sync + Send + 'static,
{
    // FIXME: This handler is problematic:
    // `get_sui_system_state` always returns the latest state
    async fn index_epoch(
        state: &S,
        data: &CheckpointData,
    ) -> Result<Option<TemporaryEpochStore>, IndexerError> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            checkpoint_contents: _,
            objects,
        } = data;

        let checkpoint_object_store = CheckpointDataObjectStore { objects };

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
                new_epoch: StoredEpochInfo {
                    epoch: 0,
                    first_checkpoint_id: 0,
                    epoch_start_timestamp: system_state.epoch_start_timestamp_ms as i64,
                    epoch_total_transactions: 0,
                    end_of_epoch_info: None,
                    validators: system_state
                        .active_validators
                        .into_iter()
                        .map(|v| bcs::to_bytes(&v))
                        .collect()?,
                    reference_gas_price: system_state.reference_gas_price as i64,
                    protocol_version: system_state.protocol_version as i64,
                },
                system_state: system_state.into(),
                validators,
            })
        } else if let Some(end_of_epoch_data) = &checkpoint_summary.end_of_epoch_data {
            let system_state = get_sui_system_state(&checkpoint_object_store)?;
            let system_state: SuiSystemStateSummary = system_state.into_sui_system_state_summary();

            let epoch_event = transactions
                .iter()
                .flat_map(|(_, _, events)| events.as_ref().map(|e| &e.data))
                .flatten()
                .find(|ev| {
                    ev.type_.address == SUI_SYSTEM_ADDRESS
                        && ev.type_.module.as_ident_str() == ident_str!("sui_system_state_inner")
                        && ev.type_.name.as_ident_str() == ident_str!("SystemEpochInfoEvent")
                })
                .unwrap_or_else(|| {
                    panic!(
                        "Can't find SystemEpochInfoEvent in epoch end checkpoint {}",
                        checkpoint_summary.sequence_number()
                    )
                });

            let event = bcs::from_bytes::<SystemEpochInfoEvent>(&epoch_event.contents)?;

            let validators = system_state
                .active_validators
                .iter()
                // .map(|v| (system_state.epoch, v.clone()).into())
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

            let last_epoch = system_state.epoch as i64 - 1;
            let network_tx_count_prev_epoch = state
                .get_network_total_transactions_previous_epoch(last_epoch)
                .await?;
            let end_of_epoch_info = EndOfEpochInfo {
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
            Some(TemporaryEpochStore {
                last_epoch: Some((
                    checkpoint_summary.network_total_transactions as i64
                        - network_tx_count_prev_epoch,
                    end_of_epoch_info)),
                new_epoch: IndexedEpochInfo {
                    epoch: system_state.epoch as i64,
                    validators, 
                    first_checkpoint_id: checkpoint_summary.sequence_number as i64 + 1,
                    epoch_start_timestamp: system_state.epoch_start_timestamp_ms as i64,
                    protocol_version: system_state.protocol_version,
                    reference_gas_price: system_state.reference_gas_price,
                    // To be filled by end of epoch
                    end_of_epoch_info: None,
                    epoch_total_transactions: 0,
                },
                // system_state: system_state.into(),
                // validators,
            })
        } else {
            None
        };

        Ok(epoch_index)
    }

    async fn index_checkpoint_and_epoch(
        state: &S,
        starting_tx_sequence_number: u64,
        data: &CheckpointData,
    ) -> Result<(TemporaryCheckpointStore, Option<TemporaryEpochStore>), IndexerError> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            checkpoint_contents,
            objects,
        } = data;

        let mut db_transactions = Vec::new();
        let mut db_events = Vec::new();
        // let mut db_input_objects = Vec::new();
        // let mut db_changed_objects = Vec::new();
        // let mut db_move_calls = Vec::new();
        // let mut db_recipients = Vec::new();
        let mut db_indices = Vec::new();

        for (idx, (tx, fx, events)) in transactions.into_iter().enumerate() {
            let tx_sequence_number = starting_tx_sequence_number + idx as u64;
            let tx_digest = tx.digest();
            let tx = tx.transaction_data();
            let events = events
                .as_ref()
                .map(|events| events.data.clone())
                .unwrap_or_default();
            // get_balance_changes_from_effect(
            //     &object_cache,
            //     tx.sender(),
            //     fx.modified_at_versions(),
            //     fx.all_changed_objects(),
            //     fx.all_removed_objects(),
            // )
            // .await?;
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

            let (balance_change, object_changes) = TxChangesProcessor::new(state, &objects)
                .get_changes(tx, fx, &tx_digest)
                .await?;

            let db_txn = IndexedTransaction {
                tx_sequence_number,
                tx_digest: *tx_digest,
                checkpoint_sequence_number: *checkpoint_summary.sequence_number(),
                timestamp_ms: checkpoint_summary.timestamp_ms,
                transaction: tx.clone(),
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

            // let input_objects = tx.input_objects()
            //     .expect("committed txns have been validated")
            //     .into_iter()
            //     .map(|obj_kind| obj_kind.object_id().to_vec())
            //     .collect::<Vec<_>>();

            // Changed Objects
            // let changed_objects = fx.all_changed_objects().into_iter().map(
            //     |(object_ref, _owner, _write_kind)|
            //         object_ref.0.to_vec()
            // ).collect::<Vec<_>>();
            let changed_objects = fx
                .all_changed_objects()
                .into_iter()
                .map(|(object_ref, _owner, _write_kind)| object_ref.0)
                .collect::<Vec<_>>();

            // Senders
            // let senders = vec![tx.sender().to_vec()];
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
            // let recipients =
            //     fx.all_changed_objects()
            //         .into_iter()
            //         .filter_map(|(_object_ref, owner, _write_kind)| match owner {
            //             Owner::AddressOwner(address) => Some(address.to_vec()),
            //             _ => None,
            //         })
            //         .unique()
            //         .collect::<Vec<_>>();

            // Move Calls
            let move_calls = tx
                .move_calls()
                .iter()
                .map(|(p, m, f)| (*p.clone(), m.to_string(), f.to_string()))
                .collect();
            // let move_calls = if let sui_types::transaction::TransactionKind::ProgrammableTransaction(pt) = tx.kind()
            // {
            //     pt.commands.iter().filter_map(move |command| {
            //         match command {
            //             sui_types::transaction::Command::MoveCall(m) => Some((
            //                 // m.package.to_vec(),
            //                 // format!("{}::{}", m.package.to_string(), m.module.to_string()),
            //                 // format!("{}::{}::{}", m.package.to_string(), m.module.to_string(), m.function.to_string()),
            //                 *m.clone()
            //             )),
            //             _ => None,
            //         }
            //     }).collect::<Vec<_>>()
            // } else {
            //     vec![]
            // };

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

        let epoch_index = Self::index_epoch(state, data).await?;

        let successful_tx_num: u64 = db_transactions.iter().map(|t| t.successful_tx_num).sum();

        Ok((
            TemporaryCheckpointStore {
                checkpoint: Checkpoint::from_sui_checkpoint(
                    checkpoint_summary,
                    checkpoint_contents,
                    successful_tx_num as i64,
                ),
                transactions: db_transactions,
                events: db_events,
                tx_indices: db_indices,
                // input_objects,
                // changed_objects,
                // move_calls,
                // recipients,
            },
            epoch_index,
        ))
    }
}

pub struct TxChangesProcessor<'a, S> {
    state: &'a S,
    updated_coin_objects: HashMap<(ObjectID, SequenceNumber), &'a sui_types::object::Object>,
    // input_coin_objects: HashMap<(ObjectID, SequenceNumber), &'a sui_types::object::Object>,
}

impl<'a, S> TxChangesProcessor<'a, S>
where
    S: IndexerStore + Clone + Sync + Send,
{
    pub fn new(state: &'a S, objects: &[sui_types::object::Object]) -> Self {
        let mut updated_coin_objects = HashMap::new();
        for obj in objects {
            if obj.is_coin() {
                updated_coin_objects.insert((obj.id(), obj.version()), obj);
            }
        }
        Self {
            state,
            updated_coin_objects,
        }
    }

    async fn get_changes(
        &self,
        tx: &TransactionData,
        effects: &TransactionEffects,
        tx_digest: &TransactionDigest,
    ) -> IndexerResult<(
        Vec<sui_json_rpc_types::BalanceChange>,
        Vec<sui_json_rpc_types::ObjectChange>,
    )> {
        let object_change = get_object_changes(
            self,
            tx.sender(),
            effects.modified_at_versions(),
            effects.all_changed_objects(),
            effects.all_removed_objects(),
        )
        .await?;
        let balance_change = get_balance_changes_from_effect(
            self,
            &effects,
            tx.input_objects().unwrap_or_else(|e| {
                panic!(
                    "Checkpointed tx {:?} has inavlid input objects: {e}",
                    tx_digest,
                )
            }),
            None,
        )
        .await?;
        Ok((balance_change, object_change))
    }
}

#[async_trait]
impl<'a, S> ObjectProvider for TxChangesProcessor<'a, S>
where
    S: IndexerStore + Clone + Sync + Send,
{
    type Error = IndexerError;

    async fn get_object(
        &self,
        id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<sui_types::object::Object, Self::Error> {
        let object = self.updated_coin_objects.get(&(*id, *version));
        if let Some(o) = object {
            return Ok(o.clone().clone());
        }

        let object = match self.state.get_object(*id, Some(*version)).await? {
            ObjectRead::Deleted(_) => {
                panic!(
                    "Object {} with version {} is found to be deleted",
                    id, version,
                );
            }
            ObjectRead::NotExists(_) => {
                panic!("Object {} with version {} does not exist", id, version,);
            }
            ObjectRead::Exists(_, object, _) => object,
        };
        Ok(object)
    }

    async fn find_object_lt_or_eq_version(
        &self,
        id: &ObjectID,
        version: &SequenceNumber,
    ) -> Result<Option<sui_types::object::Object>, Self::Error> {
        Ok(Some(self.get_object(id, version).await?))
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
                tx_indices,
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
                    .persist_transaction_index_tables(&tx_indices)
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
                        .persist_transaction_index_tables(&tx_indices)
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
        crate::store::TransactionObjectChanges,
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

        // FIXME: retry should happen in pg store.
        state
            .persist_object_changes(
                object_changes,
                metrics.object_mutation_db_commit_latency.clone(),
                metrics.object_deletion_db_commit_latency.clone(),
                metrics.total_object_change_chunk_committed.clone(),
            )
            .await?;
        // while let Err(e) = object_changes_commit_res {
        //     warn!(
        //         "Indexer object changes commit failed with error: {:?}, retrying after {:?} milli-secs...",
        //         e, DB_COMMIT_RETRY_INTERVAL_IN_MILLIS
        //     );
        //     tokio::time::sleep(std::time::Duration::from_millis(
        //         DB_COMMIT_RETRY_INTERVAL_IN_MILLIS,
        //     ))
        //     .await;
        //     object_changes_commit_res = state
        //         .persist_object_changes(
        //             object_changes,
        //             metrics.object_mutation_db_commit_latency.clone(),
        //             metrics.object_deletion_db_commit_latency.clone(),
        //             metrics.total_object_change_chunk_committed.clone(),
        //         )
        //         .await;
        // }
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
        TransactionObjectChanges,
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
            self.index_checkpoint_objects(checkpoint_data).await;
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
        &self,
        // packages_handler: S,
        data: &CheckpointData,
    ) -> TransactionObjectChanges {
        // // Index packages
        // let packages = Self::index_packages(data);
        // spawn_monitored_task!(async move {
        //     let mut package_commit_res = packages_handler.persist_packages(&packages).await;
        //     while let Err(e) = package_commit_res {
        //         warn!(
        //             "Indexer package commit failed with error: {:?}, retrying after {:?} milli-secs...",
        //             e, DB_COMMIT_RETRY_INTERVAL_IN_MILLIS
        //         );
        //         tokio::time::sleep(std::time::Duration::from_millis(
        //             DB_COMMIT_RETRY_INTERVAL_IN_MILLIS,
        //         ))
        //         .await;
        //         package_commit_res = packages_handler.persist_packages(&packages).await;
        //     }
        // });

        // Index objects
        let epoch = data.checkpoint_summary.epoch();
        let checkpoint_seq = *data.checkpoint_summary.sequence_number();

        let deleted_objects = data.transactions.iter().flat_map(|(_, fx, _)|
            get_deleted_objects(fx)
        ).collect::<Vec<_>>();

        let deleted_object_ids = deleted_objects.iter().map(|o| &(o.0, o.1)).collect::<HashSet<_>>();

        let (objects, discarded_versions) = Self::get_latest_objects(data.objects);

        let changed_objects = data.transactions
            .iter()
            .map(|(tx, fx, _)| {
                let changed_objects = fx
                    .all_changed_objects()
                    .into_iter()
                    .filter_map(|(oref, _owner, kind)| {
                        if discarded_versions.contains(&(oref.0, oref.1)) || deleted_object_ids.contains(&(oref.0, oref.1)){
                            return None;
                        }
                        let object = objects.get(&(oref.0)).unwrap_or_else(|| 
                            panic!("object {:?} not found in CheckpointData (tx_digest: {})", oref.0, tx.digest()));
                        assert_eq!(oref.1, object.version());
                        let module_cache = self.state.module_cache();
                        let df_info = Self::try_create_dynamic_field_info(object, &objects, module_cache)?;
                        Some(IndexedObject::from_object(checkpoint_seq, object, df_info))
                    })
                    .collect::<Vec<_>>();
                changed_objects
            })
            .collect();

        TransactionObjectChanges {
            changed_objects,
            deleted_objects,
        }
}

    // fn index_packages(checkpoint_data: &CheckpointData) -> Vec<Package> {
    //     let senders: HashMap<_, _> = checkpoint_data
    //         .transactions
    //         .iter()
    //         .map(|(tx, _, _)| (tx.digest(), tx.sender_address()))
    //         .collect();

    //     checkpoint_data
    //         .objects
    //         .iter()
    //         .filter_map(|o| {
    //             if let sui_types::object::Data::Package(p) = &o.data {
    //                 let sender = senders
    //                     .get(&o.previous_transaction)
    //                     .expect("transaction for this object should be present");
    //                 Some(Package::new(*sender, p))
    //             } else {
    //                 None
    //             }
    //         })
    //         .collect()
    // }

    pub fn get_latest_objects(objects: Vec<Object>) -> (HashMap<ObjectID, Object>, HashSet<(ObjectID, SequenceNumber)>) {
        let mut latest_objects = HashMap::new();
        let mut discarded_versions = HashSet::new();
        for object in objects {
            match latest_objects.entry(object.object_id.clone()) {
                Entry::Vacant(e) => {
                    e.insert(object);
                }
                Entry::Occupied(mut e) => {
                    if object.version > e.get().version {
                        discarded_versions.insert((e.get().object_id.clone(), e.get().version));
                        e.insert(object);
                    }
                }
            }
        }
        latest_objects.into_values().collect()
    }

    fn try_create_dynamic_field_info(
        o: &Object,
        written: &BTreeMap<ObjectID, Object>,
        resolver: &impl GetModule,
    ) -> IndexerResult<Option<DynamicFieldInfo>> {
        // Skip if not a move object
        let Some(move_object) = o.data.try_as_move().cloned() else {
            return Ok(None);
        };

        // FIXME <------------------------------------> EMXIF
        if !move_object.type_().is_dynamic_field() {
            return Ok(None);
        }
        // FIXME <------------------------------------> EMXIF

        let move_struct =
            move_object.to_move_struct_with_resolver(ObjectFormatOptions::default(), resolver)?;

        let (name_value, type_, object_id) =
            DynamicFieldInfo::parse_move_object(&move_struct).tap_err(|e| warn!("{e}"))?;

        let name_type = move_object.type_().try_extract_field_name(&type_)?;

        let bcs_name = bcs::to_bytes(&name_value.clone().undecorate()).map_err(|e| {
            IndexerError::SerdeError(
                format!("Failed to serialize dynamic field name {:?}: {e}", name_value),
            )})?;

        let name = DynamicFieldName {
            type_: name_type,
            value: SuiMoveValue::from(name_value).to_json_value(),
        };
        Ok(Some(match type_ {
            DynamicFieldType::DynamicObject => {
                let object = written.get(&object_id).ok_or(
                    IndexerError::UncategorizedError(
                        anyhow::anyhow!("Failed to find object_id {:?} when trying to create dynamic field info", object_id)
                    ))?;
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

}

pub fn get_deleted_objects(
    effects: &TransactionEffects,
) -> Vec<ObjectRef> {
    let deleted = effects
        .deleted()
        .into_iter();
    let wrapped = effects
        .wrapped()
        .into_iter();
    let unwrapped_then_deleted = effects
        .unwrapped_then_deleted()
        .into_iter();
    deleted
        .chain(wrapped)
        .chain(unwrapped_then_deleted)
        .collect::<Vec<_>>()
}