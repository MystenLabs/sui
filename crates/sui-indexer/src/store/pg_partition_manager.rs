// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// use diesel::query_dsl::methods::FilterDsl;
use diesel::r2d2::R2D2Connection;
use diesel::sql_types::{BigInt, VarChar};
use diesel::QueryDsl;
use diesel::{ExpressionMethods, QueryableByName, RunQueryDsl};
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;
use tracing::{error, info};

use crate::db::ConnectionPool;
use crate::errors::{Context, IndexerError};
use crate::handlers::EpochToCommit;
use crate::models::epoch::StoredEpochInfo;
use crate::schema::cp_tx;
use crate::store::diesel_macro::*;
use downcast::Any;

const GET_PARTITION_SQL: &str = if cfg!(feature = "postgres-feature") {
    r"
SELECT parent.relname                                            AS table_name,
       MIN(CAST(SUBSTRING(child.relname FROM '\d+$') AS BIGINT)) AS first_partition,
       MAX(CAST(SUBSTRING(child.relname FROM '\d+$') AS BIGINT)) AS last_partition
FROM pg_inherits
         JOIN pg_class parent ON pg_inherits.inhparent = parent.oid
         JOIN pg_class child ON pg_inherits.inhrelid = child.oid
         JOIN pg_namespace nmsp_parent ON nmsp_parent.oid = parent.relnamespace
         JOIN pg_namespace nmsp_child ON nmsp_child.oid = child.relnamespace
WHERE parent.relkind = 'p'
GROUP BY table_name;
"
} else if cfg!(feature = "mysql-feature") && cfg!(not(feature = "postgres-feature")) {
    r"
SELECT TABLE_NAME AS table_name,
       MIN(CAST(SUBSTRING_INDEX(PARTITION_NAME, '_', -1) AS UNSIGNED)) AS first_partition,
       MAX(CAST(SUBSTRING_INDEX(PARTITION_NAME, '_', -1) AS UNSIGNED)) AS last_partition
FROM information_schema.PARTITIONS
WHERE TABLE_SCHEMA = DATABASE()
AND PARTITION_NAME IS NOT NULL
GROUP BY table_name;
"
} else {
    ""
};

pub struct PgPartitionManager<T: R2D2Connection + 'static> {
    cp: ConnectionPool<T>,
    partition_strategies: HashMap<&'static str, PgPartitionStrategy>,
}

impl<T: R2D2Connection> Clone for PgPartitionManager<T> {
    fn clone(&self) -> PgPartitionManager<T> {
        Self {
            cp: self.cp.clone(),
            partition_strategies: self.partition_strategies.clone(),
        }
    }
}

#[derive(Clone)]
pub enum PgPartitionStrategy {
    CheckpointSequenceNumber,
    TxSequenceNumber,
}

#[derive(Clone, Debug)]
pub struct EpochPartitionData {
    last_epoch: u64,
    next_epoch: u64,
    last_epoch_start: u64,
    next_epoch_start: u64,
}

impl EpochPartitionData {
    pub fn compose_data(epoch: EpochToCommit, last_db_epoch: StoredEpochInfo) -> Self {
        let last_epoch = last_db_epoch.epoch as u64;
        let last_epoch_start = last_db_epoch.first_checkpoint_id as u64;
        let next_epoch = epoch.new_epoch.epoch;
        let next_epoch_start = epoch.new_epoch.first_checkpoint_id;
        Self {
            last_epoch,
            next_epoch,
            last_epoch_start,
            next_epoch_start,
        }
    }

    pub fn update_with_tx_sequence_number(&mut self, start: u64, end: u64) {
        self.last_epoch_start = start;
        self.next_epoch_start = end;
    }
}

impl<T: R2D2Connection> PgPartitionManager<T> {
    pub fn new(cp: ConnectionPool<T>) -> Result<Self, IndexerError> {
        let mut partition_strategies = HashMap::new();
        partition_strategies.insert("transactions", PgPartitionStrategy::TxSequenceNumber);
        let manager = Self {
            cp,
            partition_strategies,
        };
        let tables = manager.get_table_partitions()?;
        info!(
            "Found {} tables with partitions : [{:?}]",
            tables.len(),
            tables
        );
        Ok(manager)
    }

    pub fn get_table_partitions(&self) -> Result<BTreeMap<String, (u64, u64)>, IndexerError> {
        #[derive(QueryableByName, Debug, Clone)]
        struct PartitionedTable {
            #[diesel(sql_type = VarChar)]
            table_name: String,
            #[diesel(sql_type = BigInt)]
            first_partition: i64,
            #[diesel(sql_type = BigInt)]
            last_partition: i64,
        }

        Ok(
            read_only_blocking!(&self.cp, |conn| diesel::RunQueryDsl::load(
                diesel::sql_query(GET_PARTITION_SQL),
                conn
            ))?
            .into_iter()
            .map(|table: PartitionedTable| {
                (
                    table.table_name,
                    (table.first_partition as u64, table.last_partition as u64),
                )
            })
            .collect(),
        )
    }

    /// Tries to fetch the partitioning strategy for the given partitioned table. Defaults to
    /// `CheckpointSequenceNumber` as the majority of our tables are partitioned on an epoch's
    /// checkpoints today.
    pub fn get_strategy(&self, table_name: &str) -> &PgPartitionStrategy {
        self.partition_strategies
            .get(table_name)
            .unwrap_or(&PgPartitionStrategy::CheckpointSequenceNumber)
    }

    pub fn apply_strategy(
        &self,
        table_name: &str,
        data: &mut EpochPartitionData,
    ) -> Result<(), IndexerError> {
        match self.get_strategy(table_name) {
            PgPartitionStrategy::CheckpointSequenceNumber => Ok(()),
            PgPartitionStrategy::TxSequenceNumber => {
                let last_epoch_start = read_only_blocking!(&self.cp, |conn| {
                    cp_tx::table
                        .filter(cp_tx::checkpoint_sequence_number.eq(data.last_epoch_start as i64))
                        .select(cp_tx::min_tx_sequence_number)
                        .first::<i64>(conn)
                })
                .context("Failed to fetch last epoch start tx")
                .map(|v| v as u64)?;

                let next_epoch_start = read_only_blocking!(&self.cp, |conn| {
                    cp_tx::table
                        .filter(
                            cp_tx::checkpoint_sequence_number.eq(data.next_epoch_start as i64 - 1),
                        )
                        .select(cp_tx::max_tx_sequence_number)
                        .first::<i64>(conn)
                })
                .context("Failed to fetch next epoch start tx")
                .map(|v| v as u64)?
                    + 1; // in postgres, upper bound is exclusive

                data.update_with_tx_sequence_number(last_epoch_start, next_epoch_start);

                Ok(())
            }
        }
    }

    pub fn advance_and_prune_epoch_partition(
        &self,
        table: String,
        first_partition: u64,
        last_partition: u64,
        data: &EpochPartitionData,
        epochs_to_keep: Option<u64>,
    ) -> Result<(), IndexerError> {
        if data.next_epoch == 0 {
            tracing::info!("Epoch 0 partition has been created in the initial setup.");
            return Ok(());
        }
        if last_partition == data.last_epoch {
            #[cfg(feature = "postgres-feature")]
            transactional_blocking_with_retry!(
                &self.cp,
                |conn| {
                    RunQueryDsl::execute(
                        diesel::sql_query("CALL advance_partition($1, $2, $3, $4, $5)")
                            .bind::<diesel::sql_types::Text, _>(table.clone())
                            .bind::<diesel::sql_types::BigInt, _>(data.last_epoch as i64)
                            .bind::<diesel::sql_types::BigInt, _>(data.next_epoch as i64)
                            .bind::<diesel::sql_types::BigInt, _>(data.last_epoch_start as i64)
                            .bind::<diesel::sql_types::BigInt, _>(data.next_epoch_start as i64),
                        conn,
                    )
                },
                Duration::from_secs(10)
            )?;
            #[cfg(feature = "mysql-feature")]
            #[cfg(not(feature = "postgres-feature"))]
            transactional_blocking_with_retry!(
                &self.cp,
                |conn| {
                    RunQueryDsl::execute(diesel::sql_query(format!("ALTER TABLE {table_name} REORGANIZE PARTITION {table_name}_partition_{last_epoch} INTO (PARTITION {table_name}_partition_{last_epoch} VALUES LESS THAN ({next_epoch_start}), PARTITION {table_name}_partition_{next_epoch} VALUES LESS THAN MAXVALUE)", table_name = table.clone(), last_epoch = data.last_epoch as i64, next_epoch_start = data.next_epoch_start as i64, next_epoch = data.next_epoch as i64)), conn)
                },
                Duration::from_secs(10)
            )?;
            info!(
                "Advanced epoch partition for table {} from {} to {}, prev partition upper bound {}",
                table, last_partition, data.next_epoch, data.last_epoch_start
            );

            // prune old partitions beyond the retention period
            if let Some(epochs_to_keep) = epochs_to_keep {
                for epoch in first_partition..(data.next_epoch - epochs_to_keep + 1) {
                    #[cfg(feature = "postgres-feature")]
                    transactional_blocking_with_retry!(
                        &self.cp,
                        |conn| {
                            RunQueryDsl::execute(
                                diesel::sql_query("CALL drop_partition($1, $2)")
                                    .bind::<diesel::sql_types::Text, _>(table.clone())
                                    .bind::<diesel::sql_types::BigInt, _>(epoch as i64),
                                conn,
                            )
                        },
                        Duration::from_secs(10)
                    )?;
                    #[cfg(feature = "mysql-feature")]
                    #[cfg(not(feature = "postgres-feature"))]
                    transactional_blocking_with_retry!(
                        &self.cp,
                        |conn| {
                            RunQueryDsl::execute(
                                diesel::sql_query(format!(
                                    "ALTER TABLE {} DROP PARTITION partition_{}",
                                    table.clone(),
                                    epoch
                                )),
                                conn,
                            )
                        },
                        Duration::from_secs(10)
                    )?;
                    info!("Dropped epoch partition {} for table {}", epoch, table);
                }
            }
        } else if last_partition != data.next_epoch {
            // skip when the partition is already advanced once, which is possible when indexer
            // crashes and restarts; error otherwise.
            error!(
                "Epoch partition for table {} is not in sync with the last epoch {}.",
                table, data.last_epoch
            );
        }
        Ok(())
    }
}
