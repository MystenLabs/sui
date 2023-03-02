// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::models::checkpoints::Checkpoint;
use crate::models::error_logs::commit_error_logs;
use crate::schema::addresses::account_address;
use crate::schema::checkpoints::dsl::checkpoints as checkpoints_table;
use crate::schema::checkpoints::sequence_number;
use crate::schema::{addresses, events, objects, owner_changes, transactions};
use crate::store::indexer_store::TemporaryCheckpointStore;
use crate::store::{IndexerStore, TemporaryEpochStore};
use crate::{get_pg_pool_connection, PgConnectionPool};
use async_trait::async_trait;
use diesel::dsl::max;
use diesel::sql_types::VarChar;
use diesel::ExpressionMethods;
use diesel::QueryableByName;
use diesel::{QueryDsl, RunQueryDsl};
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_types::committee::EpochId;
use tracing::{error, info};

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
pub struct PgIndexerStore {
    cp: Arc<PgConnectionPool>,
    partition_manager: PartitionManager,
}

impl PgIndexerStore {
    pub fn new(cp: Arc<PgConnectionPool>) -> Self {
        PgIndexerStore {
            cp: cp.clone(),
            partition_manager: PartitionManager::new(cp).unwrap(),
        }
    }
}

#[async_trait]
impl IndexerStore for PgIndexerStore {
    fn get_latest_checkpoint_sequence_number(&self) -> Result<i64, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| {
                checkpoints_table
                    .select(max(sequence_number))
                    .first::<Option<i64>>(conn)
                    // -1 to differentiate between no checkpoints and the first checkpoint
                    .map(|o| o.unwrap_or(-1))
            })
            .map_err(|e| {
                IndexerError::PostgresReadError(format!(
                    "Failed reading latest checkpoint sequence number in PostgresDB with error {:?}",
                    e
                ))
            })
    }

    fn get_checkpoint(&self, checkpoint_sequence_number: i64) -> Result<Checkpoint, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| {
                checkpoints_table
                    .filter(sequence_number.eq(checkpoint_sequence_number))
                    .limit(1)
                    .first::<Checkpoint>(conn)
            })
            .map_err(|e| {
                IndexerError::PostgresReadError(format!(
                    "Failed reading previous checkpoint in PostgresDB with error {:?}",
                    e
                ))
            })
    }

    fn persist_checkpoint(&self, data: &TemporaryCheckpointStore) -> Result<usize, IndexerError> {
        let TemporaryCheckpointStore {
            checkpoint,
            transactions,
            events,
            objects,
            owner_changes,
            addresses,
            // TODO: store raw object
        } = data;

        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;

        // Commit indexed checkpoint in one transaction
        pg_pool_conn
            .build_transaction()
            .serializable()
            .read_write()
            .run(|conn| {
                diesel::insert_into(checkpoints_table)
                    .values(checkpoint)
                    .execute(conn)?;

                diesel::insert_into(transactions::table)
                    .values(transactions)
                    .execute(conn)?;

                diesel::insert_into(events::table)
                    .values(events)
                    .execute(conn)?;

                diesel::insert_into(objects::table)
                    .values(objects)
                    .execute(conn)?;

                diesel::insert_into(owner_changes::table)
                    .values(owner_changes)
                    .execute(conn)?;

                // Only insert once for address, skip if conflict
                diesel::insert_into(addresses::table)
                    .values(addresses)
                    .on_conflict(account_address)
                    .do_nothing()
                    .execute(conn)

            })
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed writing transactions to PostgresDB with transactions {:?} and error: {:?}",
                    transactions, e
                ))
            })
    }

    fn persist_epoch(&self, _data: &TemporaryEpochStore) -> Result<usize, IndexerError> {
        // TODO: create new partition on epoch change
        self.partition_manager.advance_epoch(1)
    }

    fn log_errors(&self, errors: Vec<IndexerError>) -> Result<(), IndexerError> {
        if !errors.is_empty() {
            let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
            let new_error_logs = errors.into_iter().map(|e| e.into()).collect();
            if let Err(e) = commit_error_logs(&mut pg_pool_conn, new_error_logs) {
                error!("Failed writing error logs with error {:?}", e);
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct PartitionManager {
    cp: Arc<PgConnectionPool>,
    tables: Vec<String>,
}

impl PartitionManager {
    fn new(cp: Arc<PgConnectionPool>) -> Result<Self, IndexerError> {
        // Find all tables with partition
        let mut manager = Self { cp, tables: vec![] };
        let tables = manager.get_table_partitions()?;
        info!(
            "Found {} tables with partitions : [{:?}]",
            tables.len(),
            tables
        );
        for (table, _) in tables {
            manager.tables.push(table)
        }
        Ok(manager)
    }
    fn advance_epoch(&self, next_epoch_id: EpochId) -> Result<usize, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        pg_pool_conn
            .build_transaction()
            .read_write().serializable()
            .run(|conn| {
                for table in &self.tables {
                    let sql = format!("CREATE TABLE {table}_partition_{next_epoch_id} PARTITION OF {table} FOR VALUES FROM ({next_epoch_id}) TO ({});", next_epoch_id+1);
                    diesel::sql_query(sql).execute(conn)?;
                }
                Ok::<_, diesel::result::Error>(self.tables.len())
            })
            .map_err(|e| IndexerError::PostgresReadError(e.to_string()))
    }

    fn get_table_partitions(&self) -> Result<BTreeMap<String, String>, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;

        #[derive(QueryableByName, Debug, Clone)]
        struct PartitionedTable {
            #[diesel(sql_type = VarChar)]
            table_name: String,
            #[diesel(sql_type = VarChar)]
            last_partition: String,
        }

        Ok(pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| diesel::sql_query(GET_PARTITION_SQL).load(conn))
            .map_err(|e| IndexerError::PostgresReadError(e.to_string()))?
            .into_iter()
            .map(|table: PartitionedTable| (table.table_name, table.last_partition))
            .collect())
    }
}
