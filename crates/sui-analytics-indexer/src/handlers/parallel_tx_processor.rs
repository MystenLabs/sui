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
