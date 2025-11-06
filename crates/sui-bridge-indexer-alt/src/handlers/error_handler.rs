// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::handlers::is_bridge_txn;
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use std::sync::Arc;
use sui_bridge_schema::models::SuiErrorTransactions;
use sui_bridge_schema::schema::sui_error_transactions;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::postgres::Connection;
use sui_indexer_alt_framework::postgres::handler::Handler;
use sui_indexer_alt_framework::types::effects::TransactionEffectsAPI;
use sui_indexer_alt_framework::types::execution_status::ExecutionStatus;
use sui_indexer_alt_framework::types::full_checkpoint_content::Checkpoint;
use sui_indexer_alt_framework::types::transaction::TransactionDataAPI;

pub struct ErrorTransactionHandler;

#[async_trait]
impl Processor for ErrorTransactionHandler {
    const NAME: &'static str = "error_transactions";
    type Value = SuiErrorTransactions;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let timestamp_ms = checkpoint.summary.timestamp_ms as i64;
        let mut results = vec![];

        for tx in &checkpoint.transactions {
            if !is_bridge_txn(tx) {
                continue;
            }
            if let ExecutionStatus::Failure { error, command } = tx.effects.status() {
                results.push(SuiErrorTransactions {
                    txn_digest: tx.transaction.digest().inner().to_vec(),
                    timestamp_ms,
                    failure_status: error.to_string(),
                    cmd_idx: command.map(|idx| idx as i64),
                    sender_address: tx.transaction.sender().to_vec(),
                })
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Handler for ErrorTransactionHandler {
    async fn commit<'a>(
        values: &[Self::Value],
        conn: &mut Connection<'a>,
    ) -> anyhow::Result<usize> {
        Ok(diesel::insert_into(sui_error_transactions::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
