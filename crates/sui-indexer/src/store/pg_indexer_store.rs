// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use core::result::Result::Ok;
use itertools::Itertools;
use std::any::Any;
use std::collections::hash_map::Entry;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tap::Tap;

use async_trait::async_trait;
use diesel::dsl::max;
use diesel::upsert::excluded;
use diesel::ExpressionMethods;
use diesel::OptionalExtension;
use diesel::{QueryDsl, RunQueryDsl};
use move_bytecode_utils::module_cache::SyncModuleCache;
use tracing::info;

use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::object::ObjectRead;

use crate::errors::{Context, IndexerError};
use crate::handlers::EpochToCommit;
use crate::handlers::TransactionObjectChangesToCommit;
use crate::metrics::IndexerMetrics;

use crate::db::PgConnectionPool;
use crate::models::checkpoints::StoredCheckpoint;
use crate::models::display::StoredDisplay;
use crate::models::epoch::StoredEpochInfo;
use crate::models::events::StoredEvent;
use crate::models::objects::{
    StoredDeletedHistoryObject, StoredDeletedObject, StoredHistoryObject, StoredObject,
};
use crate::models::packages::StoredPackage;
use crate::models::transactions::StoredTransaction;
use crate::schema::{
    checkpoints, display, epochs, events, objects, objects_history, objects_snapshot, packages,
    transactions, tx_calls, tx_changed_objects, tx_input_objects, tx_recipients, tx_senders,
};
use crate::store::diesel_macro::{read_only_blocking, transactional_blocking_with_retry};
use crate::store::module_resolver::IndexerStorePackageModuleResolver;
use crate::types::{IndexedCheckpoint, IndexedEvent, IndexedPackage, IndexedTransaction, TxIndex};

use super::pg_partition_manager::{EpochPartitionData, PgPartitionManager};
use super::IndexerStore;
use super::ObjectChangeToCommit;

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
// The amount of rows to update in one DB transcation
const PG_COMMIT_PARALLEL_CHUNK_SIZE_PER_DB_TX: usize = 500;
// The amount of rows to update in one DB transcation, for objects particularly
// Having this number too high may cause many db deadlocks because of
// optimistic locking.
const PG_COMMIT_OBJECTS_PARALLEL_CHUNK_SIZE_PER_DB_TX: usize = 500;

// with rn = 1, we only select the latest version of each object,
// so that we don't have to update the same object multiple times.
const UPDATE_OBJECTS_SNAPSHOT_QUERY: &str = r"
INSERT INTO objects_snapshot (object_id, object_version, object_status, object_digest, checkpoint_sequence_number, owner_type, owner_id, object_type, serialized_object, coin_type, coin_balance, df_kind, df_name, df_object_type, df_object_id)
SELECT object_id, object_version, object_status, object_digest, checkpoint_sequence_number, owner_type, owner_id, object_type, serialized_object, coin_type, coin_balance, df_kind, df_name, df_object_type, df_object_id
FROM (
    SELECT *,
           ROW_NUMBER() OVER (PARTITION BY object_id ORDER BY object_version DESC) as rn
    FROM objects_history
    WHERE checkpoint_sequence_number >= $1 AND checkpoint_sequence_number < $2
) as subquery
WHERE rn = 1
ON CONFLICT (object_id) DO UPDATE
SET object_version = EXCLUDED.object_version,
    object_status = EXCLUDED.object_status,
    object_digest = EXCLUDED.object_digest,
    checkpoint_sequence_number = EXCLUDED.checkpoint_sequence_number,
    owner_type = EXCLUDED.owner_type,
    owner_id = EXCLUDED.owner_id,
    object_type = EXCLUDED.object_type,
    serialized_object = EXCLUDED.serialized_object,
    coin_type = EXCLUDED.coin_type,
    coin_balance = EXCLUDED.coin_balance,
    df_kind = EXCLUDED.df_kind,
    df_name = EXCLUDED.df_name,
    df_object_type = EXCLUDED.df_object_type,
    df_object_id = EXCLUDED.df_object_id;
";

#[derive(Clone)]
pub struct PgIndexerStore {
    blocking_cp: PgConnectionPool,
    module_cache: Arc<SyncModuleCache<IndexerStorePackageModuleResolver>>,
    metrics: IndexerMetrics,
    parallel_chunk_size: usize,
    parallel_objects_chunk_size: usize,
    partition_manager: PgPartitionManager,
}

impl PgIndexerStore {
    pub fn new(blocking_cp: PgConnectionPool, metrics: IndexerMetrics) -> Self {
        let module_cache: Arc<SyncModuleCache<IndexerStorePackageModuleResolver>> = Arc::new(
            SyncModuleCache::new(IndexerStorePackageModuleResolver::new(blocking_cp.clone())),
        );
        let parallel_chunk_size = std::env::var("PG_COMMIT_PARALLEL_CHUNK_SIZE")
            .unwrap_or_else(|_e| PG_COMMIT_PARALLEL_CHUNK_SIZE_PER_DB_TX.to_string())
            .parse::<usize>()
            .unwrap();
        let parallel_objects_chunk_size = std::env::var("PG_COMMIT_OBJECTS_PARALLEL_CHUNK_SIZE")
            .unwrap_or_else(|_e| PG_COMMIT_OBJECTS_PARALLEL_CHUNK_SIZE_PER_DB_TX.to_string())
            .parse::<usize>()
            .unwrap();
        let partition_manager = PgPartitionManager::new(blocking_cp.clone())
            .expect("Failed to initialize partition manager");

        Self {
            blocking_cp,
            module_cache,
            metrics,
            parallel_chunk_size,
            parallel_objects_chunk_size,
            partition_manager,
        }
    }

    pub fn blocking_cp(&self) -> PgConnectionPool {
        self.blocking_cp.clone()
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

    fn get_latest_object_snapshot_checkpoint_sequence_number(
        &self,
    ) -> Result<Option<u64>, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            objects_snapshot::dsl::objects_snapshot
                .select(max(objects_snapshot::checkpoint_sequence_number))
                .first::<Option<i64>>(conn)
                .map(|v| v.map(|v| v as u64))
        })
        .context("Failed reading latest object snapshot checkpoint sequence number from PostgresDB")
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

    fn persist_display_updates(
        &self,
        display_updates: BTreeMap<String, StoredDisplay>,
    ) -> Result<(), IndexerError> {
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
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
                    .map_err(IndexerError::from)
                    .context("Failed to write display updates to PostgresDB")?;
                Ok::<(), IndexerError>(())
            },
            Duration::from_secs(60)
        )?;

        Ok(())
    }

    fn persist_objects_chunk(
        &self,
        objects: Vec<ObjectChangeToCommit>,
    ) -> Result<(), IndexerError> {
        let guard = self
            .metrics
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
                for mutated_object_change_chunk in
                    mutated_objects.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
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
                        .map_err(IndexerError::from)
                        .context("Failed to write object mutation to PostgresDB")?;
                }

                // Persist deleted objects
                for deleted_objects_chunk in
                    deleted_object_ids.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
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

    fn persist_objects_history_chunk(
        &self,
        objects: Vec<ObjectChangeToCommit>,
    ) -> Result<(), IndexerError> {
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

        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                for mutated_object_change_chunk in
                    mutated_objects.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    diesel::insert_into(objects_history::table)
                        .values(mutated_object_change_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .map_err(IndexerError::from)
                        .context("Failed to write object mutations to objects_history in DB.")?;
                }

                for deleted_objects_chunk in
                    deleted_object_ids.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX)
                {
                    diesel::insert_into(objects_history::table)
                        .values(deleted_objects_chunk)
                        .on_conflict_do_nothing()
                        .execute(conn)
                        .map_err(IndexerError::from)
                        .context("Failed to write object deletions to objects_history in DB.")?;
                }

                Ok::<(), IndexerError>(())
            },
            Duration::from_secs(60)
        )
        .tap(|_| {
            let elapsed = guard.stop_and_record();
            info!(
                elapsed,
                "Persisted {} chunked objects history",
                mutated_objects.len() + deleted_object_ids.len(),
            )
        })
    }

    fn persist_object_snapshot(&self, start_cp: u64, end_cp: u64) -> Result<(), IndexerError> {
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                RunQueryDsl::execute(
                    diesel::sql_query(UPDATE_OBJECTS_SNAPSHOT_QUERY)
                        .bind::<diesel::sql_types::BigInt, _>(start_cp as i64)
                        .bind::<diesel::sql_types::BigInt, _>(end_cp as i64),
                    conn,
                )
            },
            Duration::from_secs(10)
        )?;
        Ok(())
    }

    fn persist_checkpoints(&self, checkpoints: Vec<IndexedCheckpoint>) -> Result<(), IndexerError> {
        if checkpoints.is_empty() {
            return Ok(());
        }
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_checkpoints
            .start_timer();

        let checkpoints = checkpoints
            .iter()
            .map(StoredCheckpoint::from)
            .collect::<Vec<_>>();
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                for checkpoint_chunk in checkpoints.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
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
    ) -> Result<(), IndexerError> {
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

        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                for transaction_chunk in transactions.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
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

    fn persist_events_chunk(&self, events: Vec<IndexedEvent>) -> Result<(), IndexerError> {
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_events_chunks
            .start_timer();
        let len = events.len();
        let events = events
            .into_iter()
            .map(StoredEvent::from)
            .collect::<Vec<_>>();

        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                for event_chunk in events.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
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
            info!(elapsed, "Persisted {} chunked events", len)
        })
    }

    fn persist_packages(&self, packages: Vec<IndexedPackage>) -> Result<(), IndexerError> {
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
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                for packages_chunk in packages.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                    diesel::insert_into(packages::table)
                        .values(packages_chunk)
                        // System packages such as 0x2/0x9 will have their package_id
                        // unchanged during upgrades. In this case, we override the modules
                        // TODO: race condition is possible here. Figure out how to avoid/detect
                        .on_conflict(packages::package_id)
                        .do_update()
                        .set(packages::move_package.eq(excluded(packages::move_package)))
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

    async fn persist_tx_indices_chunk(&self, indices: Vec<TxIndex>) -> Result<(), IndexerError> {
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_tx_indices_chunks
            .start_timer();
        let len = indices.len();
        let (senders, recipients, input_objects, changed_objects, calls) =
            indices.into_iter().map(|i| i.split()).fold(
                (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()),
                |(
                    mut tx_senders,
                    mut tx_recipients,
                    mut tx_input_objects,
                    mut tx_changed_objects,
                    mut tx_calls,
                ),
                 index| {
                    tx_senders.extend(index.0);
                    tx_recipients.extend(index.1);
                    tx_input_objects.extend(index.2);
                    tx_changed_objects.extend(index.3);
                    tx_calls.extend(index.4);

                    (
                        tx_senders,
                        tx_recipients,
                        tx_input_objects,
                        tx_changed_objects,
                        tx_calls,
                    )
                },
            );

        let mut futures = vec![];
        futures.push(self.spawn_blocking_task(move |this| {
            let now = Instant::now();
            let senders_len = senders.len();
            let recipients_len = recipients.len();
            transactional_blocking_with_retry!(
                &this.blocking_cp,
                |conn| {
                    for chunk in senders.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                        diesel::insert_into(tx_senders::table)
                            .values(chunk)
                            .on_conflict_do_nothing()
                            .execute(conn)
                            .map_err(IndexerError::from)
                            .context("Failed to write tx_senders to PostgresDB")?;
                    }
                    for chunk in recipients.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                        diesel::insert_into(tx_recipients::table)
                            .values(chunk)
                            .on_conflict_do_nothing()
                            .execute(conn)
                            .map_err(IndexerError::from)
                            .context("Failed to write tx_recipients to PostgresDB")?;
                    }
                    Ok::<(), IndexerError>(())
                },
                Duration::from_secs(60)
            )
            .tap(|_| {
                let elapsed = now.elapsed().as_secs_f64();
                info!(
                    elapsed,
                    "Persisted {} rows to tx_senders and {} rows to tx_recipients",
                    senders_len,
                    recipients_len,
                );
            })
        }));
        futures.push(self.spawn_blocking_task(move |this| {
            let now = Instant::now();
            let input_objects_len = input_objects.len();
            transactional_blocking_with_retry!(
                &this.blocking_cp,
                |conn| {
                    for chunk in input_objects.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                        diesel::insert_into(tx_input_objects::table)
                            .values(chunk)
                            .on_conflict_do_nothing()
                            .execute(conn)
                            .map_err(IndexerError::from)
                            .context("Failed to write tx_input_objects chunk to PostgresDB")?;
                    }
                    Ok::<(), IndexerError>(())
                },
                Duration::from_secs(60)
            )
            .tap(|_| {
                let elapsed = now.elapsed().as_secs_f64();
                info!(
                    elapsed,
                    "Persisted {} rows to tx_input_objects", input_objects_len,
                );
            })
        }));

        futures.push(self.spawn_blocking_task(move |this| {
            let now = Instant::now();
            let changed_objects_len = changed_objects.len();
            transactional_blocking_with_retry!(
                &this.blocking_cp,
                |conn| {
                    for chunk in changed_objects.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                        diesel::insert_into(tx_changed_objects::table)
                            .values(chunk)
                            .on_conflict_do_nothing()
                            .execute(conn)
                            .map_err(IndexerError::from)
                            .context("Failed to write tx_changed_objects chunk to PostgresDB")?;
                    }
                    Ok::<(), IndexerError>(())
                },
                Duration::from_secs(60)
            )
            .tap(|_| {
                let elapsed = now.elapsed().as_secs_f64();
                info!(
                    elapsed,
                    "Persisted {} rows to tx_changed_objects table", changed_objects_len,
                );
            })
        }));
        futures.push(self.spawn_blocking_task(move |this| {
            let now = Instant::now();
            let calls_len = calls.len();
            transactional_blocking_with_retry!(
                &this.blocking_cp,
                |conn| {
                    for chunk in calls.chunks(PG_COMMIT_CHUNK_SIZE_INTRA_DB_TX) {
                        diesel::insert_into(tx_calls::table)
                            .values(chunk)
                            .on_conflict_do_nothing()
                            .execute(conn)
                            .map_err(IndexerError::from)
                            .context("Failed to write tx_calls chunk to PostgresDB")?;
                    }
                    Ok::<(), IndexerError>(())
                },
                Duration::from_secs(60)
            )
            .tap(|_| {
                let elapsed = now.elapsed().as_secs_f64();
                info!(elapsed, "Persisted {} rows to tx_calls tables", calls_len);
            })
        }));
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
        info!(elapsed, "Persisted {} chunked tx_indices", len);
        Ok(())
    }

    fn persist_epoch(&self, epoch: EpochToCommit) -> Result<(), IndexerError> {
        let guard = self
            .metrics
            .checkpoint_db_commit_latency_epoch
            .start_timer();
        let epoch_id = epoch.new_epoch.epoch;
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                if let Some(last_epoch) = &epoch.last_epoch {
                    let last_epoch_id = last_epoch.epoch;
                    let last_epoch = StoredEpochInfo::from_epoch_end_info(last_epoch);
                    info!(last_epoch_id, "Persisting epoch end data: {:?}", last_epoch);
                    diesel::insert_into(epochs::table)
                        .values(last_epoch)
                        .on_conflict(epochs::epoch)
                        .do_update()
                        .set((
                            // Note: Exclude epoch beginning info except system_state below.
                            // This is to ensure that epoch beginning info columns are not overridden with default values,
                            // because these columns are default values in `last_epoch`.
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
                        .execute(conn)?;
                }
                let epoch_id = epoch.new_epoch.epoch;
                info!(epoch_id, "Persisting epoch beginning info");
                let new_epoch = StoredEpochInfo::from_epoch_beginning_info(&epoch.new_epoch);
                diesel::insert_into(epochs::table)
                    .values(new_epoch)
                    .on_conflict_do_nothing()
                    .execute(conn)?;
                Ok::<(), IndexerError>(())
            },
            Duration::from_secs(60)
        )
        .tap(|_| {
            let elapsed = guard.stop_and_record();
            info!(elapsed, epoch_id, "Persisted epoch beginning info");
        })
    }

    fn advance_epoch(&self, epoch_to_commit: EpochToCommit) -> Result<(), IndexerError> {
        let last_epoch_id = epoch_to_commit.last_epoch.as_ref().map(|e| e.epoch);
        // partition_0 has been created, so no need to advance it.
        if let Some(last_epoch_id) = last_epoch_id {
            let last_db_epoch: Option<StoredEpochInfo> =
                read_only_blocking!(&self.blocking_cp, |conn| {
                    epochs::table
                        .filter(epochs::epoch.eq(last_epoch_id as i64))
                        .first::<StoredEpochInfo>(conn)
                        .optional()
                })
                .context("Failed to read last epoch from PostgresDB")?;
            if let Some(last_epoch) = last_db_epoch {
                let epoch_partition_data =
                    EpochPartitionData::compose_data(epoch_to_commit, last_epoch);
                let table_partitions = self.partition_manager.get_table_partitions()?;
                for (table, last_partition) in table_partitions {
                    let guard = self.metrics.advance_epoch_latency.start_timer();
                    self.partition_manager.advance_table_epoch_partition(
                        table.clone(),
                        last_partition,
                        &epoch_partition_data,
                    )?;
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

    fn get_network_total_transactions_by_end_of_epoch(
        &self,
        epoch: u64,
    ) -> Result<u64, IndexerError> {
        read_only_blocking!(&self.blocking_cp, |conn| {
            checkpoints::table
                .filter(checkpoints::epoch.eq(epoch as i64))
                .select(max(checkpoints::network_total_transactions))
                .first::<Option<i64>>(conn)
                .map(|o| o.unwrap_or(0))
        })
        .context("Failed to get network total transactions in epoch")
        .map(|v| v as u64)
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
        let guard = self.metrics.tokio_blocking_task_wait_latency.start_timer();
        tokio::task::spawn_blocking(move || {
            let _guard = current_span.enter();
            let _elapsed = guard.stop_and_record();
            f(this)
        })
    }

    fn spawn_task<F, Fut, R>(&self, f: F) -> tokio::task::JoinHandle<Result<R, IndexerError>>
    where
        F: FnOnce(Self) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<R, IndexerError>> + Send + 'static,
        R: Send + 'static,
    {
        let this = self.clone();
        tokio::task::spawn(async move { f(this).await })
    }
}

#[async_trait]
impl IndexerStore for PgIndexerStore {
    type ModuleCache = SyncModuleCache<IndexerStorePackageModuleResolver>;

    async fn get_latest_tx_checkpoint_sequence_number(&self) -> Result<Option<u64>, IndexerError> {
        self.execute_in_blocking_worker(|this| this.get_latest_tx_checkpoint_sequence_number())
            .await
    }

    async fn get_latest_object_snapshot_checkpoint_sequence_number(
        &self,
    ) -> Result<Option<u64>, IndexerError> {
        self.execute_in_blocking_worker(|this| {
            this.get_latest_object_snapshot_checkpoint_sequence_number()
        })
        .await
    }

    async fn get_object_read(
        &self,
        object_id: ObjectID,
        version: Option<SequenceNumber>,
    ) -> Result<ObjectRead, IndexerError> {
        self.execute_in_blocking_worker(move |this| this.get_object_read(object_id, version))
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
        let chunks = chunk!(objects, self.parallel_objects_chunk_size);
        let futures = chunks
            .into_iter()
            .map(|c| self.spawn_blocking_task(move |this| this.persist_objects_chunk(c)))
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
        let chunks = chunk!(objects, self.parallel_objects_chunk_size);
        let futures = chunks
            .into_iter()
            .map(|c| self.spawn_blocking_task(move |this| this.persist_objects_history_chunk(c)))
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

    async fn persist_object_snapshot(
        &self,
        start_cp: u64,
        end_cp: u64,
    ) -> Result<(), IndexerError> {
        let skip_snapshot = std::env::var("SKIP_OBJECT_SNAPSHOT")
            .map(|val| val.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if skip_snapshot {
            info!("skipping object snapshot");
            return Ok(());
        }

        let guard = self.metrics.update_object_snapshot_latency.start_timer();

        self.spawn_blocking_task(move |this| this.persist_object_snapshot(start_cp, end_cp))
            .await
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed to update objects snapshot: {:?}",
                    e
                ))
            })??;
        let elapsed = guard.stop_and_record();
        info!(
            elapsed,
            "Persisted snapshot for checkpoints from {} to {}", start_cp, end_cp
        );
        Ok(())
    }

    async fn persist_checkpoints(
        &self,
        checkpoints: Vec<IndexedCheckpoint>,
    ) -> Result<(), IndexerError> {
        self.execute_in_blocking_worker(move |this| this.persist_checkpoints(checkpoints))
            .await
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

        let chunks = chunk!(transactions, self.parallel_chunk_size);
        let futures = chunks
            .into_iter()
            .map(|c| self.spawn_blocking_task(move |this| this.persist_transactions_chunk(c)))
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
        let chunks = chunk!(events, self.parallel_chunk_size);
        let futures = chunks
            .into_iter()
            .map(|c| self.spawn_blocking_task(move |this| this.persist_events_chunk(c)))
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

        self.spawn_blocking_task(move |this| this.persist_display_updates(display_updates))
            .await?
    }

    async fn persist_packages(&self, packages: Vec<IndexedPackage>) -> Result<(), IndexerError> {
        if packages.is_empty() {
            return Ok(());
        }
        self.execute_in_blocking_worker(move |this| this.persist_packages(packages))
            .await
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
        let chunks = chunk!(indices, self.parallel_chunk_size);

        let futures = chunks
            .into_iter()
            .map(|chunk| {
                self.spawn_task(move |this: Self| async move {
                    this.persist_tx_indices_chunk(chunk).await
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

    async fn persist_epoch(&self, epoch: EpochToCommit) -> Result<(), IndexerError> {
        self.execute_in_blocking_worker(move |this| this.persist_epoch(epoch))
            .await
    }

    async fn advance_epoch(&self, epoch: EpochToCommit) -> Result<(), IndexerError> {
        self.execute_in_blocking_worker(move |this| this.advance_epoch(epoch))
            .await
    }

    async fn get_network_total_transactions_by_end_of_epoch(
        &self,
        epoch: u64,
    ) -> Result<u64, IndexerError> {
        self.execute_in_blocking_worker(move |this| {
            this.get_network_total_transactions_by_end_of_epoch(epoch)
        })
        .await
    }

    fn module_cache(&self) -> Arc<Self::ModuleCache> {
        self.module_cache.clone()
    }

    fn as_any(&self) -> &dyn Any {
        self
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
        if deleted_objects.contains_key(&object.object_id) {
            continue;
        }
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
