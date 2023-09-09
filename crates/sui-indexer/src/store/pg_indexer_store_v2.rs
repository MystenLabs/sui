// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use core::result::Result::Ok;
use itertools::Itertools;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use sui_types::dynamic_field::DynamicFieldName;
use tap::Tap;

use async_trait::async_trait;
use diesel::dsl::max;
use diesel::upsert::excluded;
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::{QueryDsl, RunQueryDsl};
use move_bytecode_utils::module_cache::SyncModuleCache;
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use sui_json_rpc_types::{Page, SuiObjectDataOptions, SuiObjectResponse};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::transaction::SenderSignedData;
use tracing::{info, Instrument};

use sui_json_rpc_types::{
    BalanceChange, DynamicFieldPage, ObjectChange, SuiObjectDataFilter, SuiTransactionBlock,
    SuiTransactionBlockEffects, SuiTransactionBlockEvents, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions,
};
use sui_json_rpc_types::{CheckpointId, EpochInfo, EventFilter, EventPage, SuiEvent};
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::committee::EpochId;
use sui_types::digests::{CheckpointDigest, TransactionDigest};
use sui_types::event::{Event, EventID};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::{Object, ObjectRead};

use crate::errors::{Context, IndexerError};
use crate::metrics::IndexerMetrics;

use crate::models_v2::checkpoints::StoredCheckpoint;
use crate::models_v2::epoch::{StoredEndOfEpochInfo, StoredEpochInfo};
use crate::models_v2::events::StoredEvent;
use crate::models_v2::objects::StoredObject;
use crate::models_v2::packages::StoredPackage;
use crate::models_v2::transactions::StoredTransaction;
use crate::models_v2::tx_indices::StoredTxIndex;
use crate::schema_v2::{checkpoints, epochs, events, objects, packages, transactions, tx_indices};
use crate::store::diesel_marco::{read_only_blocking, transactional_blocking_with_retry};
use crate::store::module_resolver_v2::IndexerModuleResolverV2;
use crate::types_v2::{
    IndexedCheckpoint, IndexedEvent, IndexedObject, IndexedObjectChange, IndexedPackage,
    IndexedTransaction, IndexerResult, OwnerType, TxIndex,
};
use crate::PgConnectionPool;

use super::{IndexerStoreV2, TemporaryEpochStoreV2, TransactionObjectChangesV2};

#[macro_export]
macro_rules! chunk {
    ($data: expr, $size: expr) => {{
        $data
            .into_iter()
            .chunks($size)
            .into_iter()
            .map(|c| c.collect())
            .collect::<Vec<Vec<_>>>()
    }};
}

// FIXME: consolidate these two?
const PG_COMMIT_CHUNK_SIZE: usize = 1000;
const PG_COMMIT_PARALLEL_CHUNK_SIZE: usize = 500;
const PG_COMMIT_OBJECTS_PARALLEL_CHUNK_SIZE: usize = 500;

#[derive(Clone)]
pub struct PgIndexerStoreV2 {
    blocking_cp: PgConnectionPool,
    module_cache: Arc<SyncModuleCache<IndexerModuleResolverV2>>,
    metrics: IndexerMetrics,
    parallel_chunk_size: usize,
    parallel_objects_chunk_size: usize,
}

impl PgIndexerStoreV2 {
    pub fn new(blocking_cp: PgConnectionPool, metrics: IndexerMetrics) -> Self {
        let module_cache: Arc<SyncModuleCache<IndexerModuleResolverV2>> = Arc::new(
            SyncModuleCache::new(IndexerModuleResolverV2::new(blocking_cp.clone())),
        );
        let parallel_chunk_size = std::env::var("PG_COMMIT_PARALLEL_CHUNK_SIZE")
            .unwrap_or_else(|e| PG_COMMIT_PARALLEL_CHUNK_SIZE.to_string())
            .parse::<usize>()
            .unwrap();
        let parallel_objects_chunk_size = std::env::var("PG_COMMIT_OBJECTS_PARALLEL_CHUNK_SIZE")
            .unwrap_or_else(|e| PG_COMMIT_OBJECTS_PARALLEL_CHUNK_SIZE.to_string())
            .parse::<usize>()
            .unwrap();
        Self {
            blocking_cp,
            module_cache,
            metrics,
            parallel_chunk_size,
            parallel_objects_chunk_size,
        }
    }

    fn get_latest_tx_checkpoint_sequence_number(&self) -> Result<Option<u64>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            checkpoints::dsl::checkpoints
                .select(max(checkpoints::sequence_number))
                .first::<Option<i64>>(conn)
                .map(|v| v.map(|v| v as u64))
        })
        .context("Failed reading latest checkpoint sequence number from PostgresDB")
    }

    fn get_checkpoint_ending_tx_sequence_number(
        &self,
        seq_num: CheckpointSequenceNumber,
    ) -> Result<Option<u64>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            checkpoints::dsl::checkpoints
                .select(checkpoints::network_total_transactions)
                .filter(checkpoints::sequence_number.eq(seq_num as i64))
                .first::<i64>(conn)
                .optional()
                .map(|v| v.map(|v| v as u64))
        })
        .context("Failed reading checkpoint end tx sequence number from PostgresDB")
    }

    fn get_object(
        &self,
        object_id: ObjectID,
        version: Option<SequenceNumber>,
    ) -> Result<Option<Object>, IndexerError> {
        // TOOD: read remote object_history kv store
        read_only_blocking!(&self.blocking_cp, |conn| {
            let query =
                objects::dsl::objects.filter(objects::dsl::object_id.eq(object_id.to_vec()));
            let boxed_query = if let Some(version) = version {
                query
                    .filter(objects::dsl::object_version.eq(version.value() as i64))
                    .into_boxed()
            } else {
                query.into_boxed()
            };
            match boxed_query.first::<StoredObject>(conn).optional()? {
                None => Ok(None),
                Some(obj) => Object::try_from(obj).map(Some),
            }
        })
        .context("Failed to read object from PostgresDB")
    }

    // Note: here we treat Deleted as NotExists too
    fn get_object_read(
        &self,
        object_id: ObjectID,
        version: Option<SequenceNumber>,
    ) -> Result<ObjectRead, IndexerError> {
        // TOOD: read remote object_history kv store
        read_only_blocking!(&self.blocking_cp, |conn| {
            let query =
                objects::dsl::objects.filter(objects::dsl::object_id.eq(object_id.to_vec()));
            let boxed_query = if let Some(version) = version {
                query
                    .filter(objects::dsl::object_version.eq(version.value() as i64))
                    .into_boxed()
            } else {
                query.into_boxed()
            };
            match boxed_query.first::<StoredObject>(conn).optional()? {
                None => Ok(ObjectRead::NotExists(object_id)),
                Some(obj) => obj.try_into_object_read(self.module_cache.as_ref()),
            }
        })
        .context("Failed to read object from PostgresDB")
    }

    // fn persist_objects_and_checkpoints(
    //     &self,
    //     object_changes: Vec<TransactionObjectChangesV2>,
    //     checkpoints: Vec<IndexedCheckpoint>,
    //     metrics: IndexerMetrics,
    // ) -> Result<(), IndexerError> {
    //     let guard = metrics
    //         .checkpoint_db_commit_latency_checkpoints_and_objects
    //         .start_timer();
    //     // If checkpoints is empty, object_changes must be empty too.
    //     if checkpoints.is_empty() {
    //         return Ok(());
    //     }

    //     let (mutated_objects, deleted_objects) = get_objects_to_commit(object_changes);
    //     let mutated_objects = mutated_objects
    //         .into_iter()
    //         .map(StoredObject::from)
    //         .collect::<Vec<_>>();

    //     let checkpoints = checkpoints
    //         .iter()
    //         .map(StoredCheckpoint::from)
    //         .collect::<Vec<_>>();
    //     transactional_blocking_with_retry!(
    //         &self.blocking_cp,
    //         |conn| {
    //             // Persist mutated objects
    //             for mutated_object_change_chunk in mutated_objects.chunks(PG_COMMIT_CHUNK_SIZE) {
    //                 diesel::insert_into(objects::table)
    //                     .values(mutated_object_change_chunk)
    //                     .on_conflict(objects::object_id)
    //                     .do_update()
    //                     .set((
    //                         objects::object_id.eq(excluded(objects::object_id)),
    //                         objects::object_version.eq(excluded(objects::object_version)),
    //                         objects::object_digest.eq(excluded(objects::object_digest)),
    //                         objects::checkpoint_sequence_number
    //                             .eq(excluded(objects::checkpoint_sequence_number)),
    //                         objects::owner_type.eq(excluded(objects::owner_type)),
    //                         objects::owner_id.eq(excluded(objects::owner_id)),
    //                         objects::serialized_object.eq(excluded(objects::serialized_object)),
    //                         objects::coin_type.eq(excluded(objects::coin_type)),
    //                         objects::coin_balance.eq(excluded(objects::coin_balance)),
    //                         objects::df_kind.eq(excluded(objects::df_kind)),
    //                         objects::df_name.eq(excluded(objects::df_name)),
    //                         objects::df_object_type.eq(excluded(objects::df_object_type)),
    //                         objects::df_object_id.eq(excluded(objects::df_object_id)),
    //                     ))
    //                     .execute(conn)
    //                     .map_err(IndexerError::from)
    //                     .context("Failed to write object mutation to PostgresDB")?;
    //             }

    //             // Persist deleted objects
    //             for deleted_objects_chunk in deleted_objects.chunks(PG_COMMIT_CHUNK_SIZE) {
    //                 diesel::delete(
    //                     objects::table.filter(
    //                         objects::object_id.eq_any(
    //                             deleted_objects_chunk
    //                                 .iter()
    //                                 .map(|o| o.to_vec())
    //                                 .collect::<Vec<_>>(),
    //                         ),
    //                     ),
    //                 )
    //                 .execute(conn)
    //                 .map_err(IndexerError::from)
    //                 .context("Failed to write object deletion to PostgresDB")?;
    //             }

    //             // Persist checkpoints
    //             for checkpoint_chunk in checkpoints.chunks(PG_COMMIT_CHUNK_SIZE) {
    //                 diesel::insert_into(checkpoints::table)
    //                     .values(checkpoint_chunk)
    //                     .on_conflict_do_nothing()
    //                     .execute(conn)
    //                     .map_err(IndexerError::from)
    //                     .context("Failed to write checkpoints to PostgresDB")?;
    //             }
    //             Ok::<(), IndexerError>(())
    //         },
    //         Duration::from_secs(60)
    //     )
    //     .tap(|_| {
    //         let elapsed = guard.stop_and_record();
    //         info!(
    //             elapsed,
    //             "Persisted {} objects and {} checkpoints",
    //             mutated_objects.len() + deleted_objects.len(),
    //             checkpoints.len()
    //         )
    //     })
    // }

    fn persist_objects_chunk(
        &self,
        objects: Vec<ObjectChangeToCommit>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        let guard = metrics
            .checkpoint_db_commit_latency_objects_chunks
            .start_timer();

        let mut mutated_objects = vec![];
        let mut deleted_object_ids = vec![];
        for object in objects {
            match object {
                ObjectChangeToCommit::MutatedObject(o) => {
                    mutated_objects.push(o);
                }
                ObjectChangeToCommit::DeletedObject(id) => {
                    deleted_object_ids.push(id);
                }
            }
        }

        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                // Persist mutated objects
                for mutated_object_change_chunk in mutated_objects.chunks(PG_COMMIT_CHUNK_SIZE) {
                    diesel::insert_into(objects::table)
                        .values(mutated_object_change_chunk)
                        .on_conflict(objects::object_id)
                        .do_update()
                        .set((
                            objects::object_id.eq(excluded(objects::object_id)),
                            objects::object_version.eq(excluded(objects::object_version)),
                            objects::object_digest.eq(excluded(objects::object_digest)),
                            objects::checkpoint_sequence_number
                                .eq(excluded(objects::checkpoint_sequence_number)),
                            objects::owner_type.eq(excluded(objects::owner_type)),
                            objects::owner_id.eq(excluded(objects::owner_id)),
                            objects::serialized_object.eq(excluded(objects::serialized_object)),
                            objects::coin_type.eq(excluded(objects::coin_type)),
                            objects::coin_balance.eq(excluded(objects::coin_balance)),
                            objects::df_kind.eq(excluded(objects::df_kind)),
                            objects::df_name.eq(excluded(objects::df_name)),
                            objects::df_object_type.eq(excluded(objects::df_object_type)),
                            objects::df_object_id.eq(excluded(objects::df_object_id)),
                        ))
                        .execute(conn)
                        .map_err(IndexerError::from)
                        .context("Failed to write object mutation to PostgresDB")?;
                }

                // Persist deleted objects
                for deleted_objects_chunk in deleted_object_ids.chunks(PG_COMMIT_CHUNK_SIZE) {
                    diesel::delete(
                        objects::table.filter(
                            objects::object_id.eq_any(
                                deleted_objects_chunk
                                    .iter()
                                    .map(|o| o.to_vec())
                                    .collect::<Vec<_>>(),
                            ),
                        ),
                    )
                    .execute(conn)
                    .map_err(IndexerError::from)
                    .context("Failed to write object deletion to PostgresDB")?;
                }

                Ok::<(), IndexerError>(())
            },
            Duration::from_secs(60)
        )
        .tap(|_| {
            let elapsed = guard.stop_and_record();
            info!(
                elapsed,
                "Persisted {} chunked objects",
                mutated_objects.len() + deleted_object_ids.len(),
            )
        })
    }

    fn persist_checkpoints(
        &self,
        checkpoints: Vec<IndexedCheckpoint>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        if checkpoints.is_empty() {
            return Ok(());
        }
        let guard = metrics
            .checkpoint_db_commit_latency_checkpoints
            .start_timer();

        let checkpoints = checkpoints
            .iter()
            .map(StoredCheckpoint::from)
            .collect::<Vec<_>>();
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                // Persist checkpoints
                for checkpoint_chunk in checkpoints.chunks(PG_COMMIT_CHUNK_SIZE) {
                    diesel::insert_into(checkpoints::table)
                        .values(checkpoint_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .map_err(IndexerError::from)
                        .context("Failed to write checkpoints to PostgresDB")?;
                }
                Ok::<(), IndexerError>(())
            },
            Duration::from_secs(60)
        )
        .tap(|_| {
            let elapsed = guard.stop_and_record();
            info!(elapsed, "Persisted {} checkpoints", checkpoints.len());
        })
    }

    fn persist_transactions_chunk(
        &self,
        transactions: Vec<IndexedTransaction>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        let guard = metrics
            .checkpoint_db_commit_latency_transactions_chunks
            .start_timer();
        let transformation_guard = metrics
            .checkpoint_db_commit_latency_transactions_chunks_transformation
            .start_timer();
        let transactions = transactions
            .iter()
            .map(StoredTransaction::from)
            .collect::<Vec<_>>();
        drop(transformation_guard);

        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                for transaction_chunk in transactions.chunks(PG_COMMIT_CHUNK_SIZE) {
                    diesel::insert_into(transactions::table)
                        .values(transaction_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .map_err(IndexerError::from)
                        .context("Failed to write transactions to PostgresDB")?;
                }
                Ok::<(), IndexerError>(())
            },
            Duration::from_secs(60)
        )
        .tap(|_| {
            let elapsed = guard.stop_and_record();
            info!(
                elapsed,
                "Persisted {} chunked transactions",
                transactions.len()
            )
        })
    }

    // fn persist_transactions(
    //     &self,
    //     transactions: Vec<IndexedTransaction>,
    //     metrics: IndexerMetrics,
    // ) -> Result<(), IndexerError> {
    //     let mut futures = vec![];
    //     for transaction_chunk in transactions.chunks(PG_COMMIT_CHUNK_SIZE) {
    //         futures.push(self.spawn_blocking(move |this| this.persist_transactions(transactions, metrics)))
    //     }
    //     futures::future::join_all(futures).await;
    // }

    fn persist_events_chunk(
        &self,
        events: Vec<IndexedEvent>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        let guard = metrics
            .checkpoint_db_commit_latency_events_chunks
            .start_timer();
        let events = events
            .into_iter()
            .map(StoredEvent::from)
            .collect::<Vec<_>>();
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                for event_chunk in events.chunks(PG_COMMIT_CHUNK_SIZE) {
                    diesel::insert_into(events::table)
                        .values(event_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .map_err(IndexerError::from)
                        .context("Failed to write events to PostgresDB")?;
                }
                Ok::<(), IndexerError>(())
            },
            Duration::from_secs(60)
        )
        .tap(|_| {
            let elapsed = guard.stop_and_record();
            info!(elapsed, "Persisted {} chunked events", events.len())
        })
    }

    fn persist_packages(
        &self,
        packages: Vec<IndexedPackage>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        if packages.is_empty() {
            return Ok(());
        }
        let guard = metrics.checkpoint_db_commit_latency_packages.start_timer();
        let packages = packages
            .into_iter()
            .map(StoredPackage::from)
            .collect::<Vec<_>>();
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                for packages_chunk in packages.chunks(PG_COMMIT_CHUNK_SIZE) {
                    diesel::insert_into(packages::table)
                        .values(packages_chunk)
                        // System packages such as 0x2/0x9 will have their package_id
                        // unchanged during upgrades. In this case, we override the modules
                        .on_conflict(packages::package_id)
                        .do_update()
                        .set((packages::modules.eq(excluded(packages::modules)),))
                        .execute(conn)
                        .map_err(IndexerError::from)
                        .context("Failed to write packages to PostgresDB")?;
                }
                Ok::<(), IndexerError>(())
            },
            Duration::from_secs(60)
        )
        .tap(|_| {
            let elapsed = guard.stop_and_record();
            info!(elapsed, "Persisted {} packages", packages.len())
        })
    }

    fn persist_tx_indices_chunk(
        &self,
        indices: Vec<TxIndex>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        let guard = metrics
            .checkpoint_db_commit_latency_tx_indices_chunks
            .start_timer();
        let indices = indices
            .into_iter()
            .map(StoredTxIndex::from)
            .collect::<Vec<_>>();
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                for indices_chunk in indices.chunks(PG_COMMIT_CHUNK_SIZE) {
                    diesel::insert_into(tx_indices::table)
                        .values(indices_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .map_err(IndexerError::from)
                        .context("Failed to write tx_indices to PostgresDB")?;
                }
                Ok::<(), IndexerError>(())
            },
            Duration::from_secs(60)
        )
        .tap(|_| {
            let elapsed = guard.stop_and_record();
            info!(elapsed, "Persisted {} chunked tx_indices", indices.len())
        })
    }

    fn get_network_total_transactions_previous_epoch(
        &self,
        epoch: u64,
    ) -> Result<u64, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            checkpoints::table
                .filter(checkpoints::epoch.eq(epoch as i64 - 1))
                .select(max(checkpoints::network_total_transactions))
                .first::<Option<i64>>(conn)
                .map(|o| o.unwrap_or(0))
        })
        .context("Failed to count network transactions in previous epoch")
        .map(|v| v as u64)
    }

    fn persist_epoch(
        &self,
        data: &TemporaryEpochStoreV2,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        let _scope = monitored_scope("pg_indexer_store_v2::persist_epoch");
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                if let Some(last_epoch) = &data.last_epoch {
                    let epoch_id = last_epoch.epoch;
                    info!("Updating epoch end data for epoch {}", epoch_id);
                    let last_epoch = StoredEndOfEpochInfo::from(last_epoch);
                    diesel::insert_into(epochs::table)
                        .values(last_epoch)
                        .on_conflict(epochs::epoch)
                        .do_update()
                        .set((
                            epochs::epoch_total_transactions
                                .eq(excluded(epochs::epoch_total_transactions)),
                            epochs::end_of_epoch_info.eq(excluded(epochs::end_of_epoch_info)),
                            epochs::end_of_epoch_data.eq(excluded(epochs::end_of_epoch_data)),
                        ))
                        .execute(conn)?;
                    info!("Updated epoch end data for epoch {}", epoch_id);
                }
                Ok::<(), IndexerError>(())
            },
            Duration::from_secs(60)
        )?;
        info!("Persisting initial state of epoch {}", data.new_epoch.epoch);
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                let new_epoch = StoredEpochInfo::from(&data.new_epoch);
                diesel::insert_into(epochs::table)
                    .values(new_epoch)
                    .on_conflict_do_nothing()
                    .execute(conn)
            },
            Duration::from_secs(60)
        )?;
        info!("Persisted initial state of epoch {}", data.new_epoch.epoch);
        Ok(())
    }

    fn get_epochs(
        &self,
        _cursor: Option<EpochId>,
        _limit: usize,
        _descending_order: Option<bool>,
    ) -> Result<Vec<EpochInfo>, IndexerError> {
        unimplemented!()
    }

    fn get_current_epoch(&self) -> Result<EpochInfo, IndexerError> {
        unimplemented!()
    }

    async fn execute_in_blocking_worker<F, R>(&self, f: F) -> Result<R, IndexerError>
    where
        F: FnOnce(Self) -> Result<R, IndexerError> + Send + 'static,
        R: Send + 'static,
    {
        let this = self.clone();
        let current_span = tracing::Span::current();
        tokio::task::spawn_blocking(move || {
            let _guard = current_span.enter();
            f(this)
        })
        .await
        .map_err(Into::into)
        .and_then(std::convert::identity)
    }

    fn spawn_blocking_task<F, R>(
        &self,
        f: F,
    ) -> tokio::task::JoinHandle<std::result::Result<R, IndexerError>>
    where
        F: FnOnce(Self) -> Result<R, IndexerError> + Send + 'static,
        R: Send + 'static,
    {
        let this = self.clone();
        let current_span = tracing::Span::current();
        tokio::task::spawn_blocking(move || {
            let _guard = current_span.enter();
            f(this)
        })
    }
}

#[async_trait]
impl IndexerStoreV2 for PgIndexerStoreV2 {
    type ModuleCache = SyncModuleCache<IndexerModuleResolverV2>;

    async fn get_latest_tx_checkpoint_sequence_number(&self) -> Result<Option<u64>, IndexerError> {
        self.execute_in_blocking_worker(|this| this.get_latest_tx_checkpoint_sequence_number())
            .await
    }

    async fn get_checkpoint_ending_tx_sequence_number(
        &self,
        seq_num: CheckpointSequenceNumber,
    ) -> Result<Option<u64>, IndexerError> {
        self.execute_in_blocking_worker(move |this| {
            this.get_checkpoint_ending_tx_sequence_number(seq_num)
        })
        .await
    }

    async fn get_checkpoint(
        &self,
        _id: CheckpointId,
    ) -> Result<sui_json_rpc_types::Checkpoint, IndexerError> {
        unimplemented!()
    }

    async fn get_checkpoints(
        &self,
        _cursor: Option<CheckpointId>,
        _limit: usize,
    ) -> Result<Vec<sui_json_rpc_types::Checkpoint>, IndexerError> {
        unimplemented!()
    }

    async fn get_checkpoint_sequence_number(
        &self,
        _digest: CheckpointDigest,
    ) -> Result<CheckpointSequenceNumber, IndexerError> {
        unimplemented!()
    }

    async fn get_event(&self, _id: EventID) -> Result<SuiEvent, IndexerError> {
        unimplemented!()
    }

    async fn get_events(
        &self,
        _query: EventFilter,
        _cursor: Option<EventID>,
        _limit: Option<usize>,
        _descending_order: bool,
    ) -> Result<EventPage, IndexerError> {
        unimplemented!()
    }

    async fn get_object_read(
        &self,
        object_id: ObjectID,
        version: Option<SequenceNumber>,
    ) -> Result<ObjectRead, IndexerError> {
        self.execute_in_blocking_worker(move |this| this.get_object_read(object_id, version))
            .await
    }

    async fn get_object(
        &self,
        object_id: ObjectID,
        version: Option<SequenceNumber>,
    ) -> Result<Option<Object>, IndexerError> {
        self.execute_in_blocking_worker(move |this| this.get_object(object_id, version))
            .await
    }

    async fn get_dynamic_fields(
        &self,
        parent_object_id: ObjectID,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> IndexerResult<DynamicFieldPage> {
        self.execute_in_blocking_worker(move |this| {
            let objects: Vec<StoredObject> = read_only_blocking!(&this.blocking_cp, |conn| {
                let mut query = objects::dsl::objects
                    .filter(objects::dsl::owner_type.eq(OwnerType::Object as i16))
                    .filter(objects::dsl::owner_id.eq(parent_object_id.to_vec()))
                    .limit((limit + 1) as i64)
                    .into_boxed();
                if let Some(object_cursor) = cursor {
                    query = query.filter(objects::dsl::object_id.gt(object_cursor.to_vec()));
                }
                query.load::<StoredObject>(conn)
            })
            .context("Failed to read stored objects from PostgresDB")?;
            let mut fields = objects
                .into_iter()
                .map(|object| object.try_into_dynamic_field_info())
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|info| {
                    info.ok_or(IndexerError::DynamicFieldError(format!(
                        "Unexpected failure to create dynamic field for parent_object_id: {}",
                        parent_object_id
                    )))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let next_cursor = fields.get(limit).map(|o| o.object_id);
            fields.truncate(limit);
            Ok(DynamicFieldPage {
                data: fields,
                next_cursor,
                has_next_page: next_cursor.is_some(),
            })
        })
        .await
    }

    async fn get_dynamic_field_object(
        &self,
        parent_object_id: ObjectID,
        name: DynamicFieldName,
    ) -> IndexerResult<SuiObjectResponse> {
        let object = self.execute_in_blocking_worker(move |this| {
            let bcs_name = bcs::to_bytes(&name).map_err(|e| {
                IndexerError::DynamicFieldError(format!(
                    "Failed to serialize dynamic field name: {}",
                    e
                ))
            })?;
            let object: Option<StoredObject> = read_only_blocking!(&this.blocking_cp, |conn| {
                objects::dsl::objects
                    .filter(objects::dsl::owner_type.eq(OwnerType::Object as i16))
                    .filter(objects::dsl::owner_id.eq(parent_object_id.to_vec()))
                    .filter(objects::dsl::df_name.eq(bcs_name.to_vec()))
                    .first::<StoredObject>(conn)
                    .optional()
            })
            .context("Failed to read stored objects from PostgresDB")?;
            Ok(object)
        })
        .await?;
        let object_read = if let Some(object) = object {
            object.try_into_object_read(&self.module_cache())
        } else {
            return Ok(SuiObjectResponse::new(None, None));
        }?;
        SuiObjectResponse::try_from((object_read, SuiObjectDataOptions::bcs_lossless()))
            .map_err(|e| {
                IndexerError::DynamicFieldError(format!(
                    "Failed to convert objectRead to SuiObjectResponse: {}",
                    e
                ))
            })
    }

    async fn get_total_transaction_number_from_checkpoints(&self) -> Result<i64, IndexerError> {
        unimplemented!()
    }

    async fn get_transaction_by_digest(
        &self,
        tx_digest: &str,
        options: Option<&SuiTransactionBlockResponseOptions>,
    ) -> Result<SuiTransactionBlockResponse, IndexerError> {
        let digest = TransactionDigest::from_str(tx_digest)
            .map_err(|e| IndexerError::InvalidTransactionDigestError(e.to_string()))?
            .into_inner()
            .to_vec();
        let stored_tx = read_only_blocking!(&self.blocking_cp, |conn| {
            transactions::dsl::transactions
                .filter(transactions::dsl::transaction_digest.eq(digest))
                .first::<StoredTransaction>(conn)
        })
        .context(&format!(
            "Failed reading transaction with digest {tx_digest}"
        ))?;
        self.compose_sui_transaction_block_response(stored_tx, options)
    }

    async fn multi_get_transactions_by_digests(
        &self,
        _tx_digests: &[String],
    ) -> Result<Vec<SuiTransactionBlockResponse>, IndexerError> {
        unimplemented!()
    }

    async fn get_owned_object(
        &self,
        address: SuiAddress,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> Result<Vec<ObjectRead>, IndexerError> {
        self.execute_in_blocking_worker(move |this| {
            let objects: Vec<StoredObject> = read_only_blocking!(&this.blocking_cp, |conn| {
                let mut query = objects::dsl::objects
                    .filter(objects::dsl::owner_type.eq(OwnerType::Address as i16))
                    .filter(objects::dsl::owner_id.eq(address.to_vec()))
                    .limit(limit as i64)
                    .into_boxed();
                if let Some(object_cursor) = cursor {
                    query = query.filter(objects::dsl::object_id.gt(object_cursor.to_vec()));
                }
                query.load::<StoredObject>(conn)
            })
            .context("Failed to read stored objects from PostgresDB")?;
            objects
                .into_iter()
                .map(|object| object.try_into_object_read(&this.module_cache))
                .collect::<Result<Vec<_>, _>>()
        })
        .await
    }

    async fn persist_objects(
        &self,
        object_changes: Vec<TransactionObjectChangesV2>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        if object_changes.is_empty() {
            return Ok(());
        }
        let guard = metrics.checkpoint_db_commit_latency_objects.start_timer();
        let objects = get_objects_to_commit(object_changes);
        let len = objects.len();
        let chunks = chunk!(objects, self.parallel_objects_chunk_size);
        let futures = chunks
            .into_iter()
            .map(|c| {
                let metrics_clone = metrics.clone();
                self.spawn_blocking_task(move |this| this.persist_objects_chunk(c, metrics_clone))
            })
            .collect::<Vec<_>>();

        futures::future::join_all(futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed to persist all objects chunks: {:?}",
                    e
                ))
            })?;
        let elapsed = guard.stop_and_record();
        info!(elapsed, "Persisted {} objects", len);
        Ok(())
    }

    async fn persist_checkpoints(
        &self,
        checkpoints: Vec<IndexedCheckpoint>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        self.execute_in_blocking_worker(move |this| this.persist_checkpoints(checkpoints, metrics))
            .await
    }

    // async fn persist_objects_and_checkpoints(
    //     &self,
    //     object_changes: Vec<TransactionObjectChangesV2>,
    //     checkpoints: Vec<IndexedCheckpoint>,
    //     metrics: IndexerMetrics,
    // ) -> Result<(), IndexerError> {
    //     self.execute_in_blocking_worker(move |this| {
    //         this.persist_objects_and_checkpoints(object_changes, checkpoints, metrics)
    //     })
    //     .await
    // }

    async fn persist_transactions(
        &self,
        transactions: Vec<IndexedTransaction>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        let guard = metrics
            .checkpoint_db_commit_latency_transactions
            .start_timer();
        let len = transactions.len();

        let chunks = chunk!(transactions, self.parallel_chunk_size);
        let futures = chunks
            .into_iter()
            .map(|c| {
                let metrics_clone = metrics.clone();
                self.spawn_blocking_task(move |this| {
                    this.persist_transactions_chunk(c, metrics_clone)
                })
            })
            .collect::<Vec<_>>();

        futures::future::join_all(futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed to persist all transactions chunks: {:?}",
                    e
                ))
            })?;
        let elapsed = guard.stop_and_record();
        info!(elapsed, "Persisted {} transactions", len);
        Ok(())
    }

    async fn persist_events(
        &self,
        events: Vec<IndexedEvent>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        if events.is_empty() {
            return Ok(());
        }
        let len = events.len();
        let guard = metrics.checkpoint_db_commit_latency_events.start_timer();
        let chunks = chunk!(events, self.parallel_chunk_size);
        let futures = chunks
            .into_iter()
            .map(|c| {
                let metrics_clone = metrics.clone();
                self.spawn_blocking_task(move |this| this.persist_events_chunk(c, metrics_clone))
            })
            .collect::<Vec<_>>();

        futures::future::join_all(futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed to persist all events chunks: {:?}",
                    e
                ))
            })?;
        let elapsed = guard.stop_and_record();
        info!(elapsed, "Persisted {} events", len);
        Ok(())
        // self.execute_in_blocking_worker(move |this| this.persist_events(events, metrics))
        //     .await
    }

    async fn persist_packages(
        &self,
        packages: Vec<IndexedPackage>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        if packages.is_empty() {
            return Ok(());
        }
        self.execute_in_blocking_worker(move |this| this.persist_packages(packages, metrics))
            .await
    }

    async fn persist_tx_indices(
        &self,
        indices: Vec<TxIndex>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        if indices.is_empty() {
            return Ok(());
        }
        let len = indices.len();
        let guard = metrics
            .checkpoint_db_commit_latency_tx_indices
            .start_timer();
        let chunks = chunk!(indices, self.parallel_chunk_size);

        let futures = chunks
            .into_iter()
            .map(|c| {
                let metrics_clone = metrics.clone();
                self.spawn_blocking_task(move |this| {
                    this.persist_tx_indices_chunk(c, metrics_clone)
                })
            })
            .collect::<Vec<_>>();
        futures::future::join_all(futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed to persist all tx_indices chunks: {:?}",
                    e
                ))
            })?;
        let elapsed = guard.stop_and_record();
        info!(elapsed, "Persisted {} tx_indices", len);
        Ok(())
    }

    async fn persist_epoch(
        &self,
        data: TemporaryEpochStoreV2,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        self.execute_in_blocking_worker(move |this| this.persist_epoch(&data, metrics))
            .await
    }

    async fn get_network_total_transactions_previous_epoch(
        &self,
        epoch: u64,
    ) -> Result<u64, IndexerError> {
        self.execute_in_blocking_worker(move |this| {
            this.get_network_total_transactions_previous_epoch(epoch)
        })
        .await
    }

    async fn get_epochs(
        &self,
        cursor: Option<EpochId>,
        limit: usize,
        descending_order: Option<bool>,
    ) -> Result<Vec<EpochInfo>, IndexerError> {
        self.execute_in_blocking_worker(move |this| {
            this.get_epochs(cursor, limit, descending_order)
        })
        .await
    }

    async fn get_current_epoch(&self) -> Result<EpochInfo, IndexerError> {
        self.execute_in_blocking_worker(move |this| this.get_current_epoch())
            .await
    }

    fn module_cache(&self) -> Arc<Self::ModuleCache> {
        self.module_cache.clone()
    }

    fn indexer_metrics(&self) -> &IndexerMetrics {
        &self.metrics
    }

    fn compose_sui_transaction_block_response(
        &self,
        tx: StoredTransaction,
        options: Option<&SuiTransactionBlockResponseOptions>,
    ) -> IndexerResult<SuiTransactionBlockResponse> {
        let tx_digest =
            TransactionDigest::try_from(tx.transaction_digest.as_slice()).map_err(|err| {
                IndexerError::DataTransformationError(format!(
                    "Failed to convert transaction digest to TransactionDigest: {}",
                    err
                ))
            })?;

        let sender_signed_data: SenderSignedData =
            bcs::from_bytes(&tx.raw_transaction).map_err(|err| {
                IndexerError::SerdeError(format!(
                    "Failed to deserialize sender signed data for tx: {:?}. Err: {}",
                    tx_digest, err
                ))
            })?;

        let mut response = SuiTransactionBlockResponse::new(tx_digest);
        let timestamp_ms = tx.timestamp_ms as u64;

        if let Some(options) = options {
            if options.show_balance_changes {
                let balance_changes: Vec<BalanceChange> = tx
                    .balance_changes
                    .into_iter()
                    .map(|bc| {
                        let bc = bc.ok_or(IndexerError::PersistentStorageDataCorruptionError(
                            "Stored Balance change bytes must not be None".to_string(),
                        ))?;
                        bcs::from_bytes(bc.as_slice()).map_err(|err| {
                            IndexerError::SerdeError(format!(
                                "Failed to deserialize balance change for tx: {:?}. Err: {}",
                                tx_digest, err
                            ))
                        })
                    })
                    .collect::<Result<Vec<BalanceChange>, IndexerError>>()?;
                response.balance_changes = Some(balance_changes);
            }
            if options.show_object_changes {
                let object_changes: Vec<ObjectChange> = tx
                    .object_changes
                    .into_iter()
                    .map(|oc| {
                        let oc = oc.ok_or(IndexerError::PersistentStorageDataCorruptionError(
                            "Stored Object change bytes must not be None".to_string(),
                        ))?;
                        // oc must be Some
                        let indexed_ob: IndexedObjectChange = bcs::from_bytes(oc.as_slice())
                            .map_err(|err| {
                                IndexerError::SerdeError(format!(
                                    "Failed to deserialize object change for tx: {:?}. Err: {}",
                                    tx_digest, err
                                ))
                            })?;
                        Ok(indexed_ob.into())
                    })
                    .collect::<Result<Vec<ObjectChange>, IndexerError>>()?;
                response.object_changes = Some(object_changes);
            }
            if options.show_events {
                let events: Vec<Event> = tx
                    .events
                    .into_iter()
                    .map(|e| {
                        let e = e.ok_or(IndexerError::PersistentStorageDataCorruptionError(
                            "Stored Event bytes must not be None".to_string(),
                        ))?;
                        bcs::from_bytes(e.as_slice()).map_err(|err| {
                            IndexerError::SerdeError(format!(
                                "Failed to deserialize event for tx: {:?}. Err: {}",
                                tx_digest, err
                            ))
                        })
                    })
                    .collect::<Result<Vec<Event>, IndexerError>>()?;
                let events = TransactionEvents { data: events };
                let events = SuiTransactionBlockEvents::try_from(
                    events,
                    tx_digest,
                    Some(timestamp_ms),
                    &self.module_cache,
                )?;
                response.events = Some(events);
            }
            if options.show_input {
                let transaction =
                    SuiTransactionBlock::try_from(sender_signed_data, &self.module_cache)?;
                response.transaction = Some(transaction);
            }
            if options.show_raw_input {
                response.raw_transaction = tx.raw_transaction;
            }
            if options.show_effects {
                let effects: TransactionEffects =
                    bcs::from_bytes(&tx.raw_effects).map_err(IndexerError::BcsError)?;
                let effects = SuiTransactionBlockEffects::try_from(effects)?;
                response.effects = Some(effects);
            }
        }
        Ok(response)
    }
}

fn get_objects_to_commit(
    tx_object_changes: Vec<TransactionObjectChangesV2>,
) -> Vec<ObjectChangeToCommit> {
    let deleted_objects = tx_object_changes
        .iter()
        .flat_map(|changes| &changes.deleted_objects)
        .map(|o| o.0)
        .into_iter()
        .map(ObjectChangeToCommit::DeletedObject)
        .collect::<Vec<_>>();

    let mutated_objects = tx_object_changes
        .into_iter()
        .flat_map(|changes| changes.changed_objects);
    let mut latest_objects = HashMap::new();
    for object in mutated_objects {
        match latest_objects.entry(object.object_id) {
            Entry::Vacant(e) => {
                e.insert(object);
            }
            Entry::Occupied(mut e) => {
                if object.object_version > e.get().object_version {
                    e.insert(object);
                }
            }
        }
    }
    deleted_objects
        .into_iter()
        .chain(
            latest_objects
                .into_values()
                .map(StoredObject::from)
                .map(ObjectChangeToCommit::MutatedObject),
        )
        .collect()

    // (
    //     latest_objects.into_values().collect(),
    //     deleted_changes.into_iter().collect(),
    // )
}

enum ObjectChangeToCommit {
    MutatedObject(StoredObject),
    DeletedObject(ObjectID),
}
