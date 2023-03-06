// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::models::checkpoints::Checkpoint;
use crate::models::error_logs::commit_error_logs;
use crate::models::transactions::Transaction;
use crate::schema::addresses::account_address;
use crate::schema::checkpoints::dsl::checkpoints as checkpoints_table;
use crate::schema::checkpoints::{checkpoint_digest, sequence_number};
use crate::schema::move_calls::dsl as move_calls_dsl;
use crate::schema::packages::{author, module_names, package_content, package_id};
use crate::schema::transactions::{dsl, transaction_digest};
use crate::schema::{
    addresses, events, move_calls, objects, owner_changes, packages, transactions,
};
use crate::store::indexer_store::TemporaryCheckpointStore;
use crate::store::{IndexerStore, TemporaryEpochStore};
use crate::{get_pg_pool_connection, PgConnectionPool};
use async_trait::async_trait;
use diesel::dsl::{count, max};
use diesel::sql_types::VarChar;
use diesel::upsert::excluded;
use diesel::QueryableByName;
use diesel::{ExpressionMethods, PgArrayExpressionMethods};
use diesel::{QueryDsl, RunQueryDsl};
use std::collections::BTreeMap;
use sui_json_rpc_types::CheckpointId;
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
    cp: PgConnectionPool,
    partition_manager: PartitionManager,
}

impl PgIndexerStore {
    pub fn new(cp: PgConnectionPool) -> Self {
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

    fn get_checkpoint(&self, id: CheckpointId) -> Result<Checkpoint, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| match id {
                CheckpointId::SequenceNumber(seq) => checkpoints_table
                    .filter(sequence_number.eq(seq as i64))
                    .limit(1)
                    .first::<Checkpoint>(conn),
                CheckpointId::Digest(digest) => checkpoints_table
                    .filter(checkpoint_digest.eq(digest.base58_encode()))
                    .limit(1)
                    .first::<Checkpoint>(conn),
            })
            .map_err(|e| {
                IndexerError::PostgresReadError(format!(
                    "Failed reading previous checkpoint in PostgresDB with error {:?}",
                    e
                ))
            })
    }

    fn get_total_transaction_number(&self) -> Result<i64, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| dsl::transactions.select(count(dsl::id)).first::<i64>(conn))
            .map_err(|e| {
                IndexerError::PostgresReadError(format!(
                    "Failed reading total transaction number with err: {:?}",
                    e
                ))
            })
    }

    // NOTE: PG table serial number does not always increment by 1
    // based on observations, thus `get_total_transaction_number` and
    // `get_latest_transaction_sequence_number` are not always equal.
    fn get_latest_transaction_sequence_number(&self) -> Result<i64, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| {
                dsl::transactions
                    .select(max(dsl::id))
                    .first::<Option<i64>>(conn)
                    // postgres serial starts from 1
                    .map(|seq_num_opt| seq_num_opt.unwrap_or(0))
            })
            .map_err(|e| {
                IndexerError::PostgresReadError(format!(
                    "Failed reading latest transaction sequence number with err: {:?}",
                    e
                ))
            })
    }

    fn get_latest_move_call_sequence_number(&self) -> Result<i64, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| {
                move_calls::table
                    .select(max(move_calls::id))
                    .first::<Option<i64>>(conn)
                    // postgres serial starts from 1
                    .map(|seq_num_opt| seq_num_opt.unwrap_or(0))
            })
            .map_err(|e| {
                IndexerError::PostgresReadError(format!(
                    "Failed reading latest move call sequence number with err: {:?}",
                    e
                ))
            })
    }

    fn get_transaction_by_digest(&self, txn_digest: String) -> Result<Transaction, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| {
                dsl::transactions
                    .filter(transaction_digest.eq(txn_digest.clone()))
                    .first::<Transaction>(conn)
            })
            .map_err(|e| {
                IndexerError::PostgresReadError(format!(
                    "Failed reading transaction with digest {} and err: {:?}",
                    txn_digest, e
                ))
            })
    }

    fn get_transaction_sequence_by_digest(
        &self,
        txn_digest: Option<String>,
        is_descending: bool,
    ) -> Result<i64, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        let Some(txn_digest) = txn_digest else {
            if is_descending {
                return self.get_latest_transaction_sequence_number();
            } else {
                // NOTE: Postgres serial starts from 1
                return Ok(1);
            }
        };
        pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| {
                dsl::transactions
                    .filter(transaction_digest.eq(txn_digest.clone()))
                    .select(dsl::id)
                    .first::<i64>(conn)
            })
            .map_err(|e| {
                IndexerError::PostgresReadError(format!(
                    "Failed reading transaction sequence with digest {} and err: {:?}",
                    txn_digest, e
                ))
            })
    }

    fn get_move_call_sequence_by_digest(
        &self,
        txn_digest: Option<String>,
        reverse: bool,
    ) -> Result<i64, IndexerError> {
        let Some(txn_digest) = txn_digest else {
            if reverse {
                return self.get_latest_move_call_sequence_number();
            } else {
                // NOTE: Postgres serial starts from 1
                return Ok(1);
            }
        };

        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| {
                let mut boxed_query = move_calls_dsl::move_calls
                    .filter(move_calls_dsl::transaction_digest.eq(txn_digest.clone()))
                    .into_boxed();
                if reverse {
                    boxed_query = boxed_query.order(move_calls_dsl::id.desc());
                } else {
                    boxed_query = boxed_query.order(move_calls_dsl::id.asc());
                }
                boxed_query.select(move_calls_dsl::id).first::<i64>(conn)
            })
            .map_err(|e| {
                IndexerError::PostgresReadError(format!(
                    "Failed reading move call sequence with digest {} and err: {:?}",
                    txn_digest, e
                ))
            })
    }

    fn get_all_transaction_digest_page(
        &self,
        start_sequence: i64,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<String>, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| {
                if is_descending {
                    dsl::transactions
                        .filter(dsl::id.le(start_sequence))
                        .order(dsl::id.desc())
                        .limit((limit + 1) as i64)
                        .select(transaction_digest)
                        .load::<String>(conn)
                } else {
                    dsl::transactions
                        .filter(dsl::id.ge(start_sequence))
                        .order(dsl::id.asc())
                        .limit((limit + 1) as i64)
                        .select(transaction_digest)
                        .load::<String>(conn)
                }
            }).map_err(|e| {
            IndexerError::PostgresReadError(format!(
                "Failed reading all transaction digests with start_sequence {} and limit {} and err: {:?}",
                start_sequence, limit, e
            ))
        })
    }

    fn get_transaction_digest_page_by_move_call(
        &self,
        package_name: String,
        module_name: Option<String>,
        function_name: Option<String>,
        start_sequence: i64,
        limit: usize,
        reverse: bool,
    ) -> Result<Vec<String>, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| {
                let mut builder = move_calls_dsl::move_calls.filter(move_calls_dsl::move_package.eq(package_name.clone())).into_boxed();
                if let Some(module_name) = module_name.clone() {
                    builder = builder.filter(move_calls_dsl::move_module.eq(module_name));
                }
                if let Some(function_name) = function_name.clone() {
                    builder = builder.filter(move_calls_dsl::move_function.eq(function_name));
                }
                if reverse {
                    builder.filter(move_calls_dsl::id.le(start_sequence))
                        .order(move_calls_dsl::id.desc())
                        // id is needed in the select clause to make distinct work
                        .select((move_calls_dsl::transaction_digest, move_calls_dsl::id))
                        .distinct()
                        .limit(limit as i64)
                        .load::<(String, i64)>(conn)
                        .map(|v| v.into_iter().map(|(digest, _)| digest).collect())
                } else {
                    builder.filter(move_calls_dsl::id.ge(start_sequence))
                        .order(move_calls_dsl::id.asc())
                        .select((move_calls_dsl::transaction_digest, move_calls_dsl::id))
                        .distinct()
                        .limit(limit as i64)
                        .load::<(String, i64)>(conn)
                        .map(|v| v.into_iter().map(|(digest, _)| digest).collect())
                }
            }).map_err(|e| {
            IndexerError::PostgresReadError(format!(
                "Failed reading transaction digests with package_name {} module_name {:?} and function_name {:?} and start_sequence {} and limit {} and err: {:?}",
                package_name, module_name, function_name, start_sequence, limit, e
            ))
        })
    }

    fn get_transaction_digest_page_by_mutated_object(
        &self,
        object_id: String,
        start_sequence: i64,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<String>, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| {
                if is_descending {
                    dsl::transactions
                        .filter(dsl::id.le(start_sequence))
                        .filter(dsl::mutated.contains(vec![Some(object_id.clone())]))
                        .order(dsl::id.desc())
                        .limit(limit as i64)
                        .select(transaction_digest)
                        .load::<String>(conn)
                } else {
                    dsl::transactions
                        .filter(dsl::id.ge(start_sequence))
                        .filter(dsl::mutated.contains(vec![Some(object_id.clone())]))
                        .order(dsl::id.asc())
                        .limit(limit as i64)
                        .select(transaction_digest)
                        .load::<String>(conn)
                }
            }).map_err(|e| {
            IndexerError::PostgresReadError(format!(
                "Failed reading transaction digests by mutated object id {} with start_sequence {} and limit {} and err: {:?}",
                object_id, start_sequence, limit, e
            ))
        })
    }

    fn get_transaction_digest_page_by_sender_address(
        &self,
        sender_address: String,
        start_sequence: i64,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<String>, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| {
                if is_descending {
                    dsl::transactions
                        .filter(dsl::id.le(start_sequence))
                        .filter(dsl::sender.eq(sender_address.clone()))
                        .order(dsl::id.desc())
                        .limit(limit as i64)
                        .select(transaction_digest)
                        .load::<String>(conn)
                } else {
                    dsl::transactions
                        .filter(dsl::id.ge(start_sequence))
                        .filter(dsl::sender.eq(sender_address.clone()))
                        .order(dsl::id.asc())
                        .limit(limit as i64)
                        .select(transaction_digest)
                        .load::<String>(conn)
                }
            }).map_err(|e| {
            IndexerError::PostgresReadError(format!(
                "Failed reading transaction digests by sender address {} with start_sequence {} and limit {} and err: {:?}",
                sender_address, start_sequence, limit, e
            ))
        })
    }

    fn get_transaction_digest_page_by_recipient_address(
        &self,
        recipient_address: String,
        start_sequence: i64,
        limit: usize,
        is_descending: bool,
    ) -> Result<Vec<String>, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| {
                if is_descending {
                    dsl::transactions
                        .filter(dsl::id.le(start_sequence))
                        .filter(dsl::recipients.contains(vec![Some(recipient_address.clone())]))
                        .order(dsl::id.desc())
                        .limit(limit as i64)
                        .select(transaction_digest)
                        .load::<String>(conn)
                } else {
                    dsl::transactions
                        .filter(dsl::id.ge(start_sequence))
                        .filter(dsl::recipients.contains(vec![Some(recipient_address.clone())]))
                        .order(dsl::id.asc())
                        .limit(limit as i64)
                        .select(transaction_digest)
                        .load::<String>(conn)
                }
            }).map_err(|e| {
            IndexerError::PostgresReadError(format!(
                "Failed reading transaction digests by recipient address {} with start_sequence {} and limit {} and err: {:?}",
                recipient_address, start_sequence, limit, e
            ))
        })
    }

    fn read_transactions(
        &self,
        last_processed_id: i64,
        limit: usize,
    ) -> Result<Vec<Transaction>, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| {
                dsl::transactions
                    .filter(dsl::id.gt(last_processed_id))
                    .limit(limit as i64)
                    .load::<Transaction>(conn)
            })
            .map_err(|e| {
                IndexerError::PostgresReadError(format!(
                    "Failed reading transactions with last_processed_id {} and err: {:?}",
                    last_processed_id, e
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
            packages,
            move_calls,
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
                    .execute(conn)?;

                diesel::insert_into(packages::table)
                    .values(packages)
                    .on_conflict(package_id)
                    .do_update()
                    .set((
                        author.eq(excluded(author)),
                        module_names.eq(excluded(module_names)),
                        package_content.eq(excluded(package_content)),
                    ))
                    .execute(conn)?;

                diesel::insert_into(move_calls::table)
                    .values(move_calls)
                    .execute(conn)

            })
            .map_err(|e| {
                IndexerError::PostgresWriteError(format!(
                    "Failed writing checkpoint to PostgresDB with transactions {:?} and error: {:?}",
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
    cp: PgConnectionPool,
    tables: Vec<String>,
}

impl PartitionManager {
    fn new(cp: PgConnectionPool) -> Result<Self, IndexerError> {
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
