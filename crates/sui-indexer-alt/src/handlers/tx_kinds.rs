// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::sync::Arc;

use anyhow::Result;
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    pipeline::Processor,
    postgres::{Connection, handler::Handler},
    types::full_checkpoint_content::Checkpoint,
};
use sui_indexer_alt_schema::{
    schema::tx_kinds,
    transactions::{StoredKind, StoredTxKind},
};
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::cp_sequence_numbers::tx_interval;
use async_trait::async_trait;

pub(crate) struct TxKinds;

#[async_trait]
impl Processor for TxKinds {
    const NAME: &'static str = "tx_kinds";

    type Value = StoredTxKind;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let Checkpoint {
            transactions,
            summary,
            ..
        } = checkpoint.as_ref();

        let mut values = Vec::new();
        let first_tx = summary.network_total_transactions as usize - transactions.len();

        for (i, tx) in transactions.iter().enumerate() {
            let tx_sequence_number = (first_tx + i) as i64;
            let tx_kind = if tx.transaction.is_system_tx() {
                StoredKind::SystemTransaction
            } else {
                StoredKind::ProgrammableTransaction
            };

            values.push(StoredTxKind {
                tx_sequence_number,
                tx_kind,
            });
        }

        Ok(values)
    }
}

#[async_trait]
impl Handler for TxKinds {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        Ok(diesel::insert_into(tx_kinds::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }

    async fn prune<'a>(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut Connection<'a>,
    ) -> Result<usize> {
        let Range {
            start: from_tx,
            end: to_tx,
        } = tx_interval(conn, from..to_exclusive).await?;
        let filter = tx_kinds::table
            .filter(tx_kinds::tx_sequence_number.between(from_tx as i64, to_tx as i64 - 1));

        Ok(diesel::delete(filter).execute(conn).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel_async::RunQueryDsl;
    use sui_indexer_alt_framework::{
        Indexer, types::test_checkpoint_data_builder::TestCheckpointBuilder,
    };
    use sui_indexer_alt_schema::MIGRATIONS;

    use crate::handlers::cp_sequence_numbers::CpSequenceNumbers;

    async fn get_all_tx_kinds(conn: &mut Connection<'_>) -> Result<Vec<i64>> {
        Ok(tx_kinds::table
            .select(tx_kinds::tx_sequence_number)
            .load(conn)
            .await?)
    }

    #[tokio::test]
    async fn test_tx_kinds_pruning_complains_if_no_mapping() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        let result = TxKinds.prune(0, 2, &mut conn).await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "No checkpoint mapping found for checkpoint 0"
        );
    }

    /// The kv_checkpoints pruner does not require cp_sequence_numbers, it can prune directly with
    /// the checkpoint sequence number range.
    #[tokio::test]
    async fn test_tx_kinds_pruning() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        let mut builder = TestCheckpointBuilder::new(0);
        builder = builder.start_transaction(0).finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = TxKinds.process(&checkpoint).await.unwrap();
        TxKinds::commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).await.unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        builder = builder.start_transaction(0).finish_transaction();
        builder = builder.start_transaction(1).finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = TxKinds.process(&checkpoint).await.unwrap();
        TxKinds::commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).await.unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        builder = builder.start_transaction(0).finish_transaction();
        builder = builder.start_transaction(1).finish_transaction();
        builder = builder.start_transaction(2).finish_transaction();
        builder = builder.start_transaction(3).finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = TxKinds.process(&checkpoint).await.unwrap();
        TxKinds::commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).await.unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        let fetched_results = get_all_tx_kinds(&mut conn).await.unwrap();
        assert_eq!(fetched_results.len(), 7);

        // Prune checkpoints from `[0, 2)`, expect 4 tx_kinds remaining
        let rows_pruned = TxKinds.prune(0, 2, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 3);
        let remaining_tx_kinds = get_all_tx_kinds(&mut conn).await.unwrap();
        assert_eq!(remaining_tx_kinds.len(), 4);
    }
}
