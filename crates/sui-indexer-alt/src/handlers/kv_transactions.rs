// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Context, Result};
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::pipeline::{concurrent::Handler, Processor};
use sui_indexer_alt_schema::{schema::kv_transactions, transactions::StoredTransaction};
use sui_pg_db as db;
use sui_types::full_checkpoint_content::CheckpointData;

pub(crate) struct KvTransactions;

impl Processor for KvTransactions {
    const NAME: &'static str = "kv_transactions";

    type Value = StoredTransaction;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            ..
        } = checkpoint.as_ref();

        let cp_sequence_number = checkpoint_summary.sequence_number as i64;

        let mut values = Vec::with_capacity(transactions.len());
        for (i, tx) in transactions.iter().enumerate() {
            let tx_digest = tx.transaction.digest();
            let transaction = &tx.transaction.data().transaction_data();
            let signatures = &tx.transaction.data().tx_signatures();

            let effects = &tx.effects;
            let events: Vec<_> = tx.events.iter().flat_map(|e| e.data.iter()).collect();

            values.push(StoredTransaction {
                tx_digest: tx_digest.inner().into(),
                cp_sequence_number,
                timestamp_ms: checkpoint_summary.timestamp_ms as i64,
                raw_transaction: bcs::to_bytes(transaction).with_context(|| {
                    format!("Serializing transaction {tx_digest} (cp {cp_sequence_number}, tx {i})")
                })?,
                raw_effects: bcs::to_bytes(effects).with_context(|| {
                    format!("Serializing effects for transaction {tx_digest} (cp {cp_sequence_number}, tx {i})")
                })?,
                events: bcs::to_bytes(&events).with_context(|| {
                    format!("Serializing events for transaction {tx_digest} (cp {cp_sequence_number}, tx {i})")
                })?,
                user_signatures: bcs::to_bytes(signatures).with_context(|| {
                    format!("Serializing signatures for transaction {tx_digest} (cp {cp_sequence_number}, tx {i})")
                })?,
            });
        }

        Ok(values)
    }
}

#[async_trait::async_trait]
impl Handler for KvTransactions {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(kv_transactions::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }

    async fn prune(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut db::Connection<'_>,
    ) -> Result<usize> {
        let filter = kv_transactions::table.filter(
            kv_transactions::cp_sequence_number.between(from as i64, to_exclusive as i64 - 1),
        );

        Ok(diesel::delete(filter).execute(conn).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel_async::RunQueryDsl;
    use sui_indexer_alt_framework::Indexer;
    use sui_indexer_alt_schema::MIGRATIONS;
    use sui_types::test_checkpoint_data_builder::TestCheckpointDataBuilder;

    async fn get_all_kv_transactions(
        conn: &mut db::Connection<'_>,
    ) -> Result<Vec<StoredTransaction>> {
        Ok(kv_transactions::table.load(conn).await?)
    }

    /// The kv_checkpoints pruner does not require cp_sequence_numbers, it can prune directly with the
    /// checkpoint sequence number range.
    #[tokio::test]
    async fn test_kv_transactions_pruning() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();

        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder.start_transaction(0).finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = KvTransactions.process(&checkpoint).unwrap();
        KvTransactions::commit(&values, &mut conn).await.unwrap();

        builder = builder.start_transaction(0).finish_transaction();
        builder = builder.start_transaction(1).finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = KvTransactions.process(&checkpoint).unwrap();
        KvTransactions::commit(&values, &mut conn).await.unwrap();

        builder = builder.start_transaction(0).finish_transaction();
        builder = builder.start_transaction(1).finish_transaction();
        builder = builder.start_transaction(2).finish_transaction();
        builder = builder.start_transaction(3).finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = KvTransactions.process(&checkpoint).unwrap();
        KvTransactions::commit(&values, &mut conn).await.unwrap();

        let transactions = get_all_kv_transactions(&mut conn).await.unwrap();
        assert_eq!(transactions.len(), 7);

        // Prune checkpoints from `[0, 2)`
        let rows_pruned = KvTransactions.prune(0, 2, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 3);

        let remaining_transactions = get_all_kv_transactions(&mut conn).await.unwrap();
        assert_eq!(remaining_transactions.len(), 4);
    }
}
