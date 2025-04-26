// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use sui_types::full_checkpoint_content::CheckpointData;
use tracing::error;

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
/// Returns a BTreeMap of transaction indices to rows, preserving order.
pub async fn run_parallel<Row, P>(
    checkpoint: Arc<CheckpointData>,
    processor: Arc<P>,
) -> Result<BTreeMap<usize, Vec<Row>>>
where
    Row: Send + 'static,
    P: TxProcessor<Row>,
{
    let checkpoint_transactions = &checkpoint.transactions;
    let transaction_indices: Vec<usize> = (0..checkpoint_transactions.len()).collect();

    // Run all transaction jobs in parallel with a buffer
    let mut results = BTreeMap::new();

    // Process transactions in parallel
    let buffered_stream = stream::iter(transaction_indices)
        .map(|idx| {
            let processor = processor.clone();
            let checkpoint = checkpoint.clone();

            // Use tokio::spawn to properly parallelize tasks
            tokio::spawn(async move {
                // Return Result directly from processor to propagate errors
                processor
                    .process_transaction(idx, &checkpoint)
                    .await
                    .map(|entries| (idx, entries))
            })
        })
        .buffer_unordered(num_cpus::get() * 4); // Scale with available CPUs

    // Collect results from the stream
    futures::pin_mut!(buffered_stream);
    while let Some(join_result) = buffered_stream.next().await {
        match join_result {
            Ok(process_result) => {
                // Break early and return the first error
                match process_result {
                    Ok((idx, entries)) => {
                        if !entries.is_empty() {
                            results.insert(idx, entries);
                        }
                    }
                    Err(e) => {
                        error!("Transaction processing error: {}", e);
                        return Err(anyhow::anyhow!("Failed to process transaction: {}", e));
                    }
                }
            }
            Err(e) => {
                // Task panicked or was cancelled, return immediately
                error!("Task join error: {}", e);
                return Err(anyhow::anyhow!("Task join error: {}", e));
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use simulacrum::Simulacrum;
    use sui_types::base_types::SuiAddress;

    struct TestProcessor {
        counter: std::sync::Mutex<usize>,
        should_fail: bool,
        fail_on_tx_idx: usize,
    }

    #[async_trait]
    impl TxProcessor<String> for TestProcessor {
        async fn process_transaction(
            &self,
            tx_idx: usize,
            checkpoint: &CheckpointData,
        ) -> Result<Vec<String>> {
            let _transaction = &checkpoint.transactions[tx_idx];
            let mut counter = self.counter.lock().unwrap();
            *counter += 1;

            // Simulate error if configured to fail
            if self.should_fail && tx_idx == self.fail_on_tx_idx {
                return Err(anyhow::anyhow!("Simulated error for tx_idx {}", tx_idx));
            }

            Ok(vec![format!("tx_{}", tx_idx)])
        }
    }

    #[tokio::test]
    async fn test_parallel_processing() -> Result<()> {
        use sui_types::storage::ReadStore;
        let mut sim = Simulacrum::new();

        // Create a few transactions
        let transfer_recipient = SuiAddress::random_for_testing_only();
        let (transaction1, _) = sim.transfer_txn(transfer_recipient);
        let (transaction2, _) = sim.transfer_txn(transfer_recipient);

        // Execute transactions
        let (_effects1, _) = sim.execute_transaction(transaction1.clone()).unwrap();
        let (_effects2, _) = sim.execute_transaction(transaction2.clone()).unwrap();

        // Create a checkpoint
        let checkpoint = sim.create_checkpoint();
        let checkpoint_data = sim.get_checkpoint_data(
            checkpoint.clone(),
            sim.get_checkpoint_contents_by_digest(&checkpoint.content_digest)
                .unwrap(),
        )?;

        // Run the parallel processing
        let processor = Arc::new(TestProcessor {
            counter: std::sync::Mutex::new(0),
            should_fail: false,
            fail_on_tx_idx: 0,
        });

        // Initialize counter
        *processor.counter.lock().unwrap() = 0;

        let checkpoint_arc = Arc::new(checkpoint_data);
        let results = run_parallel(checkpoint_arc.clone(), processor.clone()).await?;

        // Check counter
        let counter = *processor.counter.lock().unwrap();
        assert!(counter > 0, "Counter should be non-zero after processing");

        // Check that we processed the transactions
        assert_eq!(results.len(), 2);
        assert!(results.contains_key(&0));
        assert!(results.contains_key(&1));
        assert_eq!(results[&0][0], "tx_0");
        assert_eq!(results[&1][0], "tx_1");

        Ok(())
    }

    #[tokio::test]
    async fn test_error_propagation() -> Result<()> {
        use sui_types::storage::ReadStore;
        let mut sim = Simulacrum::new();

        // Create a few transactions
        let transfer_recipient = SuiAddress::random_for_testing_only();
        let (transaction1, _) = sim.transfer_txn(transfer_recipient);
        let (transaction2, _) = sim.transfer_txn(transfer_recipient);

        // Execute transactions
        let (_effects1, _) = sim.execute_transaction(transaction1.clone()).unwrap();
        let (_effects2, _) = sim.execute_transaction(transaction2.clone()).unwrap();

        // Create a checkpoint
        let checkpoint = sim.create_checkpoint();
        let checkpoint_data = sim.get_checkpoint_data(
            checkpoint.clone(),
            sim.get_checkpoint_contents_by_digest(&checkpoint.content_digest)
                .unwrap(),
        )?;

        // Run the parallel processing with a processor configured to fail
        let processor = Arc::new(TestProcessor {
            counter: std::sync::Mutex::new(0),
            should_fail: true,
            fail_on_tx_idx: 1, // Fail on the second transaction
        });

        let checkpoint_arc = Arc::new(checkpoint_data);
        let result = run_parallel(checkpoint_arc.clone(), processor.clone()).await;

        // The result should be an error
        assert!(result.is_err(), "Expected error but got success");
        let error = result.unwrap_err();
        assert!(
            error.to_string().contains("Simulated error for tx_idx 1"),
            "Error message should contain our simulated error message"
        );

        Ok(())
    }
}
