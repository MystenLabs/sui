// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::sql_types::{BigInt, VarChar};
use diesel::QueryableByName;
use diesel_async::scoped_futures::ScopedFutureExt;
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;
use tracing::{error, info};

use crate::database::ConnectionPool;
use crate::errors::IndexerError;
use crate::handlers::EpochToCommit;
use crate::models::epoch::StoredEpochInfo;
use crate::store::transaction_with_retry;

const GET_PARTITION_SQL: &str = r"
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
";

#[derive(Clone)]
pub struct PgPartitionManager {
    pool: ConnectionPool,

    partition_strategies: HashMap<&'static str, PgPartitionStrategy>,
}

#[derive(Clone, Copy)]
pub enum PgPartitionStrategy {
    CheckpointSequenceNumber,
    TxSequenceNumber,
    ObjectId,
}

impl PgPartitionStrategy {
    pub fn is_epoch_partitioned(&self) -> bool {
        matches!(
            self,
            Self::CheckpointSequenceNumber | Self::TxSequenceNumber
        )
    }
}

#[derive(Clone, Debug)]
pub struct EpochPartitionData {
    last_epoch: u64,
    next_epoch: u64,
    last_epoch_start_cp: u64,
    next_epoch_start_cp: u64,
    last_epoch_start_tx: u64,
    next_epoch_start_tx: u64,
}

impl EpochPartitionData {
    pub fn compose_data(epoch: EpochToCommit, last_db_epoch: StoredEpochInfo) -> Self {
        let last_epoch = last_db_epoch.epoch as u64;
        let last_epoch_start_cp = last_db_epoch.first_checkpoint_id as u64;
        let next_epoch = epoch.new_epoch_id();
        let next_epoch_start_cp = epoch.new_epoch_first_checkpoint_id();
        let next_epoch_start_tx = epoch.new_epoch_first_tx_sequence_number();
        let last_epoch_start_tx =
            next_epoch_start_tx - epoch.last_epoch_total_transactions().unwrap();

        Self {
            last_epoch,
            next_epoch,
            last_epoch_start_cp,
            next_epoch_start_cp,
            last_epoch_start_tx,
            next_epoch_start_tx,
        }
    }
}

impl PgPartitionManager {
    pub fn new(pool: ConnectionPool) -> Result<Self, IndexerError> {
        let mut partition_strategies = HashMap::new();
        partition_strategies.insert("events", PgPartitionStrategy::TxSequenceNumber);
        partition_strategies.insert("transactions", PgPartitionStrategy::TxSequenceNumber);
        partition_strategies.insert("objects_version", PgPartitionStrategy::ObjectId);
        let manager = Self {
            pool,
            partition_strategies,
        };
        Ok(manager)
    }

    pub async fn get_table_partitions(&self) -> Result<BTreeMap<String, (u64, u64)>, IndexerError> {
        #[derive(QueryableByName, Debug, Clone)]
        struct PartitionedTable {
            #[diesel(sql_type = VarChar)]
            table_name: String,
            #[diesel(sql_type = BigInt)]
            first_partition: i64,
            #[diesel(sql_type = BigInt)]
            last_partition: i64,
        }

        let mut connection = self.pool.get().await?;

        Ok(
            diesel_async::RunQueryDsl::load(diesel::sql_query(GET_PARTITION_SQL), &mut connection)
                .await?
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
    pub fn get_strategy(&self, table_name: &str) -> PgPartitionStrategy {
        self.partition_strategies
            .get(table_name)
            .copied()
            .unwrap_or(PgPartitionStrategy::CheckpointSequenceNumber)
    }

    pub fn determine_epoch_partition_range(
        &self,
        table_name: &str,
        data: &EpochPartitionData,
    ) -> Option<(u64, u64)> {
        match self.get_strategy(table_name) {
            PgPartitionStrategy::CheckpointSequenceNumber => {
                Some((data.last_epoch_start_cp, data.next_epoch_start_cp))
            }
            PgPartitionStrategy::TxSequenceNumber => {
                Some((data.last_epoch_start_tx, data.next_epoch_start_tx))
            }
            PgPartitionStrategy::ObjectId => None,
        }
    }

    pub async fn advance_epoch(
        &self,
        table: String,
        last_partition: u64,
        data: &EpochPartitionData,
    ) -> Result<(), IndexerError> {
        let Some(partition_range) = self.determine_epoch_partition_range(&table, data) else {
            return Ok(());
        };
        if data.next_epoch == 0 {
            tracing::info!("Epoch 0 partition has been created in the initial setup.");
            return Ok(());
        }
        if last_partition == data.last_epoch {
            transaction_with_retry(&self.pool, Duration::from_secs(10), |conn| {
                async {
                    diesel_async::RunQueryDsl::execute(
                        diesel::sql_query("CALL advance_partition($1, $2, $3, $4, $5)")
                            .bind::<diesel::sql_types::Text, _>(table.clone())
                            .bind::<diesel::sql_types::BigInt, _>(data.last_epoch as i64)
                            .bind::<diesel::sql_types::BigInt, _>(data.next_epoch as i64)
                            .bind::<diesel::sql_types::BigInt, _>(partition_range.0 as i64)
                            .bind::<diesel::sql_types::BigInt, _>(partition_range.1 as i64),
                        conn,
                    )
                    .await?;

                    Ok(())
                }
                .scope_boxed()
            })
            .await?;

            info!(
                "Advanced epoch partition for table {} from {} to {}, prev partition upper bound {}",
                table, last_partition, data.next_epoch, partition_range.0
            );
        } else if last_partition != data.next_epoch {
            // skip when the partition is already advanced once, which is possible when indexer
            // crashes and restarts; error otherwise.
            error!(
                "Epoch partition for table {} is not in sync with the last epoch {}.",
                table, data.last_epoch
            );
        } else {
            info!(
                "Epoch has been advanced to {} already, skipping.",
                data.next_epoch
            );
        }
        Ok(())
    }

    pub async fn drop_table_partition(
        &self,
        table: String,
        partition: u64,
    ) -> Result<(), IndexerError> {
        transaction_with_retry(&self.pool, Duration::from_secs(10), |conn| {
            async {
                diesel_async::RunQueryDsl::execute(
                    diesel::sql_query("CALL drop_partition($1, $2)")
                        .bind::<diesel::sql_types::Text, _>(table.clone())
                        .bind::<diesel::sql_types::BigInt, _>(partition as i64),
                    conn,
                )
                .await?;
                Ok(())
            }
            .scope_boxed()
        })
        .await?;
        Ok(())
    }
}
