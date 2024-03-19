// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::sql_types::{BigInt, VarChar};
use diesel::{QueryableByName, RunQueryDsl};
use std::collections::BTreeMap;
use std::time::Duration;
use tracing::{error, info};

use crate::db::PgConnectionPool;
use crate::handlers::EpochToCommit;
use crate::models::epoch::StoredEpochInfo;
use crate::store::diesel_macro::{read_only_blocking, transactional_blocking_with_retry};
use crate::IndexerError;

const GET_PARTITION_SQL: &str = r"
SELECT parent.relname                                            AS table_name,
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
    cp: PgConnectionPool,
}

#[derive(Clone, Debug)]
pub struct EpochPartitionData {
    last_epoch: u64,
    next_epoch: u64,
    last_epoch_start_cp: u64,
    next_epoch_start_cp: u64,
}

impl EpochPartitionData {
    pub fn compose_data(epoch: EpochToCommit, last_db_epoch: StoredEpochInfo) -> Self {
        let last_epoch = last_db_epoch.epoch as u64;
        let last_epoch_start_cp = last_db_epoch.first_checkpoint_id as u64;
        let next_epoch = epoch.new_epoch.epoch;
        let next_epoch_start_cp = epoch.new_epoch.first_checkpoint_id;
        Self {
            last_epoch,
            next_epoch,
            last_epoch_start_cp,
            next_epoch_start_cp,
        }
    }
}

impl PgPartitionManager {
    pub fn new(cp: PgConnectionPool) -> Result<Self, IndexerError> {
        let manager = Self { cp };
        let tables = manager.get_table_partitions()?;
        info!(
            "Found {} tables with partitions : [{:?}]",
            tables.len(),
            tables
        );
        Ok(manager)
    }

    pub fn get_table_partitions(&self) -> Result<BTreeMap<String, u64>, IndexerError> {
        #[derive(QueryableByName, Debug, Clone)]
        struct PartitionedTable {
            #[diesel(sql_type = VarChar)]
            table_name: String,
            #[diesel(sql_type = BigInt)]
            last_partition: i64,
        }

        Ok(
            read_only_blocking!(&self.cp, |conn| diesel::RunQueryDsl::load(
                diesel::sql_query(GET_PARTITION_SQL),
                conn
            ))?
            .into_iter()
            .map(|table: PartitionedTable| (table.table_name, table.last_partition as u64))
            .collect(),
        )
    }

    pub fn advance_table_epoch_partition(
        &self,
        table: String,
        last_partition: u64,
        data: &EpochPartitionData,
    ) -> Result<(), IndexerError> {
        if data.next_epoch == 0 {
            tracing::info!("Epoch 0 partition has been crate in migrations, skipped.");
            return Ok(());
        }
        if last_partition == data.last_epoch {
            transactional_blocking_with_retry!(
                &self.cp,
                |conn| {
                    RunQueryDsl::execute(
                        diesel::sql_query("CALL advance_partition($1, $2, $3, $4, $5)")
                            .bind::<diesel::sql_types::Text, _>(table.clone())
                            .bind::<diesel::sql_types::BigInt, _>(data.last_epoch as i64)
                            .bind::<diesel::sql_types::BigInt, _>(data.next_epoch as i64)
                            .bind::<diesel::sql_types::BigInt, _>(data.last_epoch_start_cp as i64)
                            .bind::<diesel::sql_types::BigInt, _>(data.next_epoch_start_cp as i64),
                        conn,
                    )
                },
                Duration::from_secs(10)
            )?;
            info!(
                "Advanced epoch partition for table {} from {} to {}",
                table, last_partition, data.next_epoch
            );
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
