// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use sui_types::full_checkpoint_content::CheckpointData;

/// Trait for processing transactions in parallel across all transactions in a checkpoint.
/// Implementations will extract and transform transaction data into structured rows for analytics.
#[async_trait]
pub trait TxProcessor<Row>: Send + Sync + 'static {
    /// Process a single transaction at the given index and return the rows to be stored.
    /// The implementation should handle extracting the transaction from the checkpoint.
    async fn process_transaction(
        &self,
        tx_idx: usize,
        checkpoint: &CheckpointData,
    ) -> Result<Vec<Row>>;
}

/// Run transaction processing in parallel across all transactions in a checkpoint.
pub async fn run_parallel<Row, P>(
    checkpoint: Arc<CheckpointData>,
    processor: Arc<P>,
) -> Result<Vec<Row>>
where
    Row: Send + 'static,
    P: TxProcessor<Row>,
{
    // Process transactions in parallel using buffered stream for ordered execution
    let txn_len = checkpoint.transactions.len();
    let mut entries = Vec::new();

    let mut stream = stream::iter(0..txn_len)
        .map(|idx| {
            let checkpoint = checkpoint.clone();
            let processor = processor.clone();
            tokio::spawn(async move { processor.process_transaction(idx, &checkpoint).await })
        })
        .buffered(num_cpus::get() * 4);

    while let Some(join_res) = stream.next().await {
        match join_res {
            Ok(Ok(tx_entries)) => {
                entries.extend(tx_entries);
            }
            Ok(Err(e)) => {
                // Task executed but application logic returned an error
                return Err(e);
            }
            Err(e) => {
                // Task panicked or was cancelled
                return Err(anyhow::anyhow!("Task join error: {}", e));
            }
        }
    }
    Ok(entries)
}
