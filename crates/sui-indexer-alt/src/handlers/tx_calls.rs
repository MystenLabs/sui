// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Ok, Result};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    db,
    pipeline::{concurrent::Handler, Processor},
};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::transaction::TransactionDataAPI;

use crate::{models::transactions::StoredTxCalls, schema::tx_calls};

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
}
