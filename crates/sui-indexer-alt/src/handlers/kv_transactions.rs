// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Context, Result};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    db,
    pipeline::{concurrent::Handler, Processor},
};
use sui_types::full_checkpoint_content::CheckpointData;

use crate::{models::transactions::StoredTransaction, schema::kv_transactions};

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
            let transaction = &tx.transaction.data().intent_message().value;

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
}
