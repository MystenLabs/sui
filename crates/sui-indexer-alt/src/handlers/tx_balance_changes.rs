// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::{collections::BTreeMap, sync::Arc};

use anyhow::{Context, Result};
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    models::cp_sequence_numbers::tx_interval,
    pipeline::{concurrent::Handler, Processor},
};
use sui_indexer_alt_schema::{
    schema::tx_balance_changes,
    transactions::{BalanceChange, StoredTxBalanceChange},
};
use sui_pg_db as db;
use sui_types::{
    coin::Coin,
    effects::TransactionEffectsAPI,
    full_checkpoint_content::{CheckpointData, CheckpointTransaction},
    gas_coin::GAS,
};

pub(crate) struct TxBalanceChanges;

impl Processor for TxBalanceChanges {
    const NAME: &'static str = "tx_balance_changes";

    type Value = StoredTxBalanceChange;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            ..
        } = checkpoint.as_ref();

        let mut values = Vec::new();
        let first_tx = checkpoint_summary.network_total_transactions as usize - transactions.len();

        for (i, tx) in transactions.iter().enumerate() {
            let tx_sequence_number = (first_tx + i) as i64;
            let balance_changes = balance_changes(tx).with_context(|| {
                format!("Calculating balance changes for transaction {tx_sequence_number}")
            })?;

            values.push(StoredTxBalanceChange {
                tx_sequence_number,
                balance_changes: bcs::to_bytes(&balance_changes).with_context(|| {
                    format!("Serializing balance changes for transaction {tx_sequence_number}")
                })?,
            });
        }

        Ok(values)
    }
}

#[async_trait::async_trait]
impl Handler for TxBalanceChanges {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(tx_balance_changes::table)
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
        let filter = tx_balance_changes::table.filter(
            tx_balance_changes::tx_sequence_number.between(from_tx as i64, to_tx as i64 - 1),
        );

        Ok(diesel::delete(filter).execute(conn).await?)
    }
}

/// Calculate balance changes based on the object's input and output objects.
fn balance_changes(transaction: &CheckpointTransaction) -> Result<Vec<BalanceChange>> {
    // Shortcut if the transaction failed -- we know that only gas was charged.
    if transaction.effects.status().is_err() {
        return Ok(vec![BalanceChange::V1 {
            owner: transaction.effects.gas_object().1,
            coin_type: GAS::type_tag().to_canonical_string(/* with_prefix */ true),
            amount: -(transaction.effects.gas_cost_summary().net_gas_usage() as i128),
        }]);
    }

    let mut changes = BTreeMap::new();
    for object in &transaction.input_objects {
        if let Some((type_, balance)) = Coin::extract_balance_if_coin(object)? {
            *changes.entry((object.owner(), type_)).or_insert(0i128) -= balance as i128;
        }
    }

    for object in &transaction.output_objects {
        if let Some((type_, balance)) = Coin::extract_balance_if_coin(object)? {
            *changes.entry((object.owner(), type_)).or_insert(0i128) += balance as i128;
        }
    }

    Ok(changes
        .into_iter()
        .map(|((owner, coin_type), amount)| BalanceChange::V1 {
            owner: owner.clone(),
            coin_type: coin_type.to_canonical_string(/* with_prefix */ true),
            amount,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel_async::RunQueryDsl;
    use sui_indexer_alt_framework::{handlers::cp_sequence_numbers::CpSequenceNumbers, Indexer};
    use sui_indexer_alt_schema::MIGRATIONS;
    use sui_types::test_checkpoint_data_builder::TestCheckpointDataBuilder;

    async fn get_all_tx_balance_changes(conn: &mut db::Connection<'_>) -> Result<Vec<i64>> {
        Ok(tx_balance_changes::table
            .select(tx_balance_changes::tx_sequence_number)
            .order_by(tx_balance_changes::tx_sequence_number)
            .load(conn)
            .await?)
    }

    #[tokio::test]
    async fn test_tx_balance_changes_pruning_complains_if_no_mapping() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();

        let result = TxBalanceChanges.prune(0, 2, &mut conn).await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "No checkpoint mapping found for checkpoint 0"
        );
    }

    /// The kv_checkpoints pruner does not require cp_sequence_numbers, it can prune directly with the
    /// checkpoint sequence number range.
    #[tokio::test]
    async fn test_tx_balance_changes_pruning() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();

        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder.start_transaction(0).finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = TxBalanceChanges.process(&checkpoint).unwrap();
        TxBalanceChanges::commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        builder = builder.start_transaction(0).finish_transaction();
        builder = builder.start_transaction(1).finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = TxBalanceChanges.process(&checkpoint).unwrap();
        TxBalanceChanges::commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        builder = builder.start_transaction(0).finish_transaction();
        builder = builder.start_transaction(1).finish_transaction();
        builder = builder.start_transaction(2).finish_transaction();
        builder = builder.start_transaction(3).finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = TxBalanceChanges.process(&checkpoint).unwrap();
        TxBalanceChanges::commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        let fetched_results = get_all_tx_balance_changes(&mut conn).await.unwrap();
        assert_eq!(fetched_results.len(), 7);

        // Prune checkpoints from `[0, 2)`, expect 4 tx_balance_changes remaining
        let rows_pruned = TxBalanceChanges.prune(0, 2, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 3);
        let remaining_tx_balance_changes = get_all_tx_balance_changes(&mut conn).await.unwrap();
        assert_eq!(remaining_tx_balance_changes.len(), 4);
        assert_eq!(remaining_tx_balance_changes, vec![3, 4, 5, 6]);
    }
}
