// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::sync::Arc;

use anyhow::{Ok, Result};
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    models::cp_sequence_numbers::tx_interval,
    pipeline::{concurrent::Handler, Processor},
};
use sui_indexer_alt_schema::{schema::tx_calls, transactions::StoredTxCalls};
use sui_pg_db as db;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::transaction::TransactionDataAPI;

pub(crate) struct TxCalls;

impl Processor for TxCalls {
    const NAME: &'static str = "tx_calls";

    type Value = StoredTxCalls;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            ..
        } = checkpoint.as_ref();

        let first_tx = checkpoint_summary.network_total_transactions as usize - transactions.len();

        Ok(transactions
            .iter()
            .enumerate()
            .flat_map(|(i, tx)| {
                let tx_sequence_number = (first_tx + i) as i64;
                let sender = tx.transaction.sender_address().to_vec();
                let calls = tx.transaction.data().transaction_data().move_calls();

                calls
                    .iter()
                    .map(|(package, module, function)| StoredTxCalls {
                        tx_sequence_number,
                        package: package.to_vec(),
                        module: module.to_string(),
                        function: function.to_string(),
                        sender: sender.clone(),
                    })
                    .collect::<Vec<_>>()
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl Handler for TxCalls {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(tx_calls::table)
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
        let Range {
            start: from_tx,
            end: to_tx,
        } = tx_interval(conn, from..to_exclusive).await?;
        let filter = tx_calls::table
            .filter(tx_calls::tx_sequence_number.between(from_tx as i64, to_tx as i64 - 1));

        Ok(diesel::delete(filter).execute(conn).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel_async::RunQueryDsl;
    use sui_indexer_alt_framework::{handlers::cp_sequence_numbers::CpSequenceNumbers, Indexer};
    use sui_indexer_alt_schema::MIGRATIONS;
    use sui_types::{
        base_types::ObjectID, test_checkpoint_data_builder::TestCheckpointDataBuilder,
    };

    async fn get_all_tx_calls(conn: &mut db::Connection<'_>) -> Result<Vec<StoredTxCalls>> {
        Ok(tx_calls::table
            .order_by((
                tx_calls::tx_sequence_number,
                tx_calls::sender,
                tx_calls::package,
                tx_calls::module,
                tx_calls::function,
            ))
            .load(conn)
            .await?)
    }

    #[tokio::test]
    async fn test_tx_calls_pruning_complains_if_no_mapping() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();

        let result = TxCalls.prune(0, 2, &mut conn).await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "No checkpoint mapping found for checkpoint 0"
        );
    }

    /// The kv_checkpoints pruner does not require cp_sequence_numbers, it can prune directly with the
    /// checkpoint sequence number range.
    #[tokio::test]
    async fn test_tx_calls_pruning() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();

        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .add_move_call(ObjectID::random(), "module", "function")
            .finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = TxCalls.process(&checkpoint).unwrap();
        TxCalls::commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .add_move_call(ObjectID::random(), "module", "function")
            .add_move_call(ObjectID::random(), "module", "function")
            .finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = TxCalls.process(&checkpoint).unwrap();
        TxCalls::commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        let reuse_package_id = ObjectID::random();
        builder = builder
            .start_transaction(0)
            .add_move_call(reuse_package_id, "donut", "prune")
            .add_move_call(reuse_package_id, "donut", "prune2")
            .add_move_call(reuse_package_id, "donut", "prune3")
            .add_move_call(reuse_package_id, "donut", "prune4")
            .finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = TxCalls.process(&checkpoint).unwrap();
        TxCalls::commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        let fetched_results = get_all_tx_calls(&mut conn).await.unwrap();
        assert_eq!(fetched_results.len(), 7);

        // Prune checkpoints from `[0, 2)`, expect 4 tx_calls remaining
        let rows_pruned = TxCalls.prune(0, 2, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 3);
        let remaining_tx_calls = get_all_tx_calls(&mut conn).await.unwrap();
        assert_eq!(remaining_tx_calls.len(), 4);
        assert_eq!(
            remaining_tx_calls
                .iter()
                .map(|tx_call| (tx_call.module.clone(), tx_call.function.clone()))
                .collect::<Vec<_>>(),
            vec![
                ("donut".to_string(), "prune".to_string()),
                ("donut".to_string(), "prune2".to_string()),
                ("donut".to_string(), "prune3".to_string()),
                ("donut".to_string(), "prune4".to_string())
            ]
            .into_iter()
            .collect::<Vec<_>>()
        );
    }
}
