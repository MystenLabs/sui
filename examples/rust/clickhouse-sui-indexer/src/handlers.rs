// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use clickhouse::Row;
use serde::Serialize;
use std::sync::Arc;

use sui_types::full_checkpoint_content::CheckpointData;
use sui_indexer_alt_framework::{
    pipeline::{Processor, sequential::Handler},
};
use sui_indexer_alt_framework_store_traits::Store;

use crate::store::ClickHouseStore;

/// Structure representing a transaction record in ClickHouse
/// This matches the transactions table schema we created
#[derive(Row, Serialize, Clone, Debug)]
pub struct Transaction {
    pub checkpoint_sequence_number: u64,
    pub transaction_digest: String,
}

/// Handler that processes checkpoint data and extracts transaction digests
#[derive(Clone, Default)]
pub struct TransactionDigestHandler;

impl Processor for TransactionDigestHandler {
    const NAME: &'static str = "transaction_digest_handler";
    type Value = Transaction;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        println!("-------1");
        let checkpoint_seq = checkpoint.checkpoint_summary.sequence_number;
        
        let mut transactions = Vec::new();
        for txn in &checkpoint.transactions {
            transactions.push(Transaction {
                checkpoint_sequence_number: checkpoint_seq,
                transaction_digest: txn.transaction.digest().to_string(),
            });
        }
        
        Ok(transactions)
    }
}

#[async_trait]
impl Handler for TransactionDigestHandler {
    type Store = ClickHouseStore;
    type Batch = Vec<Transaction>;

    fn batch(batch: &mut Self::Batch, values: Vec<Self::Value>) {
        batch.extend(values);
    }

    async fn commit<'a>(
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> Result<usize> {
        let row_count = batch.len();
        if row_count == 0 {
            return Ok(0);
        }

        // Use ClickHouse inserter for efficient bulk inserts
        let mut inserter = conn.client.inserter("transactions")?;
        for transaction in batch {
            inserter.write(transaction)?;
        }
        inserter.end().await?;

        Ok(row_count)
    }
}