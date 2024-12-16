// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::{Context, Result};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::pipeline::{concurrent::Handler, Processor};
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
