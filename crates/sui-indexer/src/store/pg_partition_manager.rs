// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use diesel::sql_types::VarChar;
use diesel::{QueryableByName, RunQueryDsl};
use std::collections::BTreeMap;
use std::str::FromStr;
use tracing::info;

use crate::store::diesel_marco::{read_only_blocking, transactional_blocking};
use crate::store::TemporaryEpochStore;
use crate::IndexerError;
use crate::PgConnectionPool;

const GET_PARTITION_SQL: &str = r#"
SELECT parent.relname                           AS table_name,
       MAX(SUBSTRING(child.relname FROM '\d$')) AS last_partition
FROM pg_inherits
         JOIN pg_class parent ON pg_inherits.inhparent = parent.oid
         JOIN pg_class child ON pg_inherits.inhrelid = child.oid
         JOIN pg_namespace nmsp_parent ON nmsp_parent.oid = parent.relnamespace
         JOIN pg_namespace nmsp_child ON nmsp_child.oid = child.relnamespace
WHERE parent.relkind = 'p'
GROUP BY table_name;
"#;

#[derive(Clone)]
pub struct PgPartitionManager {
    cp: PgConnectionPool,
}

#[derive(Clone)]
pub struct EpochPartitionData {
    last_epoch: u64,
    next_epoch: u64,
    last_epoch_start_cp: u64,
    next_epoch_start_cp: u64,
}

impl From<TemporaryEpochStore> for EpochPartitionData {
    fn from(data: TemporaryEpochStore) -> Self {
        let last_epoch = data.last_epoch;
        Self {
            last_epoch: last_epoch.clone().map_or(0, |e| e.epoch as u64),
            next_epoch: data.new_epoch.epoch as u64,
            last_epoch_start_cp: last_epoch.map_or(0, |e| e.first_checkpoint_id as u64),
            next_epoch_start_cp: data.new_epoch.first_checkpoint_id as u64,
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

    pub fn advance_table_epoch_partition(
        &self,
        table: &str,
        data: &EpochPartitionData,
    ) -> Result<(), IndexerError> {
        let (last_epoch, next_epoch, last_epoch_start_cp, next_epoch_start_cp) = (
            data.last_epoch,
            data.next_epoch,
            data.last_epoch_start_cp,
            data.next_epoch_start_cp,
        );
        let detach_last_partition =
            format!("ALTER TABLE {table} DETACH PARTITION {table}_partition_{last_epoch};");
        transactional_blocking!(&self.cp, |conn| {
            RunQueryDsl::execute(diesel::sql_query(detach_last_partition), conn)
        })?;

        let create_new_partition = format!(
            "CREATE TABLE {table}_partition_{next_epoch} PARTITION OF {table}
            FOR VALUES FROM ({next_epoch_start_cp}) TO (MAXVALUE);"
        );
        let move_rows_to_new_partition = format!(
            "INSERT INTO {table}_partition_{next_epoch} SELECT * FROM {table}_partition_{last_epoch} WHERE checkpoint >= {next_epoch_start_cp};"
        );
        // until this tx finishes, concurrent writes will fail and keep retrying.
        transactional_blocking!(&self.cp, |conn| {
            RunQueryDsl::execute(diesel::sql_query(create_new_partition), conn)?;
            RunQueryDsl::execute(diesel::sql_query(move_rows_to_new_partition), conn)
        })?;

        let delete_rows_from_old_partition = format!(
            "DELETE FROM {table}_partition_{last_epoch} WHERE checkpoint >= {next_epoch_start_cp};"
        );
        let reattach_last_partition = format!(
            "ALTER TABLE {table} ATTACH PARTITION {table}_partition_{last_epoch} FOR VALUES FROM ({last_epoch_start_cp}) TO ({next_epoch_start_cp});"
        );
        transactional_blocking!(&self.cp, |conn| {
            RunQueryDsl::execute(diesel::sql_query(delete_rows_from_old_partition), conn)?;
            RunQueryDsl::execute(diesel::sql_query(reattach_last_partition), conn)
        })?;
        Ok(())
    }

    pub fn get_table_partitions(&self) -> Result<BTreeMap<String, u64>, IndexerError> {
        #[derive(QueryableByName, Debug, Clone)]
        struct PartitionedTable {
            #[diesel(sql_type = VarChar)]
            table_name: String,
            #[diesel(sql_type = VarChar)]
            last_partition: String,
        }

        Ok(
            read_only_blocking!(&self.cp, |conn| diesel::RunQueryDsl::load(
                diesel::sql_query(GET_PARTITION_SQL),
                conn
            ))?
            .into_iter()
            .map(|table: PartitionedTable| {
                u64::from_str(&table.last_partition)
                    .map(|last_partition| (table.table_name, last_partition))
                    .map_err(|e| anyhow!(e))
            })
            .collect::<Result<_, _>>()?,
        )
    }
}
