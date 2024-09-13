// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use std::time::Duration;

use async_trait::async_trait;
use core::result::Result::Ok;
use csv::Writer;
use diesel::dsl::{max, min};
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::QueryDsl;
use diesel_async::scoped_futures::ScopedFutureExt;
use itertools::Itertools;
use object_store::path::Path;
use tap::TapFallible;
use tracing::{info, warn};

use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_protocol_config::ProtocolConfig;
use sui_storage::object_store::util::put;
use sui_types::base_types::ObjectID;

use crate::config::RestoreConfig;
use crate::database::ConnectionPool;
use crate::errors::{Context, IndexerError};
use crate::handlers::EpochToCommit;
use crate::handlers::TransactionObjectChangesToCommit;
use crate::metrics::IndexerMetrics;
use crate::models::checkpoints::StoredChainIdentifier;
use crate::models::checkpoints::StoredCheckpoint;
use crate::models::checkpoints::StoredCpTx;
use crate::models::display::StoredDisplay;
use crate::models::epoch::StoredEpochInfo;
use crate::models::epoch::{StoredFeatureFlag, StoredProtocolConfig};
use crate::models::events::StoredEvent;
use crate::models::obj_indices::StoredObjectVersion;
use crate::models::objects::StoredFullHistoryObject;
use crate::models::objects::{
    StoredDeletedHistoryObject, StoredDeletedObject, StoredHistoryObject, StoredObject,
    StoredObjectSnapshot,
};
use crate::models::packages::StoredPackage;
use crate::models::transactions::StoredTransaction;
use crate::schema::{
    chain_identifier, checkpoints, display, epochs, event_emit_module, event_emit_package,
    event_senders, event_struct_instantiation, event_struct_module, event_struct_name,
    event_struct_package, events, feature_flags, full_objects_history, objects, objects_history,
    objects_snapshot, objects_version, packages, protocol_configs, pruner_cp_watermark,
    transactions, tx_affected_addresses, tx_calls_fun, tx_calls_mod, tx_calls_pkg,
    tx_changed_objects, tx_digests, tx_input_objects, tx_kinds, tx_recipients, tx_senders,
};
use crate::store::transaction_with_retry;
use crate::types::EventIndex;
use crate::types::{IndexedCheckpoint, IndexedEvent, IndexedPackage, IndexedTransaction, TxIndex};

use super::pg_partition_manager::{EpochPartitionData, PgPartitionManager};
use super::IndexerStore;
use super::ObjectChangeToCommit;

use diesel::upsert::excluded;
use sui_types::digests::{ChainIdentifier, CheckpointDigest};

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

// In one DB transaction, the update could be chunked into
// a few statements, this is the amount of rows to update in one statement
// TODO: I think with the `per_db_tx` params, `PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX`
// is now less relevant. We should do experiments and remove it if it's true.
const PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX: usize = 1000;
// The amount of rows to update in one DB transaction
const PG_COMMIT_PARALLEL_CHUNK_SIZE: usize = 100;
// The amount of rows to update in one DB transaction, for objects particularly
// Having this number too high may cause many db deadlocks because of
// optimistic locking.
const PG_COMMIT_OBJECTS_PARALLEL_CHUNK_SIZE: usize = 500;
const PG_DB_COMMIT_SLEEP_DURATION: Duration = Duration::from_secs(3600);

#[derive(Clone)]
pub struct PgIndexerStoreConfig {
    pub parallel_chunk_size: usize,
    pub parallel_objects_chunk_size: usize,
    pub gcs_cred_path: Option<String>,
    pub gcs_display_bucket: Option<String>,
}

#[derive(Clone)]
pub struct PgIndexerStore {
    pool: ConnectionPool,
    metrics: IndexerMetrics,
    partition_manager: PgPartitionManager,
    config: PgIndexerStoreConfig,
}

impl PgIndexerStore {
    pub fn new(
        pool: ConnectionPool,
        restore_config: RestoreConfig,
        metrics: IndexerMetrics,
    ) -> Self {
        let parallel_chunk_size = std::env::var("PG_COMMIT_PARALLEL_CHUNK_SIZE")
            .unwrap_or_else(|_e| PG_COMMIT_PARALLEL_CHUNK_SIZE.to_string())
            .parse::<usize>()
            .unwrap();
        let parallel_objects_chunk_size = std::env::var("PG_COMMIT_OBJECTS_PARALLEL_CHUNK_SIZE")
            .unwrap_or_else(|_e| PG_COMMIT_OBJECTS_PARALLEL_CHUNK_SIZE.to_string())
            .parse::<usize>()
            .unwrap();
        let partition_manager =
            PgPartitionManager::new(pool.clone()).expect("Failed to initialize partition manager");
        let config = PgIndexerStoreConfig {
            parallel_chunk_size,
            parallel_objects_chunk_size,
            gcs_cred_path: restore_config.gcs_cred_path,
            gcs_display_bucket: restore_config.gcs_display_bucket,
        };

        Self {
            pool,
            metrics,
            partition_manager,
            config,
        }
    }

    pub fn pool(&self) -> ConnectionPool {
        self.pool.clone()
    }

    /// Get the range of the protocol versions that need to be indexed.
    pub async fn get_protocol_version_index_range(&self) -> Result<(i64, i64), IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;
        // We start indexing from the next protocol version after the latest one stored in the db.
        let start = protocol_configs::table
            .select(max(protocol_configs::protocol_version))
            .first::<Option<i64>>(&mut connection)
            .await
            .map_err(Into::into)
            .context("Failed reading latest protocol version from PostgresDB")?
            .map_or(1, |v| v + 1);

        // We end indexing at the protocol version of the latest epoch stored in the db.
        let end = epochs::table
            .select(max(epochs::protocol_version))
            .first::<Option<i64>>(&mut connection)
            .await
            .map_err(Into::into)
            .context("Failed reading latest epoch protocol version from PostgresDB")?
            .unwrap_or(1);
        Ok((start, end))
    }

    async fn get_chain_identifier(&self) -> Result<Option<Vec<u8>>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        chain_identifier::table
            .select(chain_identifier::checkpoint_digest)
            .first::<Vec<u8>>(&mut connection)
            .await
            .optional()
            .map_err(Into::into)
            .context("Failed reading chain id from PostgresDB")
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<Option<u64>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        checkpoints::table
            .select(max(checkpoints::sequence_number))
            .first::<Option<i64>>(&mut connection)
            .await
            .map_err(Into::into)
            .map(|v| v.map(|v| v as u64))
            .context("Failed reading latest checkpoint sequence number from PostgresDB")
    }

    async fn get_available_checkpoint_range(&self) -> Result<(u64, u64), IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        checkpoints::table
            .select((
                min(checkpoints::sequence_number),
                max(checkpoints::sequence_number),
            ))
            .first::<(Option<i64>, Option<i64>)>(&mut connection)
            .await
            .map_err(Into::into)
            .map(|(min, max)| {
                (
                    min.unwrap_or_default() as u64,
                    max.unwrap_or_default() as u64,
                )
            })
            .context("Failed reading min and max checkpoint sequence numbers from PostgresDB")
    }

    async fn get_prunable_epoch_range(&self) -> Result<(u64, u64), IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        epochs::table
            .select((min(epochs::epoch), max(epochs::epoch)))
            .first::<(Option<i64>, Option<i64>)>(&mut connection)
            .await
            .map_err(Into::into)
            .map(|(min, max)| {
                (
                    min.unwrap_or_default() as u64,
                    max.unwrap_or_default() as u64,
                )
            })
            .context("Failed reading min and max epoch numbers from PostgresDB")
    }

    async fn get_min_prunable_checkpoint(&self) -> Result<u64, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        pruner_cp_watermark::table
            .select(min(pruner_cp_watermark::checkpoint_sequence_number))
            .first::<Option<i64>>(&mut connection)
            .await
            .map_err(Into::into)
            .map(|v| v.unwrap_or_default() as u64)
            .context("Failed reading min prunable checkpoint sequence number from PostgresDB")
    }

    async fn get_checkpoint_range_for_epoch(
        &self,
        epoch: u64,
    ) -> Result<(u64, Option<u64>), IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        epochs::table
            .select((epochs::first_checkpoint_id, epochs::last_checkpoint_id))
            .filter(epochs::epoch.eq(epoch as i64))
            .first::<(i64, Option<i64>)>(&mut connection)
            .await
            .map_err(Into::into)
            .map(|(min, max)| (min as u64, max.map(|v| v as u64)))
            .context("Failed reading checkpoint range from PostgresDB")
    }

    async fn get_transaction_range_for_checkpoint(
        &self,
        checkpoint: u64,
    ) -> Result<(u64, u64), IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        pruner_cp_watermark::table
            .select((
                pruner_cp_watermark::min_tx_sequence_number,
                pruner_cp_watermark::max_tx_sequence_number,
            ))
            .filter(pruner_cp_watermark::checkpoint_sequence_number.eq(checkpoint as i64))
            .first::<(i64, i64)>(&mut connection)
            .await
            .map_err(Into::into)
            .map(|(min, max)| (min as u64, max as u64))
            .context("Failed reading transaction range from PostgresDB")
    }

    async fn get_latest_object_snapshot_checkpoint_sequence_number(
        &self,
    ) -> Result<Option<u64>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        objects_snapshot::table
            .select(max(objects_snapshot::checkpoint_sequence_number))
            .first::<Option<i64>>(&mut connection)
            .await
            .map_err(Into::into)
            .map(|v| v.map(|v| v as u64))
            .context(
                "Failed reading latest object snapshot checkpoint sequence number from PostgresDB",
            )
    }

    async fn persist_display_updates(
        &self,
        display_updates: BTreeMap<String, StoredDisplay>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                diesel::insert_into(display::table)
                    .values(display_updates.values().collect::<Vec<_>>())
                    .on_conflict(display::object_type)
                    .do_update()
                    .set((
                        display::id.eq(excluded(display::id)),
                        display::version.eq(excluded(display::version)),
                        display::bcs.eq(excluded(display::bcs)),
                    ))
                    .execute(conn)
                    .await?;

                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await?;

        Ok(())
    }

    async fn persist_object_mutation_chunk(
        &self,
        mutated_object_mutation_chunk: Vec<StoredObject>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        let guard = self
            .metrics
            .checkpoint_db_commit_latency_objects_chunks
            .start_timer();
        let len = mutated_object_mutation_chunk.len();
        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                diesel::insert_into(objects::table)
                    .values(mutated_object_mutation_chunk.clone())
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
                        objects::object_type.eq(excluded(objects::object_type)),
                        objects::serialized_object.eq(excluded(objects::serialized_object)),
                        objects::coin_type.eq(excluded(objects::coin_type)),
                        objects::coin_balance.eq(excluded(objects::coin_balance)),
                        objects::df_kind.eq(excluded(objects::df_kind)),
                        objects::df_name.eq(excluded(objects::df_name)),
                        objects::df_object_type.eq(excluded(objects::df_object_type)),
                        objects::df_object_id.eq(excluded(objects::df_object_id)),
                    ))
                    .execute(conn)
                    .await?;
                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await
        .tap_ok(|_| {
            let elapsed = guard.stop_and_record();
            info!(elapsed, "Persisted {} chunked objects", len);
        })
        .tap_err(|e| {
            tracing::error!("Failed to persist object mutations with error: {}", e);
        })
    }

    async fn persist_object_deletion_chunk(
        &self,
        deleted_objects_chunk: Vec<StoredDeletedObject>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_objects_chunks
            .start_timer();
        let len = deleted_objects_chunk.len();
        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                diesel::delete(
                    objects::table.filter(
                        objects::object_id.eq_any(
                            deleted_objects_chunk
                                .iter()
                                .map(|o| o.object_id.clone())
                                .collect::<Vec<_>>(),
                        ),
                    ),
                )
                .execute(conn)
                .await
                .map_err(IndexerError::from)
                .context("Failed to write object deletion to PostgresDB")?;

                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await
        .tap_ok(|_| {
            let elapsed = guard.stop_and_record();
            info!(elapsed, "Deleted {} chunked objects", len);
        })
        .tap_err(|e| {
            tracing::error!("Failed to persist object deletions with error: {}", e);
        })
    }

    async fn backfill_objects_snapshot_chunk(
        &self,
        objects: Vec<ObjectChangeToCommit>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_objects_snapshot_chunks
            .start_timer();
        let mut objects_snapshot: Vec<StoredObjectSnapshot> = vec![];
        for object in objects {
            match object {
                ObjectChangeToCommit::MutatedObject(stored_object) => {
                    objects_snapshot.push(stored_object.into());
                }
                ObjectChangeToCommit::DeletedObject(stored_deleted_object) => {
                    objects_snapshot.push(stored_deleted_object.into());
                }
            }
        }

        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                for objects_snapshot_chunk in
                    objects_snapshot.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    diesel::insert_into(objects_snapshot::table)
                        .values(objects_snapshot_chunk)
                        .on_conflict(objects_snapshot::object_id)
                        .do_update()
                        .set((
                            objects_snapshot::object_version
                                .eq(excluded(objects_snapshot::object_version)),
                            objects_snapshot::object_status
                                .eq(excluded(objects_snapshot::object_status)),
                            objects_snapshot::object_digest
                                .eq(excluded(objects_snapshot::object_digest)),
                            objects_snapshot::checkpoint_sequence_number
                                .eq(excluded(objects_snapshot::checkpoint_sequence_number)),
                            objects_snapshot::owner_type.eq(excluded(objects_snapshot::owner_type)),
                            objects_snapshot::owner_id.eq(excluded(objects_snapshot::owner_id)),
                            objects_snapshot::object_type_package
                                .eq(excluded(objects_snapshot::object_type_package)),
                            objects_snapshot::object_type_module
                                .eq(excluded(objects_snapshot::object_type_module)),
                            objects_snapshot::object_type_name
                                .eq(excluded(objects_snapshot::object_type_name)),
                            objects_snapshot::object_type
                                .eq(excluded(objects_snapshot::object_type)),
                            objects_snapshot::serialized_object
                                .eq(excluded(objects_snapshot::serialized_object)),
                            objects_snapshot::coin_type.eq(excluded(objects_snapshot::coin_type)),
                            objects_snapshot::coin_balance
                                .eq(excluded(objects_snapshot::coin_balance)),
                            objects_snapshot::df_kind.eq(excluded(objects_snapshot::df_kind)),
                            objects_snapshot::df_name.eq(excluded(objects_snapshot::df_name)),
                            objects_snapshot::df_object_type
                                .eq(excluded(objects_snapshot::df_object_type)),
                            objects_snapshot::df_object_id
                                .eq(excluded(objects_snapshot::df_object_id)),
                        ))
                        .execute(conn)
                        .await?;
                }
                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await
        .tap_ok(|_| {
            let elapsed = guard.stop_and_record();
            info!(
                elapsed,
                "Persisted {} chunked objects snapshot",
                objects_snapshot.len(),
            );
        })
        .tap_err(|e| {
            tracing::error!("Failed to persist object snapshot with error: {}", e);
        })
    }

    async fn persist_objects_history_chunk(
        &self,
        objects: Vec<ObjectChangeToCommit>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_objects_history_chunks
            .start_timer();
        let mut mutated_objects: Vec<StoredHistoryObject> = vec![];
        let mut deleted_object_ids: Vec<StoredDeletedHistoryObject> = vec![];
        for object in objects {
            match object {
                ObjectChangeToCommit::MutatedObject(stored_object) => {
                    mutated_objects.push(stored_object.into());
                }
                ObjectChangeToCommit::DeletedObject(stored_deleted_object) => {
                    deleted_object_ids.push(stored_deleted_object.into());
                }
            }
        }
        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                for mutated_object_change_chunk in
                    mutated_objects.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    let error_message = concat!(
                        "Failed to write to ",
                        stringify!((objects_history::table)),
                        " DB"
                    );
                    diesel::insert_into(objects_history::table)
                        .values(mutated_object_change_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await
                        .map_err(IndexerError::from)
                        .context(error_message)?;
                }

                for deleted_objects_chunk in
                    deleted_object_ids.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    let error_message = concat!(
                        "Failed to write to ",
                        stringify!((objects_history::table)),
                        " DB"
                    );
                    diesel::insert_into(objects_history::table)
                        .values(deleted_objects_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await
                        .map_err(IndexerError::from)
                        .context(error_message)?;
                }

                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await
        .tap_ok(|_| {
            let elapsed = guard.stop_and_record();
            info!(
                elapsed,
                "Persisted {} chunked objects history",
                mutated_objects.len() + deleted_object_ids.len(),
            );
        })
        .tap_err(|e| {
            tracing::error!("Failed to persist object history with error: {}", e);
        })
    }

    async fn persist_full_objects_history_chunk(
        &self,
        objects: Vec<StoredFullHistoryObject>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        let guard = self
            .metrics
            .checkpoint_db_commit_latency_full_objects_history_chunks
            .start_timer();

        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                for objects_chunk in objects.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                    diesel::insert_into(full_objects_history::table)
                        .values(objects_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await
                        .map_err(IndexerError::from)
                        .context("Failed to write to full_objects_history table")?;
                }

                Ok(())
            }
            .scope_boxed()
        })
        .await
        .tap_ok(|_| {
            let elapsed = guard.stop_and_record();
            info!(
                elapsed,
                "Persisted {} chunked full objects history",
                objects.len(),
            );
        })
        .tap_err(|e| {
            tracing::error!("Failed to persist full object history with error: {}", e);
        })
    }

    async fn persist_object_version_chunk(
        &self,
        object_versions: Vec<StoredObjectVersion>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                for object_version_chunk in object_versions.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    diesel::insert_into(objects_version::table)
                        .values(object_version_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await
                        .map_err(IndexerError::from)
                        .context("Failed to write to objects_version table")?;
                }
                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await
    }

    async fn persist_checkpoints(
        &self,
        checkpoints: Vec<IndexedCheckpoint>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        let Some(first_checkpoint) = checkpoints.as_slice().first() else {
            return Ok(());
        };

        // If the first checkpoint has sequence number 0, we need to persist the digest as
        // chain identifier.
        if first_checkpoint.sequence_number == 0 {
            let checkpoint_digest = first_checkpoint.checkpoint_digest.into_inner().to_vec();
            self.persist_protocol_configs_and_feature_flags(checkpoint_digest.clone())
                .await?;

            transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
                async {
                    let checkpoint_digest =
                        first_checkpoint.checkpoint_digest.into_inner().to_vec();
                    diesel::insert_into(chain_identifier::table)
                        .values(StoredChainIdentifier { checkpoint_digest })
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await
                        .map_err(IndexerError::from)
                        .context("failed to write to chain_identifier table")?;
                    Ok::<(), IndexerError>(())
                }
                .scope_boxed()
            })
            .await?;
        }
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_checkpoints
            .start_timer();

        let stored_cp_txs = checkpoints.iter().map(StoredCpTx::from).collect::<Vec<_>>();
        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                for stored_cp_tx_chunk in stored_cp_txs.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                    diesel::insert_into(pruner_cp_watermark::table)
                        .values(stored_cp_tx_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await
                        .map_err(IndexerError::from)
                        .context("Failed to write to pruner_cp_watermark table")?;
                }
                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await
        .tap_ok(|_| {
            info!(
                "Persisted {} pruner_cp_watermark rows.",
                stored_cp_txs.len(),
            );
        })
        .tap_err(|e| {
            tracing::error!("Failed to persist pruner_cp_watermark with error: {}", e);
        })?;

        let stored_checkpoints = checkpoints
            .iter()
            .map(StoredCheckpoint::from)
            .collect::<Vec<_>>();
        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                for stored_checkpoint_chunk in
                    stored_checkpoints.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    diesel::insert_into(checkpoints::table)
                        .values(stored_checkpoint_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await
                        .map_err(IndexerError::from)
                        .context("Failed to write to checkpoints table")?;
                    let time_now_ms = chrono::Utc::now().timestamp_millis();
                    for stored_checkpoint in stored_checkpoint_chunk {
                        self.metrics
                            .db_commit_lag_ms
                            .set(time_now_ms - stored_checkpoint.timestamp_ms);
                        self.metrics
                            .max_committed_checkpoint_sequence_number
                            .set(stored_checkpoint.sequence_number);
                        self.metrics
                            .committed_checkpoint_timestamp_ms
                            .set(stored_checkpoint.timestamp_ms);
                    }

                    for stored_checkpoint in stored_checkpoint_chunk {
                        info!(
                            "Indexer lag: \
                            persisted checkpoint {} with time now {} and checkpoint time {}",
                            stored_checkpoint.sequence_number,
                            time_now_ms,
                            stored_checkpoint.timestamp_ms
                        );
                    }
                }
                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await
        .tap_ok(|_| {
            let elapsed = guard.stop_and_record();
            info!(
                elapsed,
                "Persisted {} checkpoints",
                stored_checkpoints.len()
            );
        })
        .tap_err(|e| {
            tracing::error!("Failed to persist checkpoints with error: {}", e);
        })
    }

    async fn persist_transactions_chunk(
        &self,
        transactions: Vec<IndexedTransaction>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_transactions_chunks
            .start_timer();
        let transformation_guard = self
            .metrics
            .checkpoint_db_commit_latency_transactions_chunks_transformation
            .start_timer();
        let transactions = transactions
            .iter()
            .map(StoredTransaction::from)
            .collect::<Vec<_>>();
        drop(transformation_guard);

        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                for transaction_chunk in transactions.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                    let error_message = concat!(
                        "Failed to write to ",
                        stringify!((transactions::table)),
                        " DB"
                    );
                    diesel::insert_into(transactions::table)
                        .values(transaction_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await
                        .map_err(IndexerError::from)
                        .context(error_message)?;
                }
                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await
        .tap_ok(|_| {
            let elapsed = guard.stop_and_record();
            info!(
                elapsed,
                "Persisted {} chunked transactions",
                transactions.len()
            );
        })
        .tap_err(|e| {
            tracing::error!("Failed to persist transactions with error: {}", e);
        })
    }

    async fn persist_events_chunk(&self, events: Vec<IndexedEvent>) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_events_chunks
            .start_timer();
        let len = events.len();
        let events = events
            .into_iter()
            .map(StoredEvent::from)
            .collect::<Vec<_>>();

        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                for event_chunk in events.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                    let error_message =
                        concat!("Failed to write to ", stringify!((events::table)), " DB");
                    diesel::insert_into(events::table)
                        .values(event_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await
                        .map_err(IndexerError::from)
                        .context(error_message)?;
                }
                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await
        .tap_ok(|_| {
            let elapsed = guard.stop_and_record();
            info!(elapsed, "Persisted {} chunked events", len);
        })
        .tap_err(|e| {
            tracing::error!("Failed to persist events with error: {}", e);
        })
    }

    async fn persist_packages(&self, packages: Vec<IndexedPackage>) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;
        if packages.is_empty() {
            return Ok(());
        }
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_packages
            .start_timer();
        let packages = packages
            .into_iter()
            .map(StoredPackage::from)
            .collect::<Vec<_>>();
        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                for packages_chunk in packages.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                    diesel::insert_into(packages::table)
                        .values(packages_chunk)
                        .on_conflict(packages::package_id)
                        .do_update()
                        .set((
                            packages::package_id.eq(excluded(packages::package_id)),
                            packages::move_package.eq(excluded(packages::move_package)),
                        ))
                        .execute(conn)
                        .await?;
                }
                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await
        .tap_ok(|_| {
            let elapsed = guard.stop_and_record();
            info!(elapsed, "Persisted {} packages", packages.len());
        })
        .tap_err(|e| {
            tracing::error!("Failed to persist packages with error: {}", e);
        })
    }

    async fn persist_event_indices_chunk(
        &self,
        indices: Vec<EventIndex>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        let guard = self
            .metrics
            .checkpoint_db_commit_latency_event_indices_chunks
            .start_timer();
        let len = indices.len();
        let (
            event_emit_packages,
            event_emit_modules,
            event_senders,
            event_struct_packages,
            event_struct_modules,
            event_struct_names,
            event_struct_instantiations,
        ) = indices.into_iter().map(|i| i.split()).fold(
            (
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ),
            |(
                mut event_emit_packages,
                mut event_emit_modules,
                mut event_senders,
                mut event_struct_packages,
                mut event_struct_modules,
                mut event_struct_names,
                mut event_struct_instantiations,
            ),
             index| {
                event_emit_packages.push(index.0);
                event_emit_modules.push(index.1);
                event_senders.push(index.2);
                event_struct_packages.push(index.3);
                event_struct_modules.push(index.4);
                event_struct_names.push(index.5);
                event_struct_instantiations.push(index.6);
                (
                    event_emit_packages,
                    event_emit_modules,
                    event_senders,
                    event_struct_packages,
                    event_struct_modules,
                    event_struct_names,
                    event_struct_instantiations,
                )
            },
        );

        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                diesel::insert_into(event_emit_package::table)
                    .values(&event_emit_packages)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                diesel::insert_into(event_emit_module::table)
                    .values(&event_emit_modules)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                diesel::insert_into(event_senders::table)
                    .values(&event_senders)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                diesel::insert_into(event_struct_package::table)
                    .values(&event_struct_packages)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                diesel::insert_into(event_struct_module::table)
                    .values(&event_struct_modules)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                diesel::insert_into(event_struct_name::table)
                    .values(&event_struct_names)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                diesel::insert_into(event_struct_instantiation::table)
                    .values(&event_struct_instantiations)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                Ok(())
            }
            .scope_boxed()
        })
        .await?;

        let elapsed = guard.stop_and_record();
        info!(elapsed, "Persisted {} chunked event indices", len);
        Ok(())
    }

    async fn persist_tx_indices_chunk(&self, indices: Vec<TxIndex>) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        let guard = self
            .metrics
            .checkpoint_db_commit_latency_tx_indices_chunks
            .start_timer();
        let len = indices.len();
        let (
            affected_addresses,
            senders,
            recipients,
            input_objects,
            changed_objects,
            pkgs,
            mods,
            funs,
            digests,
            kinds,
        ) = indices.into_iter().map(|i| i.split()).fold(
            (
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ),
            |(
                mut tx_affected_addresses,
                mut tx_senders,
                mut tx_recipients,
                mut tx_input_objects,
                mut tx_changed_objects,
                mut tx_pkgs,
                mut tx_mods,
                mut tx_funs,
                mut tx_digests,
                mut tx_kinds,
            ),
             index| {
                tx_affected_addresses.extend(index.0);
                tx_senders.extend(index.1);
                tx_recipients.extend(index.2);
                tx_input_objects.extend(index.3);
                tx_changed_objects.extend(index.4);
                tx_pkgs.extend(index.5);
                tx_mods.extend(index.6);
                tx_funs.extend(index.7);
                tx_digests.extend(index.8);
                tx_kinds.extend(index.9);
                (
                    tx_affected_addresses,
                    tx_senders,
                    tx_recipients,
                    tx_input_objects,
                    tx_changed_objects,
                    tx_pkgs,
                    tx_mods,
                    tx_funs,
                    tx_digests,
                    tx_kinds,
                )
            },
        );

        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                diesel::insert_into(tx_affected_addresses::table)
                    .values(&affected_addresses)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                diesel::insert_into(tx_senders::table)
                    .values(&senders)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                diesel::insert_into(tx_recipients::table)
                    .values(&recipients)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                diesel::insert_into(tx_input_objects::table)
                    .values(&input_objects)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                diesel::insert_into(tx_changed_objects::table)
                    .values(&changed_objects)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                diesel::insert_into(tx_calls_pkg::table)
                    .values(&pkgs)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                diesel::insert_into(tx_calls_mod::table)
                    .values(&mods)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                diesel::insert_into(tx_calls_fun::table)
                    .values(&funs)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                diesel::insert_into(tx_digests::table)
                    .values(&digests)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                diesel::insert_into(tx_kinds::table)
                    .values(&kinds)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await?;

                Ok(())
            }
            .scope_boxed()
        })
        .await?;

        let elapsed = guard.stop_and_record();
        info!(elapsed, "Persisted {} chunked tx_indices", len);
        Ok(())
    }

    async fn persist_epoch(&self, epoch: EpochToCommit) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_epoch
            .start_timer();
        let epoch_id = epoch.new_epoch.epoch;

        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                if let Some(last_epoch) = &epoch.last_epoch {
                    let last_epoch_id = last_epoch.epoch;
                    // Overwrites the `epoch_total_transactions` field on `epoch.last_epoch` because
                    // we are not guaranteed to have the latest data in db when this is set on
                    // indexer's chain-reading side. However, when we `persist_epoch`, the
                    // checkpoints from an epoch ago must have been indexed.
                    let previous_epoch_network_total_transactions = match epoch_id {
                        0 | 1 => 0,
                        _ => {
                            let prev_epoch_id = epoch_id - 2;
                            let result = checkpoints::table
                                .filter(checkpoints::epoch.eq(prev_epoch_id as i64))
                                .select(max(checkpoints::network_total_transactions))
                                .first::<Option<i64>>(conn)
                                .await
                                .map(|o| o.unwrap_or(0))?;

                            result as u64
                        }
                    };

                    let epoch_total_transactions = epoch.network_total_transactions
                        - previous_epoch_network_total_transactions;

                    let mut last_epoch = StoredEpochInfo::from_epoch_end_info(last_epoch);
                    last_epoch.epoch_total_transactions = Some(epoch_total_transactions as i64);
                    info!(last_epoch_id, "Persisting epoch end data.");
                    diesel::insert_into(epochs::table)
                        .values(vec![last_epoch])
                        .on_conflict(epochs::epoch)
                        .do_update()
                        .set((
                            epochs::system_state.eq(excluded(epochs::system_state)),
                            epochs::epoch_total_transactions
                                .eq(excluded(epochs::epoch_total_transactions)),
                            epochs::last_checkpoint_id.eq(excluded(epochs::last_checkpoint_id)),
                            epochs::epoch_end_timestamp.eq(excluded(epochs::epoch_end_timestamp)),
                            epochs::storage_fund_reinvestment
                                .eq(excluded(epochs::storage_fund_reinvestment)),
                            epochs::storage_charge.eq(excluded(epochs::storage_charge)),
                            epochs::storage_rebate.eq(excluded(epochs::storage_rebate)),
                            epochs::stake_subsidy_amount.eq(excluded(epochs::stake_subsidy_amount)),
                            epochs::total_gas_fees.eq(excluded(epochs::total_gas_fees)),
                            epochs::total_stake_rewards_distributed
                                .eq(excluded(epochs::total_stake_rewards_distributed)),
                            epochs::leftover_storage_fund_inflow
                                .eq(excluded(epochs::leftover_storage_fund_inflow)),
                            epochs::epoch_commitments.eq(excluded(epochs::epoch_commitments)),
                        ))
                        .execute(conn)
                        .await?;
                }

                let epoch_id = epoch.new_epoch.epoch;
                info!(epoch_id, "Persisting epoch beginning info");
                let new_epoch = StoredEpochInfo::from_epoch_beginning_info(&epoch.new_epoch);
                let error_message =
                    concat!("Failed to write to ", stringify!((epochs::table)), " DB");
                diesel::insert_into(epochs::table)
                    .values(new_epoch)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await
                    .map_err(IndexerError::from)
                    .context(error_message)?;
                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await
        .tap_ok(|_| {
            let elapsed = guard.stop_and_record();
            info!(elapsed, epoch_id, "Persisted epoch beginning info");
        })
        .tap_err(|e| {
            tracing::error!("Failed to persist epoch with error: {}", e);
        })
    }

    async fn advance_epoch(&self, epoch_to_commit: EpochToCommit) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let last_epoch_id = epoch_to_commit.last_epoch.as_ref().map(|e| e.epoch);
        // partition_0 has been created, so no need to advance it.
        if let Some(last_epoch_id) = last_epoch_id {
            let last_db_epoch: Option<StoredEpochInfo> = epochs::table
                .filter(epochs::epoch.eq(last_epoch_id as i64))
                .first::<StoredEpochInfo>(&mut connection)
                .await
                .optional()
                .map_err(Into::into)
                .context("Failed to read last epoch from PostgresDB")?;
            if let Some(last_epoch) = last_db_epoch {
                let epoch_partition_data =
                    EpochPartitionData::compose_data(epoch_to_commit, last_epoch);
                let table_partitions = self.partition_manager.get_table_partitions().await?;
                for (table, (_, last_partition)) in table_partitions {
                    // Only advance epoch partition for epoch partitioned tables.
                    if !self
                        .partition_manager
                        .get_strategy(&table)
                        .is_epoch_partitioned()
                    {
                        continue;
                    }
                    let guard = self.metrics.advance_epoch_latency.start_timer();
                    self.partition_manager
                        .advance_epoch(table.clone(), last_partition, &epoch_partition_data)
                        .await?;
                    let elapsed = guard.stop_and_record();
                    info!(
                        elapsed,
                        "Advanced epoch partition {} for table {}",
                        last_partition,
                        table.clone()
                    );
                }
            } else {
                tracing::error!("Last epoch: {} from PostgresDB is None.", last_epoch_id);
            }
        }

        Ok(())
    }

    async fn prune_checkpoints_table(&self, cp: u64) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;
        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                diesel::delete(
                    checkpoints::table.filter(checkpoints::sequence_number.eq(cp as i64)),
                )
                .execute(conn)
                .await
                .map_err(IndexerError::from)
                .context("Failed to prune checkpoints table")?;

                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await
    }

    async fn prune_epochs_table(&self, epoch: u64) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;
        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                diesel::delete(epochs::table.filter(epochs::epoch.eq(epoch as i64)))
                    .execute(conn)
                    .await
                    .map_err(IndexerError::from)
                    .context("Failed to prune epochs table")?;
                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await
    }

    async fn prune_event_indices_table(
        &self,
        min_tx: u64,
        max_tx: u64,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        let (min_tx, max_tx) = (min_tx as i64, max_tx as i64);
        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                diesel::delete(
                    event_emit_module::table
                        .filter(event_emit_module::tx_sequence_number.between(min_tx, max_tx)),
                )
                .execute(conn)
                .await?;

                diesel::delete(
                    event_emit_package::table
                        .filter(event_emit_package::tx_sequence_number.between(min_tx, max_tx)),
                )
                .execute(conn)
                .await?;

                diesel::delete(
                    event_senders::table
                        .filter(event_senders::tx_sequence_number.between(min_tx, max_tx)),
                )
                .execute(conn)
                .await?;

                diesel::delete(event_struct_instantiation::table.filter(
                    event_struct_instantiation::tx_sequence_number.between(min_tx, max_tx),
                ))
                .execute(conn)
                .await?;

                diesel::delete(
                    event_struct_module::table
                        .filter(event_struct_module::tx_sequence_number.between(min_tx, max_tx)),
                )
                .execute(conn)
                .await?;

                diesel::delete(
                    event_struct_name::table
                        .filter(event_struct_name::tx_sequence_number.between(min_tx, max_tx)),
                )
                .execute(conn)
                .await?;

                diesel::delete(
                    event_struct_package::table
                        .filter(event_struct_package::tx_sequence_number.between(min_tx, max_tx)),
                )
                .execute(conn)
                .await?;

                Ok(())
            }
            .scope_boxed()
        })
        .await
    }

    async fn prune_tx_indices_table(&self, min_tx: u64, max_tx: u64) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        let (min_tx, max_tx) = (min_tx as i64, max_tx as i64);
        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                diesel::delete(
                    tx_affected_addresses::table
                        .filter(tx_affected_addresses::tx_sequence_number.between(min_tx, max_tx)),
                )
                .execute(conn)
                .await?;

                diesel::delete(
                    tx_senders::table
                        .filter(tx_senders::tx_sequence_number.between(min_tx, max_tx)),
                )
                .execute(conn)
                .await?;

                diesel::delete(
                    tx_recipients::table
                        .filter(tx_recipients::tx_sequence_number.between(min_tx, max_tx)),
                )
                .execute(conn)
                .await?;

                diesel::delete(
                    tx_input_objects::table
                        .filter(tx_input_objects::tx_sequence_number.between(min_tx, max_tx)),
                )
                .execute(conn)
                .await?;

                diesel::delete(
                    tx_changed_objects::table
                        .filter(tx_changed_objects::tx_sequence_number.between(min_tx, max_tx)),
                )
                .execute(conn)
                .await?;

                diesel::delete(
                    tx_calls_pkg::table
                        .filter(tx_calls_pkg::tx_sequence_number.between(min_tx, max_tx)),
                )
                .execute(conn)
                .await?;

                diesel::delete(
                    tx_calls_mod::table
                        .filter(tx_calls_mod::tx_sequence_number.between(min_tx, max_tx)),
                )
                .execute(conn)
                .await?;

                diesel::delete(
                    tx_calls_fun::table
                        .filter(tx_calls_fun::tx_sequence_number.between(min_tx, max_tx)),
                )
                .execute(conn)
                .await?;

                diesel::delete(
                    tx_digests::table
                        .filter(tx_digests::tx_sequence_number.between(min_tx, max_tx)),
                )
                .execute(conn)
                .await?;

                Ok(())
            }
            .scope_boxed()
        })
        .await
    }

    async fn prune_cp_tx_table(&self, cp: u64) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                diesel::delete(
                    pruner_cp_watermark::table
                        .filter(pruner_cp_watermark::checkpoint_sequence_number.eq(cp as i64)),
                )
                .execute(conn)
                .await
                .map_err(IndexerError::from)
                .context("Failed to prune pruner_cp_watermark table")?;
                Ok(())
            }
            .scope_boxed()
        })
        .await
    }

    async fn get_network_total_transactions_by_end_of_epoch(
        &self,
        epoch: u64,
    ) -> Result<u64, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        checkpoints::table
            .filter(checkpoints::epoch.eq(epoch as i64))
            .select(checkpoints::network_total_transactions)
            .order_by(checkpoints::sequence_number.desc())
            .first::<i64>(&mut connection)
            .await
            .map_err(Into::into)
            .context("Failed to get network total transactions in epoch")
            .map(|v| v as u64)
    }
}

#[async_trait]
impl IndexerStore for PgIndexerStore {
    async fn get_latest_checkpoint_sequence_number(&self) -> Result<Option<u64>, IndexerError> {
        self.get_latest_checkpoint_sequence_number().await
    }

    async fn get_available_epoch_range(&self) -> Result<(u64, u64), IndexerError> {
        self.get_prunable_epoch_range().await
    }

    async fn get_available_checkpoint_range(&self) -> Result<(u64, u64), IndexerError> {
        self.get_available_checkpoint_range().await
    }

    async fn get_chain_identifier(&self) -> Result<Option<Vec<u8>>, IndexerError> {
        self.get_chain_identifier().await
    }

    async fn get_latest_object_snapshot_checkpoint_sequence_number(
        &self,
    ) -> Result<Option<u64>, IndexerError> {
        self.get_latest_object_snapshot_checkpoint_sequence_number()
            .await
    }

    async fn persist_objects(
        &self,
        object_changes: Vec<TransactionObjectChangesToCommit>,
    ) -> Result<(), IndexerError> {
        if object_changes.is_empty() {
            return Ok(());
        }
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_objects
            .start_timer();
        let objects = make_final_list_of_objects_to_commit(object_changes);
        let len = objects.len();

        let mut object_mutations = vec![];
        let mut object_deletions = vec![];
        for object in objects {
            match object {
                ObjectChangeToCommit::MutatedObject(mutation) => {
                    object_mutations.push(mutation);
                }
                ObjectChangeToCommit::DeletedObject(deletion) => {
                    object_deletions.push(deletion);
                }
            }
        }
        let mutation_len = object_mutations.len();
        let deletion_len = object_deletions.len();

        let object_mutation_chunks =
            chunk!(object_mutations, self.config.parallel_objects_chunk_size);
        let object_deletion_chunks =
            chunk!(object_deletions, self.config.parallel_objects_chunk_size);
        let mutation_futures = object_mutation_chunks
            .into_iter()
            .map(|c| self.persist_object_mutation_chunk(c))
            .collect::<Vec<_>>();
        futures::future::join_all(mutation_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed to persist all object mutation chunks: {:?}",
                    e
                ))
            })?;
        let deletion_futures = object_deletion_chunks
            .into_iter()
            .map(|c| self.persist_object_deletion_chunk(c))
            .collect::<Vec<_>>();
        futures::future::join_all(deletion_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed to persist all object deletion chunks: {:?}",
                    e
                ))
            })?;

        let elapsed = guard.stop_and_record();
        info!(
            elapsed,
            "Persisted {} objects with {} mutations and {} deletions ",
            len,
            mutation_len,
            deletion_len,
        );
        Ok(())
    }

    async fn persist_objects_snapshot(
        &self,
        object_changes: Vec<TransactionObjectChangesToCommit>,
    ) -> Result<(), IndexerError> {
        if object_changes.is_empty() {
            return Ok(());
        }
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_objects_snapshot
            .start_timer();
        let objects = make_final_list_of_objects_to_commit(object_changes);
        let len = objects.len();
        let chunks = chunk!(objects, self.config.parallel_objects_chunk_size);
        let futures = chunks
            .into_iter()
            .map(|c| self.backfill_objects_snapshot_chunk(c))
            .collect::<Vec<_>>();

        futures::future::join_all(futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed to persist all objects snapshot chunks: {:?}",
                    e
                ))
            })?;
        let elapsed = guard.stop_and_record();
        info!(elapsed, "Persisted {} objects snapshot", len);
        Ok(())
    }

    async fn persist_object_history(
        &self,
        object_changes: Vec<TransactionObjectChangesToCommit>,
    ) -> Result<(), IndexerError> {
        let skip_history = std::env::var("SKIP_OBJECT_HISTORY")
            .map(|val| val.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if skip_history {
            info!("skipping object history");
            return Ok(());
        }

        if object_changes.is_empty() {
            return Ok(());
        }
        let objects = make_objects_history_to_commit(object_changes);
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_objects_history
            .start_timer();

        let len = objects.len();
        let chunks = chunk!(objects, self.config.parallel_objects_chunk_size);
        let futures = chunks
            .into_iter()
            .map(|c| self.persist_objects_history_chunk(c))
            .collect::<Vec<_>>();

        futures::future::join_all(futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed to persist all objects history chunks: {:?}",
                    e
                ))
            })?;
        let elapsed = guard.stop_and_record();
        info!(elapsed, "Persisted {} objects history", len);
        Ok(())
    }

    // TODO: There are quite some shared boiler-plate code in all functions.
    // We should clean them up eventually.
    async fn persist_full_objects_history(
        &self,
        object_changes: Vec<TransactionObjectChangesToCommit>,
    ) -> Result<(), IndexerError> {
        let skip_history = std::env::var("SKIP_OBJECT_HISTORY")
            .map(|val| val.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if skip_history {
            info!("skipping object history");
            return Ok(());
        }

        if object_changes.is_empty() {
            return Ok(());
        }
        let objects: Vec<StoredFullHistoryObject> = object_changes
            .into_iter()
            .flat_map(|c| {
                let TransactionObjectChangesToCommit {
                    changed_objects,
                    deleted_objects,
                } = c;
                changed_objects
                    .into_iter()
                    .map(|o| o.into())
                    .chain(deleted_objects.into_iter().map(|o| o.into()))
            })
            .collect();
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_full_objects_history
            .start_timer();

        let len = objects.len();
        let chunks = chunk!(objects, self.config.parallel_objects_chunk_size);
        let futures = chunks
            .into_iter()
            .map(|c| self.persist_full_objects_history_chunk(c))
            .collect::<Vec<_>>();

        futures::future::join_all(futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed to persist all full objects history chunks: {:?}",
                    e
                ))
            })?;
        let elapsed = guard.stop_and_record();
        info!(elapsed, "Persisted {} full objects history", len);
        Ok(())
    }

    async fn persist_object_versions(
        &self,
        object_versions: Vec<StoredObjectVersion>,
    ) -> Result<(), IndexerError> {
        if object_versions.is_empty() {
            return Ok(());
        }
        let object_versions_count = object_versions.len();
        let chunks = chunk!(object_versions, self.config.parallel_objects_chunk_size);
        let futures = chunks
            .into_iter()
            .map(|c| self.persist_object_version_chunk(c))
            .collect::<Vec<_>>();

        futures::future::join_all(futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed to persist all object version chunks: {:?}",
                    e
                ))
            })?;
        info!("Persisted {} object versions", object_versions_count);
        Ok(())
    }

    async fn persist_checkpoints(
        &self,
        checkpoints: Vec<IndexedCheckpoint>,
    ) -> Result<(), IndexerError> {
        self.persist_checkpoints(checkpoints).await
    }

    async fn persist_transactions(
        &self,
        transactions: Vec<IndexedTransaction>,
    ) -> Result<(), IndexerError> {
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_transactions
            .start_timer();
        let len = transactions.len();

        let chunks = chunk!(transactions, self.config.parallel_chunk_size);
        let futures = chunks
            .into_iter()
            .map(|c| self.persist_transactions_chunk(c))
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

    async fn persist_events(&self, events: Vec<IndexedEvent>) -> Result<(), IndexerError> {
        if events.is_empty() {
            return Ok(());
        }
        let len = events.len();
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_events
            .start_timer();
        let chunks = chunk!(events, self.config.parallel_chunk_size);
        let futures = chunks
            .into_iter()
            .map(|c| self.persist_events_chunk(c))
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
    }

    async fn persist_displays(
        &self,
        display_updates: BTreeMap<String, StoredDisplay>,
    ) -> Result<(), IndexerError> {
        if display_updates.is_empty() {
            return Ok(());
        }

        self.persist_display_updates(display_updates).await
    }

    async fn persist_packages(&self, packages: Vec<IndexedPackage>) -> Result<(), IndexerError> {
        if packages.is_empty() {
            return Ok(());
        }
        self.persist_packages(packages).await
    }

    async fn persist_event_indices(&self, indices: Vec<EventIndex>) -> Result<(), IndexerError> {
        if indices.is_empty() {
            return Ok(());
        }
        let len = indices.len();
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_event_indices
            .start_timer();
        let chunks = chunk!(indices, self.config.parallel_chunk_size);

        let futures = chunks
            .into_iter()
            .map(|chunk| self.persist_event_indices_chunk(chunk))
            .collect::<Vec<_>>();
        futures::future::join_all(futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed to persist all event_indices chunks: {:?}",
                    e
                ))
            })?;
        let elapsed = guard.stop_and_record();
        info!(elapsed, "Persisted {} event_indices chunks", len);
        Ok(())
    }

    async fn persist_tx_indices(&self, indices: Vec<TxIndex>) -> Result<(), IndexerError> {
        if indices.is_empty() {
            return Ok(());
        }
        let len = indices.len();
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_tx_indices
            .start_timer();
        let chunks = chunk!(indices, self.config.parallel_chunk_size);

        let futures = chunks
            .into_iter()
            .map(|chunk| self.persist_tx_indices_chunk(chunk))
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
        info!(elapsed, "Persisted {} tx_indices chunks", len);
        Ok(())
    }

    async fn persist_epoch(&self, epoch: EpochToCommit) -> Result<(), IndexerError> {
        self.persist_epoch(epoch).await
    }

    async fn advance_epoch(&self, epoch: EpochToCommit) -> Result<(), IndexerError> {
        self.advance_epoch(epoch).await
    }

    async fn prune_epoch(&self, epoch: u64) -> Result<(), IndexerError> {
        let (mut min_cp, max_cp) = match self.get_checkpoint_range_for_epoch(epoch).await? {
            (min_cp, Some(max_cp)) => Ok((min_cp, max_cp)),
            _ => Err(IndexerError::PostgresReadError(format!(
                "Failed to get checkpoint range for epoch {}",
                epoch
            ))),
        }?;

        // NOTE: for disaster recovery, min_cp is the min cp of the current epoch, which is likely
        // partially pruned already. min_prunable_cp is the min cp to be pruned.
        // By std::cmp::max, we will resume the pruning process from the next checkpoint, instead of
        // the first cp of the current epoch.
        let min_prunable_cp = self.get_min_prunable_checkpoint().await?;
        min_cp = std::cmp::max(min_cp, min_prunable_cp);
        for cp in min_cp..=max_cp {
            // NOTE: the order of pruning tables is crucial:
            // 1. prune checkpoints table, checkpoints table is the source table of available range,
            // we prune it first to make sure that we always have full data for checkpoints within the available range;
            // 2. then prune tx_* tables;
            // 3. then prune pruner_cp_watermark table, which is the checkpoint pruning watermark table and also tx seq source
            // of a checkpoint to prune tx_* tables;
            // 4. lastly we prune epochs table when all checkpoints of the epoch have been pruned.
            info!(
                "Pruning checkpoint {} of epoch {} (min_prunable_cp: {})",
                cp, epoch, min_prunable_cp
            );
            self.prune_checkpoints_table(cp).await?;

            let (min_tx, max_tx) = self.get_transaction_range_for_checkpoint(cp).await?;
            self.prune_tx_indices_table(min_tx, max_tx).await?;
            info!(
                "Pruned transactions for checkpoint {} from tx {} to tx {}",
                cp, min_tx, max_tx
            );
            self.prune_event_indices_table(min_tx, max_tx).await?;
            info!(
                "Pruned events of transactions for checkpoint {} from tx {} to tx {}",
                cp, min_tx, max_tx
            );
            self.metrics.last_pruned_transaction.set(max_tx as i64);

            self.prune_cp_tx_table(cp).await?;
            info!("Pruned checkpoint {} of epoch {}", cp, epoch);
            self.metrics.last_pruned_checkpoint.set(cp as i64);
        }

        // NOTE: prune epochs table last, otherwise get_checkpoint_range_for_epoch would fail.
        self.prune_epochs_table(epoch).await?;
        Ok(())
    }

    async fn upload_display(&self, epoch_number: u64) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = Writer::from_writer(&mut buffer);

            let displays = display::table
                .load::<StoredDisplay>(&mut connection)
                .await
                .map_err(Into::into)
                .context("Failed to get display from database")?;

            info!("Read {} displays", displays.len());
            writer
                .write_record(["object_type", "id", "version", "bcs"])
                .map_err(|_| {
                    IndexerError::GcsError("Failed to write display to csv".to_string())
                })?;

            for display in displays {
                writer
                    .write_record(&[
                        display.object_type,
                        hex::encode(display.id),
                        display.version.to_string(),
                        hex::encode(display.bcs),
                    ])
                    .map_err(|_| IndexerError::GcsError("Failed to write to csv".to_string()))?;
            }

            writer
                .flush()
                .map_err(|_| IndexerError::GcsError("Failed to flush csv".to_string()))?;
        }

        if let (Some(cred_path), Some(bucket)) = (
            self.config.gcs_cred_path.clone(),
            self.config.gcs_display_bucket.clone(),
        ) {
            let remote_store_config = ObjectStoreConfig {
                object_store: Some(ObjectStoreType::GCS),
                bucket: Some(bucket),
                google_service_account: Some(cred_path),
                object_store_connection_limit: 200,
                no_sign_request: false,
                ..Default::default()
            };
            let remote_store = remote_store_config.make().map_err(|e| {
                IndexerError::GcsError(format!("Failed to make GCS remote store: {}", e))
            })?;

            let path = Path::from(format!("display_{}.csv", epoch_number).as_str());
            put(&remote_store, &path, buffer.into_inner().into())
                .await
                .map_err(|e| IndexerError::GcsError(format!("Failed to put to GCS: {}", e)))?;
        } else {
            warn!("Either GCS cred path or bucket is not set, skipping display upload.");
        }
        Ok(())
    }

    async fn get_network_total_transactions_by_end_of_epoch(
        &self,
        epoch: u64,
    ) -> Result<u64, IndexerError> {
        self.get_network_total_transactions_by_end_of_epoch(epoch)
            .await
    }

    /// Persist protocol configs and feature flags until the protocol version for the latest epoch
    /// we have stored in the db, inclusive.
    async fn persist_protocol_configs_and_feature_flags(
        &self,
        chain_id: Vec<u8>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        let chain_id = ChainIdentifier::from(
            CheckpointDigest::try_from(chain_id).expect("Unable to convert chain id"),
        );

        let mut all_configs = vec![];
        let mut all_flags = vec![];

        let (start_version, end_version) = self.get_protocol_version_index_range().await?;
        info!(
            "Persisting protocol configs with start_version: {}, end_version: {}",
            start_version, end_version
        );

        // Gather all protocol configs and feature flags for all versions between start and end.
        for version in start_version..=end_version {
            let protocol_configs = ProtocolConfig::get_for_version_if_supported(
                (version as u64).into(),
                chain_id.chain(),
            )
            .ok_or(IndexerError::GenericError(format!(
                "Unable to fetch protocol version {} and chain {:?}",
                version,
                chain_id.chain()
            )))?;
            let configs_vec = protocol_configs
                .attr_map()
                .into_iter()
                .map(|(k, v)| StoredProtocolConfig {
                    protocol_version: version,
                    config_name: k,
                    config_value: v.map(|v| v.to_string()),
                })
                .collect::<Vec<_>>();
            all_configs.extend(configs_vec);

            let feature_flags = protocol_configs
                .feature_map()
                .into_iter()
                .map(|(k, v)| StoredFeatureFlag {
                    protocol_version: version,
                    flag_name: k,
                    flag_value: v,
                })
                .collect::<Vec<_>>();
            all_flags.extend(feature_flags);
        }

        // Now insert all of them into the db.
        // TODO: right now the size of these updates is manageable but later we may consider batching.
        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                for config_chunk in all_configs.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                    diesel::insert_into(protocol_configs::table)
                        .values(config_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await
                        .map_err(IndexerError::from)
                        .context("Failed to write to protocol_configs table")?;
                }

                diesel::insert_into(feature_flags::table)
                    .values(all_flags.clone())
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await
                    .map_err(IndexerError::from)
                    .context("Failed to write to feature_flags table")?;
                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await?;
        Ok(())
    }
}

/// Construct deleted objects and mutated objects to commit.
/// In particular, filter mutated objects updates that would
/// be override immediately.
fn make_final_list_of_objects_to_commit(
    tx_object_changes: Vec<TransactionObjectChangesToCommit>,
) -> Vec<ObjectChangeToCommit> {
    let deleted_objects = tx_object_changes
        .clone()
        .into_iter()
        .flat_map(|changes| changes.deleted_objects)
        .map(|o| (o.object_id, o.into()))
        .collect::<HashMap<ObjectID, StoredDeletedObject>>();

    let mutated_objects = tx_object_changes
        .into_iter()
        .flat_map(|changes| changes.changed_objects);
    let mut latest_objects = HashMap::new();
    for object in mutated_objects {
        if deleted_objects.contains_key(&object.object.id()) {
            continue;
        }
        match latest_objects.entry(object.object.id()) {
            Entry::Vacant(e) => {
                e.insert(object);
            }
            Entry::Occupied(mut e) => {
                if object.object.version() > e.get().object.version() {
                    e.insert(object);
                }
            }
        }
    }
    deleted_objects
        .into_values()
        .map(ObjectChangeToCommit::DeletedObject)
        .chain(
            latest_objects
                .into_values()
                .map(StoredObject::from)
                .map(ObjectChangeToCommit::MutatedObject),
        )
        .collect()
}

fn make_objects_history_to_commit(
    tx_object_changes: Vec<TransactionObjectChangesToCommit>,
) -> Vec<ObjectChangeToCommit> {
    let deleted_objects: Vec<StoredDeletedObject> = tx_object_changes
        .clone()
        .into_iter()
        .flat_map(|changes| changes.deleted_objects)
        .map(|o| o.into())
        .collect();
    let mutated_objects: Vec<StoredObject> = tx_object_changes
        .into_iter()
        .flat_map(|changes| changes.changed_objects)
        .map(|o| o.into())
        .collect();
    deleted_objects
        .into_iter()
        .map(ObjectChangeToCommit::DeletedObject)
        .chain(
            mutated_objects
                .into_iter()
                .map(ObjectChangeToCommit::MutatedObject),
        )
        .collect()
}
