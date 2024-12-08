// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use itertools::Itertools;
use sui_types::dynamic_field::DynamicFieldInfo;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use move_core_types::language_storage::{StructTag, TypeTag};
use mysten_metrics::{get_metrics, spawn_monitored_task};
use sui_data_ingestion_core::Worker;
use sui_rpc_api::{CheckpointData, CheckpointTransaction};
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::effects::{ObjectChange, TransactionEffectsAPI};
use sui_types::event::SystemEpochInfoEvent;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSequenceNumber,
};
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::sui_system_state::{get_sui_system_state, SuiSystemStateTrait};
use sui_types::transaction::TransactionDataAPI;

use crate::errors::IndexerError;
use crate::handlers::committer::start_tx_checkpoint_commit_task;
use crate::metrics::IndexerMetrics;
use crate::models::display::StoredDisplay;
use crate::models::epoch::{EndOfEpochUpdate, EpochEndInfo, EpochStartInfo, StartOfEpochUpdate};
use crate::models::obj_indices::StoredObjectVersion;
use crate::store::{IndexerStore, PgIndexerStore};
use crate::types::{
    EventIndex, IndexedCheckpoint, IndexedDeletedObject, IndexedEvent, IndexedObject,
    IndexedPackage, IndexedTransaction, IndexerResult, TransactionKind, TxIndex,
};

use super::tx_processor::EpochEndIndexingObjectStore;
use super::tx_processor::TxChangesProcessor;
use super::CheckpointDataToCommit;
use super::EpochToCommit;
use super::TransactionObjectChangesToCommit;

const CHECKPOINT_QUEUE_SIZE: usize = 100;

pub async fn new_handlers(
    state: PgIndexerStore,
    metrics: IndexerMetrics,
    cancel: CancellationToken,
    start_checkpoint_opt: Option<CheckpointSequenceNumber>,
    end_checkpoint_opt: Option<CheckpointSequenceNumber>,
    mvr_mode: bool,
) -> Result<(CheckpointHandler, u64), IndexerError> {
    let start_checkpoint = match start_checkpoint_opt {
        Some(start_checkpoint) => start_checkpoint,
        None => state
            .get_latest_checkpoint_sequence_number()
            .await?
            .map(|seq| seq.saturating_add(1))
            .unwrap_or_default(),
    };

    let checkpoint_queue_size = std::env::var("CHECKPOINT_QUEUE_SIZE")
        .unwrap_or(CHECKPOINT_QUEUE_SIZE.to_string())
        .parse::<usize>()
        .unwrap();
    let global_metrics = get_metrics().unwrap();
    let (indexed_checkpoint_sender, indexed_checkpoint_receiver) =
        mysten_metrics::metered_channel::channel(
            checkpoint_queue_size,
            &global_metrics
                .channel_inflight
                .with_label_values(&["checkpoint_indexing"]),
        );

    let state_clone = state.clone();
    let metrics_clone = metrics.clone();
    spawn_monitored_task!(start_tx_checkpoint_commit_task(
        state_clone,
        metrics_clone,
        indexed_checkpoint_receiver,
        cancel.clone(),
        start_checkpoint,
        end_checkpoint_opt,
        mvr_mode
    ));
    Ok((
        CheckpointHandler::new(state, metrics, indexed_checkpoint_sender),
        start_checkpoint,
    ))
}

pub struct CheckpointHandler {
    state: PgIndexerStore,
    metrics: IndexerMetrics,
    indexed_checkpoint_sender: mysten_metrics::metered_channel::Sender<CheckpointDataToCommit>,
}

#[async_trait]
impl Worker for CheckpointHandler {
    type Result = ();
    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> anyhow::Result<()> {
        let time_now_ms = chrono::Utc::now().timestamp_millis();
        let cp_download_lag = time_now_ms - checkpoint.checkpoint_summary.timestamp_ms as i64;
        info!(
            "checkpoint download lag for cp {}: {} ms",
            checkpoint.checkpoint_summary.sequence_number, cp_download_lag
        );
        self.metrics.download_lag_ms.set(cp_download_lag);
        self.metrics
            .max_downloaded_checkpoint_sequence_number
            .set(checkpoint.checkpoint_summary.sequence_number as i64);
        self.metrics
            .downloaded_checkpoint_timestamp_ms
            .set(checkpoint.checkpoint_summary.timestamp_ms as i64);
        info!(
            "Indexer lag: downloaded checkpoint {} with time now {} and checkpoint time {}",
            checkpoint.checkpoint_summary.sequence_number,
            time_now_ms,
            checkpoint.checkpoint_summary.timestamp_ms
        );
        let checkpoint_data = Self::index_checkpoint(
            &self.state,
            checkpoint,
            Arc::new(self.metrics.clone()),
            Self::index_packages(std::slice::from_ref(checkpoint), &self.metrics),
        )
        .await?;
        self.indexed_checkpoint_sender.send(checkpoint_data).await?;
        Ok(())
    }
}

impl CheckpointHandler {
    fn new(
        state: PgIndexerStore,
        metrics: IndexerMetrics,
        indexed_checkpoint_sender: mysten_metrics::metered_channel::Sender<CheckpointDataToCommit>,
    ) -> Self {
        Self {
            state,
            metrics,
            indexed_checkpoint_sender,
        }
    }

    async fn index_epoch(
        state: &PgIndexerStore,
        data: &CheckpointData,
    ) -> Result<Option<EpochToCommit>, IndexerError> {
        let checkpoint_object_store = EpochEndIndexingObjectStore::new(data);

        let CheckpointData {
            transactions,
            checkpoint_summary,
            checkpoint_contents: _,
        } = data;

        // Genesis epoch
        if *checkpoint_summary.sequence_number() == 0 {
            info!("Processing genesis epoch");
            let system_state_summary =
                get_sui_system_state(&checkpoint_object_store)?.into_sui_system_state_summary();
            return Ok(Some(EpochToCommit {
                last_epoch: None,
                new_epoch: StartOfEpochUpdate::new(system_state_summary, EpochStartInfo::default()),
            }));
        }

        // If not end of epoch, return
        if checkpoint_summary.end_of_epoch_data.is_none() {
            return Ok(None);
        }

        let system_state_summary =
            get_sui_system_state(&checkpoint_object_store)?.into_sui_system_state_summary();

        let epoch_event_opt = transactions
            .iter()
            .find_map(|t| {
                t.events.as_ref()?.data.iter().find_map(|ev| {
                    if ev.is_system_epoch_info_event() {
                        Some(bcs::from_bytes::<SystemEpochInfoEvent>(&ev.contents))
                    } else {
                        None
                    }
                })
            })
            .transpose()?;
        if epoch_event_opt.is_none() {
            warn!(
                "No SystemEpochInfoEvent found at end of epoch {}, some epoch data will be set to default.",
                checkpoint_summary.epoch,
            );
            assert!(
                system_state_summary.safe_mode,
                "Sui is not in safe mode but no SystemEpochInfoEvent found at end of epoch {}",
                checkpoint_summary.epoch
            );
        }

        // At some point while committing data in epoch X - 1, we will encounter a new epoch X. We
        // want to retrieve X - 2's network total transactions to calculate the number of
        // transactions that occurred in epoch X - 1.
        let first_tx_sequence_number = match system_state_summary.epoch {
            // If first epoch change, this number is 0
            1 => Ok(0),
            _ => {
                let last_epoch = system_state_summary.epoch - 2;
                state
                    .get_network_total_transactions_by_end_of_epoch(last_epoch)
                    .await?
                    .ok_or_else(|| {
                        IndexerError::PersistentStorageDataCorruptionError(format!(
                            "Network total transactions for epoch {} not found",
                            last_epoch
                        ))
                    })
            }
        }?;

        let epoch_end_info = EpochEndInfo::new(epoch_event_opt.as_ref());
        let epoch_start_info = EpochStartInfo::new(
            checkpoint_summary.sequence_number.saturating_add(1),
            checkpoint_summary.network_total_transactions,
            epoch_event_opt.as_ref(),
        );

        Ok(Some(EpochToCommit {
            last_epoch: Some(EndOfEpochUpdate::new(
                checkpoint_summary,
                first_tx_sequence_number,
                epoch_end_info,
            )),
            new_epoch: StartOfEpochUpdate::new(system_state_summary, epoch_start_info),
        }))
    }

    fn derive_object_versions(
        object_history_changes: &TransactionObjectChangesToCommit,
    ) -> Vec<StoredObjectVersion> {
        let mut object_versions = vec![];
        for changed_obj in object_history_changes.changed_objects.iter() {
            object_versions.push(StoredObjectVersion {
                object_id: changed_obj.object.id().to_vec(),
                object_version: changed_obj.object.version().value() as i64,
                cp_sequence_number: changed_obj.checkpoint_sequence_number as i64,
            });
        }
        for deleted_obj in object_history_changes.deleted_objects.iter() {
            object_versions.push(StoredObjectVersion {
                object_id: deleted_obj.object_id.to_vec(),
                object_version: deleted_obj.object_version as i64,
                cp_sequence_number: deleted_obj.checkpoint_sequence_number as i64,
            });
        }
        object_versions
    }

    async fn index_checkpoint(
        state: &PgIndexerStore,
        data: &CheckpointData,
        metrics: Arc<IndexerMetrics>,
        packages: Vec<IndexedPackage>,
    ) -> Result<CheckpointDataToCommit, IndexerError> {
        let checkpoint_seq = data.checkpoint_summary.sequence_number;
        info!(checkpoint_seq, "Indexing checkpoint data blob");

        // Index epoch
        let epoch = Self::index_epoch(state, data).await?;

        // Index Objects
        let object_changes: TransactionObjectChangesToCommit =
            Self::index_objects(data, &metrics).await?;
        let object_history_changes: TransactionObjectChangesToCommit =
            Self::index_objects_history(data).await?;
        let object_versions = Self::derive_object_versions(&object_history_changes);

        let (checkpoint, db_transactions, db_events, db_tx_indices, db_event_indices, db_displays) = {
            let CheckpointData {
                transactions,
                checkpoint_summary,
                checkpoint_contents,
            } = data;

            let (db_transactions, db_events, db_tx_indices, db_event_indices, db_displays) =
                Self::index_transactions(
                    transactions,
                    checkpoint_summary,
                    checkpoint_contents,
                    &metrics,
                )
                .await?;

            let successful_tx_num: u64 = db_transactions.iter().map(|t| t.successful_tx_num).sum();
            (
                IndexedCheckpoint::from_sui_checkpoint(
                    checkpoint_summary,
                    checkpoint_contents,
                    successful_tx_num as usize,
                ),
                db_transactions,
                db_events,
                db_tx_indices,
                db_event_indices,
                db_displays,
            )
        };
        let time_now_ms = chrono::Utc::now().timestamp_millis();
        metrics
            .index_lag_ms
            .set(time_now_ms - checkpoint.timestamp_ms as i64);
        metrics
            .max_indexed_checkpoint_sequence_number
            .set(checkpoint.sequence_number as i64);
        metrics
            .indexed_checkpoint_timestamp_ms
            .set(checkpoint.timestamp_ms as i64);
        info!(
            "Indexer lag: indexed checkpoint {} with time now {} and checkpoint time {}",
            checkpoint.sequence_number, time_now_ms, checkpoint.timestamp_ms
        );

        Ok(CheckpointDataToCommit {
            checkpoint,
            transactions: db_transactions,
            events: db_events,
            tx_indices: db_tx_indices,
            event_indices: db_event_indices,
            display_updates: db_displays,
            object_changes,
            object_history_changes,
            object_versions,
            packages,
            epoch,
        })
    }

    async fn index_transactions(
        transactions: &[CheckpointTransaction],
        checkpoint_summary: &CertifiedCheckpointSummary,
        checkpoint_contents: &CheckpointContents,
        metrics: &IndexerMetrics,
    ) -> IndexerResult<(
        Vec<IndexedTransaction>,
        Vec<IndexedEvent>,
        Vec<TxIndex>,
        Vec<EventIndex>,
        BTreeMap<String, StoredDisplay>,
    )> {
        let checkpoint_seq = checkpoint_summary.sequence_number();

        let mut tx_seq_num_iter = checkpoint_contents
            .enumerate_transactions(checkpoint_summary)
            .map(|(seq, execution_digest)| (execution_digest.transaction, seq));

        if checkpoint_contents.size() != transactions.len() {
            return Err(IndexerError::FullNodeReadingError(format!(
                "CheckpointContents has different size {} compared to Transactions {} for checkpoint {}",
                checkpoint_contents.size(),
                transactions.len(),
                checkpoint_seq
            )));
        }

        let mut db_transactions = Vec::new();
        let mut db_events = Vec::new();
        let mut db_displays = BTreeMap::new();
        let mut db_tx_indices = Vec::new();
        let mut db_event_indices = Vec::new();

        for tx in transactions {
            let CheckpointTransaction {
                transaction: sender_signed_data,
                effects: fx,
                events,
                input_objects,
                output_objects,
            } = tx;
            // Unwrap safe - we checked they have equal length above
            let (tx_digest, tx_sequence_number) = tx_seq_num_iter.next().unwrap();
            if tx_digest != *sender_signed_data.digest() {
                return Err(IndexerError::FullNodeReadingError(format!(
                    "Transactions has different ordering from CheckpointContents, for checkpoint {}, Mismatch found at {} v.s. {}",
                    checkpoint_seq, tx_digest, sender_signed_data.digest()
                )));
            }

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
                    *checkpoint_seq,
                    tx_digest,
                    event,
                    checkpoint_summary.timestamp_ms,
                )
            }));

            db_event_indices.extend(
                events.iter().enumerate().map(|(idx, event)| {
                    EventIndex::from_event(tx_sequence_number, idx as u64, event)
                }),
            );

            db_displays.extend(
                events
                    .iter()
                    .flat_map(StoredDisplay::try_from_event)
                    .map(|display| (display.object_type.clone(), display)),
            );

            let objects: Vec<_> = input_objects.iter().chain(output_objects.iter()).collect();

            let (balance_change, object_changes) =
                TxChangesProcessor::new(&objects, metrics.clone())
                    .get_changes(tx, fx, &tx_digest)
                    .await?;

            let db_txn = IndexedTransaction {
                tx_sequence_number,
                tx_digest,
                checkpoint_sequence_number: *checkpoint_summary.sequence_number(),
                timestamp_ms: checkpoint_summary.timestamp_ms,
                sender_signed_data: sender_signed_data.data().clone(),
                effects: fx.clone(),
                object_changes,
                balance_change,
                events,
                transaction_kind: transaction_kind.clone(),
                successful_tx_num: if fx.status().is_ok() {
                    tx.kind().tx_count() as u64
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
                .collect();

            // Changed Objects
            let changed_objects = fx
                .all_changed_objects()
                .into_iter()
                .map(|(object_ref, _owner, _write_kind)| object_ref.0)
                .collect();

            // Affected Objects
            let affected_objects = fx
                .object_changes()
                .into_iter()
                .map(|ObjectChange { id, .. }| id)
                .collect();

            // Payers
            let payers = vec![tx.gas_owner()];

            // Sender
            let sender = tx.sender();

            // Recipients
            let recipients = fx
                .all_changed_objects()
                .into_iter()
                .filter_map(|(_object_ref, owner, _write_kind)| match owner {
                    Owner::AddressOwner(address) => Some(address),
                    _ => None,
                })
                .unique()
                .collect();

            // Move Calls
            let move_calls = tx
                .move_calls()
                .into_iter()
                .map(|(p, m, f)| (*p, m.to_string(), f.to_string()))
                .collect();

            db_tx_indices.push(TxIndex {
                tx_sequence_number,
                transaction_digest: tx_digest,
                checkpoint_sequence_number: *checkpoint_seq,
                input_objects,
                changed_objects,
                affected_objects,
                sender,
                payers,
                recipients,
                move_calls,
                tx_kind: transaction_kind,
            });
        }
        Ok((
            db_transactions,
            db_events,
            db_tx_indices,
            db_event_indices,
            db_displays,
        ))
    }

    pub(crate) async fn index_objects(
        data: &CheckpointData,
        metrics: &IndexerMetrics,
    ) -> Result<TransactionObjectChangesToCommit, IndexerError> {
        let _timer = metrics.indexing_objects_latency.start_timer();
        let checkpoint_seq = data.checkpoint_summary.sequence_number;

        let eventually_removed_object_refs_post_version =
            data.eventually_removed_object_refs_post_version();
        let indexed_eventually_removed_objects = eventually_removed_object_refs_post_version
            .into_iter()
            .map(|obj_ref| IndexedDeletedObject {
                object_id: obj_ref.0,
                object_version: obj_ref.1.into(),
                checkpoint_sequence_number: checkpoint_seq,
            })
            .collect();

        let latest_live_output_objects = data.latest_live_output_objects();
        let changed_objects = latest_live_output_objects
            .into_iter()
            .map(|o| {
                try_extract_df_kind(o)
                    .map(|df_kind| IndexedObject::from_object(checkpoint_seq, o.clone(), df_kind))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(TransactionObjectChangesToCommit {
            changed_objects,
            deleted_objects: indexed_eventually_removed_objects,
        })
    }

    // similar to index_objects, but objects_history keeps all versions of objects
    async fn index_objects_history(
        data: &CheckpointData,
    ) -> Result<TransactionObjectChangesToCommit, IndexerError> {
        let checkpoint_seq = data.checkpoint_summary.sequence_number;
        let deleted_objects = data
            .transactions
            .iter()
            .flat_map(|tx| tx.removed_object_refs_post_version())
            .collect::<Vec<_>>();
        let indexed_deleted_objects: Vec<IndexedDeletedObject> = deleted_objects
            .into_iter()
            .map(|obj_ref| IndexedDeletedObject {
                object_id: obj_ref.0,
                object_version: obj_ref.1.into(),
                checkpoint_sequence_number: checkpoint_seq,
            })
            .collect();

        let output_objects: Vec<_> = data
            .transactions
            .iter()
            .flat_map(|tx| &tx.output_objects)
            .collect();

        // TODO(gegaowp): the current df_info implementation is not correct,
        // but we have decided remove all df_* except df_kind.
        let changed_objects = output_objects
            .into_iter()
            .map(|o| {
                try_extract_df_kind(o)
                    .map(|df_kind| IndexedObject::from_object(checkpoint_seq, o.clone(), df_kind))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(TransactionObjectChangesToCommit {
            changed_objects,
            deleted_objects: indexed_deleted_objects,
        })
    }

    fn index_packages(
        checkpoint_data: &[CheckpointData],
        metrics: &IndexerMetrics,
    ) -> Vec<IndexedPackage> {
        let _timer = metrics.indexing_packages_latency.start_timer();
        checkpoint_data
            .iter()
            .flat_map(|data| {
                let checkpoint_sequence_number = data.checkpoint_summary.sequence_number;
                data.transactions
                    .iter()
                    .flat_map(|tx| &tx.output_objects)
                    .filter_map(|o| {
                        if let sui_types::object::Data::Package(p) = &o.data {
                            Some(IndexedPackage {
                                package_id: o.id(),
                                move_package: p.clone(),
                                checkpoint_sequence_number,
                            })
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

/// If `o` is a dynamic `Field<K, V>`, determine whether it represents a Dynamic Field or a Dynamic
/// Object Field based on its type.
fn try_extract_df_kind(o: &Object) -> IndexerResult<Option<DynamicFieldType>> {
    // Skip if not a move object
    let Some(move_object) = o.data.try_as_move() else {
        return Ok(None);
    };

    if !move_object.type_().is_dynamic_field() {
        return Ok(None);
    }

    let type_: StructTag = move_object.type_().clone().into();
    let [name, _] = type_.type_params.as_slice() else {
        return Ok(None);
    };

    Ok(Some(
        if matches!(name, TypeTag::Struct(s) if DynamicFieldInfo::is_dynamic_object_field_wrapper(s))
        {
            DynamicFieldType::DynamicObject
        } else {
            DynamicFieldType::DynamicField
        },
    ))
}
