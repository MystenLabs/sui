// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Ok, Result};
use diesel_async::RunQueryDsl;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::transaction::TransactionDataAPI;

use crate::{
    db, models::transactions::StoredTxCallsFun, pipeline::concurrent::Handler, pipeline::Processor,
    schema::tx_calls_fun,
};

pub struct TxCallsFun;

impl Processor for TxCallsFun {
    const NAME: &'static str = "tx_calls_fun";

    type Value = StoredTxCallsFun;

    fn process(checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
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
                    .map(|(package, module, func)| StoredTxCallsFun {
                        tx_sequence_number,
                        package: package.to_vec(),
                        module: module.to_string(),
                        func: func.to_string(),
                        sender: sender.clone(),
                    })
                    .collect::<Vec<_>>()
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl Handler for TxCallsFun {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_CHUNK_ROWS: usize = 1000;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(tx_calls_fun::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
