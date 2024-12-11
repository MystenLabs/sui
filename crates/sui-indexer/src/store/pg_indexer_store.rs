// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use std::time::Duration;

use async_trait::async_trait;
use core::result::Result::Ok;
use csv::{ReaderBuilder, Writer};
use diesel::dsl::{max, min};
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::QueryDsl;
use diesel_async::scoped_futures::ScopedFutureExt;
use futures::future::Either;
use itertools::Itertools;
use object_store::path::Path;
use strum::IntoEnumIterator;
use sui_types::base_types::ObjectID;
use tap::TapFallible;
use tracing::{info, warn};

use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_protocol_config::ProtocolConfig;
use sui_storage::object_store::util::put;

use crate::config::UploadOptions;
use crate::database::ConnectionPool;
use crate::errors::{Context, IndexerError};
use crate::handlers::pruner::PrunableTable;
use crate::handlers::TransactionObjectChangesToCommit;
use crate::handlers::{CommitterWatermark, EpochToCommit};
use crate::metrics::IndexerMetrics;
use crate::models::checkpoints::StoredChainIdentifier;
use crate::models::checkpoints::StoredCheckpoint;
use crate::models::checkpoints::StoredCpTx;
use crate::models::display::StoredDisplay;
use crate::models::epoch::StoredEpochInfo;
use crate::models::epoch::{StoredFeatureFlag, StoredProtocolConfig};
use crate::models::events::StoredEvent;
use crate::models::obj_indices::StoredObjectVersion;
use crate::models::objects::{
    StoredDeletedObject, StoredFullHistoryObject, StoredHistoryObject, StoredObject,
    StoredObjectSnapshot,
};
use crate::models::packages::StoredPackage;
use crate::models::transactions::StoredTransaction;
use crate::models::watermarks::StoredWatermark;
use crate::schema::{
    chain_identifier, checkpoints, display, epochs, event_emit_module, event_emit_package,
    event_senders, event_struct_instantiation, event_struct_module, event_struct_name,
    event_struct_package, events, feature_flags, full_objects_history, objects, objects_history,
    objects_snapshot, objects_version, packages, protocol_configs, pruner_cp_watermark,
    raw_checkpoints, transactions, tx_affected_addresses, tx_affected_objects, tx_calls_fun,
    tx_calls_mod, tx_calls_pkg, tx_changed_objects, tx_digests, tx_input_objects, tx_kinds,
    watermarks,
};
use crate::store::{read_with_retry, transaction_with_retry};
use crate::types::{EventIndex, IndexedDeletedObject, IndexedObject};
use crate::types::{IndexedCheckpoint, IndexedEvent, IndexedPackage, IndexedTransaction, TxIndex};

use super::pg_partition_manager::{EpochPartitionData, PgPartitionManager};
use super::IndexerStore;

use crate::models::raw_checkpoints::StoredRawCheckpoint;
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
        upload_options: UploadOptions,
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
            gcs_cred_path: upload_options.gcs_cred_path,
            gcs_display_bucket: upload_options.gcs_display_bucket,
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

    // `pub` is needed for wait_for_checkpoint in tests
    pub async fn get_latest_checkpoint_sequence_number(&self) -> Result<Option<u64>, IndexerError> {
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

    pub async fn get_checkpoint_range_for_epoch(
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

    pub async fn get_transaction_range_for_checkpoint(
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

    pub async fn get_latest_object_snapshot_checkpoint_sequence_number(
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
        display_updates: Vec<StoredDisplay>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                diesel::insert_into(display::table)
                    .values(display_updates)
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
                        objects::owner_type.eq(excluded(objects::owner_type)),
                        objects::owner_id.eq(excluded(objects::owner_id)),
                        objects::object_type.eq(excluded(objects::object_type)),
                        objects::serialized_object.eq(excluded(objects::serialized_object)),
                        objects::coin_type.eq(excluded(objects::coin_type)),
                        objects::coin_balance.eq(excluded(objects::coin_balance)),
                        objects::df_kind.eq(excluded(objects::df_kind)),
                    ))
                    .execute(conn)
                    .await?;
                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await
        .tap_ok(|_| {
            guard.stop_and_record();
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
            guard.stop_and_record();
        })
        .tap_err(|e| {
            tracing::error!("Failed to persist object deletions with error: {}", e);
        })
    }

    async fn persist_object_snapshot_mutation_chunk(
        &self,
        objects_snapshot_mutations: Vec<StoredObjectSnapshot>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_objects_snapshot_chunks
            .start_timer();
        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                for mutation_chunk in
                    objects_snapshot_mutations.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    diesel::insert_into(objects_snapshot::table)
                        .values(mutation_chunk)
                        .on_conflict(objects_snapshot::object_id)
                        .do_update()
                        .set((
                            objects_snapshot::object_version
                                .eq(excluded(objects_snapshot::object_version)),
                            objects_snapshot::object_status
                                .eq(excluded(objects_snapshot::object_status)),
                            objects_snapshot::object_digest
                                .eq(excluded(objects_snapshot::object_digest)),
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
                            objects_snapshot::checkpoint_sequence_number
                                .eq(excluded(objects_snapshot::checkpoint_sequence_number)),
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
            guard.stop_and_record();
        })
        .tap_err(|e| {
            tracing::error!("Failed to persist object snapshot with error: {}", e);
        })
    }

    async fn persist_object_snapshot_deletion_chunk(
        &self,
        objects_snapshot_deletions: Vec<StoredObjectSnapshot>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_objects_snapshot_chunks
            .start_timer();

        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                for deletion_chunk in
                    objects_snapshot_deletions.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    diesel::delete(
                        objects_snapshot::table.filter(
                            objects_snapshot::object_id.eq_any(
                                deletion_chunk
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
                "Deleted {} chunked object snapshots",
                objects_snapshot_deletions.len(),
            );
        })
        .tap_err(|e| {
            tracing::error!(
                "Failed to persist object snapshot deletions with error: {}",
                e
            );
        })
    }

    async fn persist_objects_history_chunk(
        &self,
        stored_objects_history: Vec<StoredHistoryObject>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_objects_history_chunks
            .start_timer();
        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                for stored_objects_history_chunk in
                    stored_objects_history.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    let error_message = concat!(
                        "Failed to write to ",
                        stringify!((objects_history::table)),
                        " DB"
                    );
                    diesel::insert_into(objects_history::table)
                        .values(stored_objects_history_chunk)
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
            guard.stop_and_record();
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

    async fn persist_objects_version_chunk(
        &self,
        object_versions: Vec<StoredObjectVersion>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        let guard = self
            .metrics
            .checkpoint_db_commit_latency_objects_version_chunks
            .start_timer();

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
        .tap_ok(|_| {
            let elapsed = guard.stop_and_record();
            info!(
                elapsed,
                "Persisted {} chunked object versions",
                object_versions.len(),
            );
        })
        .tap_err(|e| {
            tracing::error!("Failed to persist object versions with error: {}", e);
        })
    }

    async fn persist_raw_checkpoints_impl(
        &self,
        raw_checkpoints: &[StoredRawCheckpoint],
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                diesel::insert_into(raw_checkpoints::table)
                    .values(raw_checkpoints)
                    .on_conflict_do_nothing()
                    .execute(conn)
                    .await
                    .map_err(IndexerError::from)
                    .context("Failed to write to raw_checkpoints table")?;
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
            self.persist_chain_identifier(checkpoint_digest).await?;
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
                            packages::package_version.eq(excluded(packages::package_version)),
                            packages::move_package.eq(excluded(packages::move_package)),
                            packages::checkpoint_sequence_number
                                .eq(excluded(packages::checkpoint_sequence_number)),
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
                for event_emit_packages_chunk in
                    event_emit_packages.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    diesel::insert_into(event_emit_package::table)
                        .values(event_emit_packages_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;
                }

                for event_emit_modules_chunk in
                    event_emit_modules.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    diesel::insert_into(event_emit_module::table)
                        .values(event_emit_modules_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;
                }

                for event_senders_chunk in event_senders.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                    diesel::insert_into(event_senders::table)
                        .values(event_senders_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;
                }

                for event_struct_packages_chunk in
                    event_struct_packages.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    diesel::insert_into(event_struct_package::table)
                        .values(event_struct_packages_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;
                }

                for event_struct_modules_chunk in
                    event_struct_modules.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    diesel::insert_into(event_struct_module::table)
                        .values(event_struct_modules_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;
                }

                for event_struct_names_chunk in
                    event_struct_names.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    diesel::insert_into(event_struct_name::table)
                        .values(event_struct_names_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;
                }

                for event_struct_instantiations_chunk in
                    event_struct_instantiations.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    diesel::insert_into(event_struct_instantiation::table)
                        .values(event_struct_instantiations_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;
                }
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
            affected_objects,
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
            ),
            |(
                mut tx_affected_addresses,
                mut tx_affected_objects,
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
                tx_affected_objects.extend(index.1);
                tx_input_objects.extend(index.2);
                tx_changed_objects.extend(index.3);
                tx_pkgs.extend(index.4);
                tx_mods.extend(index.5);
                tx_funs.extend(index.6);
                tx_digests.extend(index.7);
                tx_kinds.extend(index.8);
                (
                    tx_affected_addresses,
                    tx_affected_objects,
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
                for affected_addresses_chunk in
                    affected_addresses.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    diesel::insert_into(tx_affected_addresses::table)
                        .values(affected_addresses_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;
                }

                for affected_objects_chunk in
                    affected_objects.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    diesel::insert_into(tx_affected_objects::table)
                        .values(affected_objects_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;
                }

                for input_objects_chunk in input_objects.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                    diesel::insert_into(tx_input_objects::table)
                        .values(input_objects_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;
                }

                for changed_objects_chunk in
                    changed_objects.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    diesel::insert_into(tx_changed_objects::table)
                        .values(changed_objects_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;
                }

                for pkgs_chunk in pkgs.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                    diesel::insert_into(tx_calls_pkg::table)
                        .values(pkgs_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;
                }

                for mods_chunk in mods.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                    diesel::insert_into(tx_calls_mod::table)
                        .values(mods_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;
                }

                for funs_chunk in funs.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                    diesel::insert_into(tx_calls_fun::table)
                        .values(funs_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;
                }

                for digests_chunk in digests.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                    diesel::insert_into(tx_digests::table)
                        .values(digests_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;
                }

                for kinds_chunk in kinds.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                    diesel::insert_into(tx_kinds::table)
                        .values(kinds_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .await?;
                }

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

                    info!(last_epoch_id, "Persisting epoch end data.");
                    diesel::update(epochs::table.filter(epochs::epoch.eq(last_epoch_id)))
                        .set(last_epoch)
                        .execute(conn)
                        .await?;
                }

                let epoch_id = epoch.new_epoch.epoch;
                info!(epoch_id, "Persisting epoch beginning info");
                let error_message =
                    concat!("Failed to write to ", stringify!((epochs::table)), " DB");
                diesel::insert_into(epochs::table)
                    .values(epoch.new_epoch)
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
                .filter(epochs::epoch.eq(last_epoch_id))
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
                    tx_affected_objects::table
                        .filter(tx_affected_objects::tx_sequence_number.between(min_tx, max_tx)),
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
    ) -> Result<Option<u64>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        // TODO: (wlmyng) update to read from epochs::network_total_transactions

        Ok(Some(
            checkpoints::table
                .filter(checkpoints::epoch.eq(epoch as i64))
                .select(checkpoints::network_total_transactions)
                .order_by(checkpoints::sequence_number.desc())
                .first::<i64>(&mut connection)
                .await
                .map_err(Into::into)
                .context("Failed to get network total transactions in epoch")
                .map(|v| v as u64)?,
        ))
    }

    async fn update_watermarks_upper_bound<E: IntoEnumIterator>(
        &self,
        watermark: CommitterWatermark,
    ) -> Result<(), IndexerError>
    where
        E::Iterator: Iterator<Item: AsRef<str>>,
    {
        use diesel_async::RunQueryDsl;

        let guard = self
            .metrics
            .checkpoint_db_commit_latency_watermarks
            .start_timer();

        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            let upper_bound_updates = E::iter()
                .map(|table| StoredWatermark::from_upper_bound_update(table.as_ref(), watermark))
                .collect::<Vec<_>>();
            async {
                diesel::insert_into(watermarks::table)
                    .values(upper_bound_updates)
                    .on_conflict(watermarks::pipeline)
                    .do_update()
                    .set((
                        watermarks::epoch_hi_inclusive.eq(excluded(watermarks::epoch_hi_inclusive)),
                        watermarks::checkpoint_hi_inclusive
                            .eq(excluded(watermarks::checkpoint_hi_inclusive)),
                        watermarks::tx_hi.eq(excluded(watermarks::tx_hi)),
                    ))
                    .execute(conn)
                    .await
                    .map_err(IndexerError::from)
                    .context("Failed to update watermarks upper bound")?;

                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await
        .tap_ok(|_| {
            let elapsed = guard.stop_and_record();
            info!(elapsed, "Persisted watermarks");
        })
        .tap_err(|e| {
            tracing::error!("Failed to persist watermarks with error: {}", e);
        })
    }

    async fn map_epochs_to_cp_tx(
        &self,
        epochs: &[u64],
    ) -> Result<HashMap<u64, (u64, u64)>, IndexerError> {
        use diesel_async::RunQueryDsl;

        let mut connection = self.pool.get().await?;

        let results: Vec<(i64, i64, Option<i64>)> = epochs::table
            .filter(epochs::epoch.eq_any(epochs.iter().map(|&e| e as i64)))
            .select((
                epochs::epoch,
                epochs::first_checkpoint_id,
                epochs::first_tx_sequence_number,
            ))
            .load::<(i64, i64, Option<i64>)>(&mut connection)
            .await
            .map_err(Into::into)
            .context("Failed to fetch first checkpoint and tx seq num for epochs")?;

        Ok(results
            .into_iter()
            .map(|(epoch, checkpoint, tx)| {
                (
                    epoch as u64,
                    (checkpoint as u64, tx.unwrap_or_default() as u64),
                )
            })
            .collect())
    }

    async fn update_watermarks_lower_bound(
        &self,
        watermarks: Vec<(PrunableTable, u64)>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        let epochs: Vec<u64> = watermarks.iter().map(|(_table, epoch)| *epoch).collect();
        let epoch_mapping = self.map_epochs_to_cp_tx(&epochs).await?;
        let lookups: Result<Vec<StoredWatermark>, IndexerError> = watermarks
            .into_iter()
            .map(|(table, epoch)| {
                let (checkpoint, tx) = epoch_mapping.get(&epoch).ok_or_else(|| {
                    IndexerError::PersistentStorageDataCorruptionError(format!(
                        "Epoch {} not found in epoch mapping",
                        epoch
                    ))
                })?;

                Ok(StoredWatermark::from_lower_bound_update(
                    table.as_ref(),
                    epoch,
                    table.select_reader_lo(*checkpoint, *tx),
                ))
            })
            .collect();
        let lower_bound_updates = lookups?;

        let guard = self
            .metrics
            .checkpoint_db_commit_latency_watermarks
            .start_timer();

        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                use diesel::dsl::sql;
                use diesel::query_dsl::methods::FilterDsl;

                diesel::insert_into(watermarks::table)
                    .values(lower_bound_updates)
                    .on_conflict(watermarks::pipeline)
                    .do_update()
                    .set((
                        watermarks::reader_lo.eq(excluded(watermarks::reader_lo)),
                        watermarks::epoch_lo.eq(excluded(watermarks::epoch_lo)),
                        watermarks::timestamp_ms.eq(sql::<diesel::sql_types::BigInt>(
                            "(EXTRACT(EPOCH FROM CURRENT_TIMESTAMP) * 1000)::bigint",
                        )),
                    ))
                    .filter(excluded(watermarks::reader_lo).gt(watermarks::reader_lo))
                    .filter(excluded(watermarks::epoch_lo).gt(watermarks::epoch_lo))
                    .filter(
                        diesel::dsl::sql::<diesel::sql_types::BigInt>(
                            "(EXTRACT(EPOCH FROM CURRENT_TIMESTAMP) * 1000)::bigint",
                        )
                        .gt(watermarks::timestamp_ms),
                    )
                    .execute(conn)
                    .await?;

                Ok::<(), IndexerError>(())
            }
            .scope_boxed()
        })
        .await
        .tap_ok(|_| {
            let elapsed = guard.stop_and_record();
            info!(elapsed, "Persisted watermarks");
        })
        .tap_err(|e| {
            tracing::error!("Failed to persist watermarks with error: {}", e);
        })
    }

    async fn get_watermarks(&self) -> Result<(Vec<StoredWatermark>, i64), IndexerError> {
        use diesel_async::RunQueryDsl;

        // read_only transaction, otherwise this will block and get blocked by write transactions to
        // the same table.
        read_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
                let stored = watermarks::table
                    .load::<StoredWatermark>(conn)
                    .await
                    .map_err(Into::into)
                    .context("Failed reading watermarks from PostgresDB")?;

                let timestamp = diesel::select(diesel::dsl::sql::<diesel::sql_types::BigInt>(
                    "(EXTRACT(EPOCH FROM CURRENT_TIMESTAMP) * 1000)::bigint",
                ))
                .get_result(conn)
                .await
                .map_err(Into::into)
                .context("Failed reading current timestamp from PostgresDB")?;

                Ok((stored, timestamp))
            }
            .scope_boxed()
        })
        .await
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
        let (indexed_mutations, indexed_deletions) = retain_latest_indexed_objects(object_changes);
        let object_mutations = indexed_mutations
            .into_iter()
            .map(StoredObject::from)
            .collect::<Vec<_>>();
        let object_deletions = indexed_deletions
            .into_iter()
            .map(StoredDeletedObject::from)
            .collect::<Vec<_>>();
        let mutation_len = object_mutations.len();
        let deletion_len = object_deletions.len();

        let object_mutation_chunks =
            chunk!(object_mutations, self.config.parallel_objects_chunk_size);
        let object_deletion_chunks =
            chunk!(object_deletions, self.config.parallel_objects_chunk_size);
        let mutation_futures = object_mutation_chunks
            .into_iter()
            .map(|c| self.persist_object_mutation_chunk(c))
            .map(Either::Left);
        let deletion_futures = object_deletion_chunks
            .into_iter()
            .map(|c| self.persist_object_deletion_chunk(c))
            .map(Either::Right);
        let all_futures = mutation_futures.chain(deletion_futures).collect::<Vec<_>>();

        futures::future::join_all(all_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed to persist all object mutation or deletion chunks: {:?}",
                    e
                ))
            })?;
        let elapsed = guard.stop_and_record();
        info!(
            elapsed,
            "Persisted {} objects mutations and {} deletions", mutation_len, deletion_len
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
        let (indexed_mutations, indexed_deletions) = retain_latest_indexed_objects(object_changes);
        let object_snapshot_mutations: Vec<StoredObjectSnapshot> = indexed_mutations
            .into_iter()
            .map(StoredObjectSnapshot::from)
            .collect();
        let object_snapshot_deletions: Vec<StoredObjectSnapshot> = indexed_deletions
            .into_iter()
            .map(StoredObjectSnapshot::from)
            .collect();
        let mutation_len = object_snapshot_mutations.len();
        let deletion_len = object_snapshot_deletions.len();
        let object_snapshot_mutation_chunks = chunk!(
            object_snapshot_mutations,
            self.config.parallel_objects_chunk_size
        );
        let object_snapshot_deletion_chunks = chunk!(
            object_snapshot_deletions,
            self.config.parallel_objects_chunk_size
        );
        let mutation_futures = object_snapshot_mutation_chunks
            .into_iter()
            .map(|c| self.persist_object_snapshot_mutation_chunk(c))
            .map(Either::Left)
            .collect::<Vec<_>>();
        let deletion_futures = object_snapshot_deletion_chunks
            .into_iter()
            .map(|c| self.persist_object_snapshot_deletion_chunk(c))
            .map(Either::Right)
            .collect::<Vec<_>>();
        let all_futures = mutation_futures
            .into_iter()
            .chain(deletion_futures)
            .collect::<Vec<_>>();
        futures::future::join_all(all_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed to persist object snapshot mutation or deletion chunks: {:?}",
                    e
                ))
            })
            .tap_ok(|_| {
                let elapsed = guard.stop_and_record();
                info!(
                    elapsed,
                    "Persisted {} objects snapshot mutations and {} deletions",
                    mutation_len,
                    deletion_len
                );
            })
            .tap_err(|e| {
                tracing::error!(
                    "Failed to persist object snapshot mutation or deletion chunks: {:?}",
                    e
                )
            })?;
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

    async fn persist_objects_version(
        &self,
        object_versions: Vec<StoredObjectVersion>,
    ) -> Result<(), IndexerError> {
        if object_versions.is_empty() {
            return Ok(());
        }

        let guard = self
            .metrics
            .checkpoint_db_commit_latency_objects_version
            .start_timer();

        let len = object_versions.len();
        let chunks = chunk!(object_versions, self.config.parallel_objects_chunk_size);
        let futures = chunks
            .into_iter()
            .map(|c| self.persist_objects_version_chunk(c))
            .collect::<Vec<_>>();

        futures::future::join_all(futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed to persist all objects version chunks: {:?}",
                    e
                ))
            })?;

        let elapsed = guard.stop_and_record();
        info!(elapsed, "Persisted {} object versions", len);
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
        self.persist_display_updates(display_updates.values().cloned().collect::<Vec<_>>())
            .await
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
            })
            .tap_ok(|_| {
                let elapsed = guard.stop_and_record();
                info!(elapsed, "Persisted {} event_indices chunks", len);
            })
            .tap_err(|e| tracing::error!("Failed to persist all event_indices chunks: {:?}", e))?;
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
            })
            .tap_ok(|_| {
                let elapsed = guard.stop_and_record();
                info!(elapsed, "Persisted {} tx_indices chunks", len);
            })
            .tap_err(|e| tracing::error!("Failed to persist all tx_indices chunks: {:?}", e))?;
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
            // 1. prune tx_* tables;
            // 2. prune event_* tables;
            // 3. then prune pruner_cp_watermark table, which is the checkpoint pruning watermark table and also tx seq source
            // of a checkpoint to prune tx_* tables;
            // 4. lastly prune checkpoints table, because wait_for_graphql_checkpoint_pruned
            // uses this table as the pruning watermark table.
            info!(
                "Pruning checkpoint {} of epoch {} (min_prunable_cp: {})",
                cp, epoch, min_prunable_cp
            );

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
            // NOTE: prune checkpoints table last b/c wait_for_graphql_checkpoint_pruned
            // uses this table as the watermark table.
            self.prune_checkpoints_table(cp).await?;

            info!("Pruned checkpoint {} of epoch {}", cp, epoch);
            self.metrics.last_pruned_checkpoint.set(cp as i64);
        }

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

    async fn restore_display(&self, bytes: bytes::Bytes) -> Result<(), IndexerError> {
        let cursor = Cursor::new(bytes);
        let mut csv_reader = ReaderBuilder::new().has_headers(true).from_reader(cursor);
        let displays = csv_reader
            .deserialize()
            .collect::<Result<Vec<StoredDisplay>, csv::Error>>()
            .map_err(|e| {
                IndexerError::GcsError(format!("Failed to deserialize display records: {}", e))
            })?;
        self.persist_display_updates(displays).await
    }

    async fn get_network_total_transactions_by_end_of_epoch(
        &self,
        epoch: u64,
    ) -> Result<Option<u64>, IndexerError> {
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

    async fn persist_chain_identifier(
        &self,
        checkpoint_digest: Vec<u8>,
    ) -> Result<(), IndexerError> {
        use diesel_async::RunQueryDsl;

        transaction_with_retry(&self.pool, PG_DB_COMMIT_SLEEP_DURATION, |conn| {
            async {
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
        Ok(())
    }

    async fn persist_raw_checkpoints(
        &self,
        checkpoints: Vec<StoredRawCheckpoint>,
    ) -> Result<(), IndexerError> {
        self.persist_raw_checkpoints_impl(&checkpoints).await
    }

    async fn update_watermarks_upper_bound<E: IntoEnumIterator>(
        &self,
        watermark: CommitterWatermark,
    ) -> Result<(), IndexerError>
    where
        E::Iterator: Iterator<Item: AsRef<str>>,
    {
        self.update_watermarks_upper_bound::<E>(watermark).await
    }

    async fn update_watermarks_lower_bound(
        &self,
        watermarks: Vec<(PrunableTable, u64)>,
    ) -> Result<(), IndexerError> {
        self.update_watermarks_lower_bound(watermarks).await
    }

    async fn get_watermarks(&self) -> Result<(Vec<StoredWatermark>, i64), IndexerError> {
        self.get_watermarks().await
    }
}

fn make_objects_history_to_commit(
    tx_object_changes: Vec<TransactionObjectChangesToCommit>,
) -> Vec<StoredHistoryObject> {
    let deleted_objects: Vec<StoredHistoryObject> = tx_object_changes
        .clone()
        .into_iter()
        .flat_map(|changes| changes.deleted_objects)
        .map(|o| o.into())
        .collect();
    let mutated_objects: Vec<StoredHistoryObject> = tx_object_changes
        .into_iter()
        .flat_map(|changes| changes.changed_objects)
        .map(|o| o.into())
        .collect();
    deleted_objects.into_iter().chain(mutated_objects).collect()
}

// Partition object changes into deletions and mutations,
// within partition of mutations or deletions, retain the latest with highest version;
// For overlappings of mutations and deletions, only keep one with higher version.
// This is necessary b/c after this step, DB commit will be done in parallel and not in order.
fn retain_latest_indexed_objects(
    tx_object_changes: Vec<TransactionObjectChangesToCommit>,
) -> (Vec<IndexedObject>, Vec<IndexedDeletedObject>) {
    // Only the last deleted / mutated object will be in the map,
    // b/c tx_object_changes are in order and versions always increment,
    let (mutations, deletions) = tx_object_changes
        .into_iter()
        .flat_map(|change| {
            change
                .changed_objects
                .into_iter()
                .map(Either::Left)
                .chain(
                    change
                        .deleted_objects
                        .into_iter()
                        .map(Either::Right),
                )
        })
        .fold(
            (HashMap::<ObjectID, IndexedObject>::new(), HashMap::<ObjectID, IndexedDeletedObject>::new()),
            |(mut mutations, mut deletions), either_change| {
                match either_change {
                    // Remove mutation / deletion with a following deletion / mutation,
                    // b/c following deletion / mutation always has a higher version.
                    // Technically, assertions below are not required, double check just in case.
                    Either::Left(mutation) => {
                        let id = mutation.object.id();
                        let mutation_version = mutation.object.version();
                        if let Some(existing) = deletions.remove(&id) {
                            assert!(
                                existing.object_version < mutation_version.value(),
                                "Mutation version ({:?}) should be greater than existing deletion version ({:?}) for object {:?}",
                                mutation_version,
                                existing.object_version,
                                id
                            );
                        }
                        if let Some(existing) = mutations.insert(id, mutation) {
                            assert!(
                                existing.object.version() < mutation_version,
                                "Mutation version ({:?}) should be greater than existing mutation version ({:?}) for object {:?}",
                                mutation_version,
                                existing.object.version(),
                                id
                            );
                        }
                    }
                    Either::Right(deletion) => {
                        let id = deletion.object_id;
                        let deletion_version = deletion.object_version;
                        if let Some(existing) = mutations.remove(&id) {
                            assert!(
                                existing.object.version().value() < deletion_version,
                                "Deletion version ({:?}) should be greater than existing mutation version ({:?}) for object {:?}",
                                deletion_version,
                                existing.object.version(),
                                id
                            );
                        }
                        if let Some(existing) = deletions.insert(id, deletion) {
                            assert!(
                                existing.object_version < deletion_version,
                                "Deletion version ({:?}) should be greater than existing deletion version ({:?}) for object {:?}",
                                deletion_version,
                                existing.object_version,
                                id
                            );
                        }
                    }
                }
                (mutations, deletions)
            },
        );
    (
        mutations.into_values().collect(),
        deletions.into_values().collect(),
    )
}
