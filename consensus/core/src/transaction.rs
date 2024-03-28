// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use mysten_metrics::metered_channel;
use mysten_metrics::metered_channel::channel_with_total;
use sui_protocol_config::ProtocolConfig;
use tap::tap::TapFallible;
use thiserror::Error;
use tokio::sync::oneshot;
use tracing::error;

use crate::block::Transaction;
use crate::context::Context;

/// The maximum number of transactions pending to the queue to be pulled for block proposal
const MAX_PENDING_TRANSACTIONS: usize = 2_000;

const MAX_CONSUMED_TRANSACTIONS_PER_REQUEST: u64 = 5_000;

/// The guard acts as an acknowledgment mechanism for the inclusion of the transaction to a block.
/// When the transaction is included to a block then the inclusion should be explicitly acknowledged
/// by calling the `acknowledge` method. If the guard is dropped without getting acknowledged then
/// that means the transaction has not been included to a block and the consensus is shutting down.
pub(crate) struct TransactionGuard {
    pub transaction: Transaction,
    included_in_block_ack: oneshot::Sender<()>,
}

impl TransactionGuard {
    pub fn acknowledge(self) {
        self.included_in_block_ack.send(()).ok();
    }
}

/// The TransactionConsumer is responsible for fetching the next transactions to be included for the block proposals.
/// The transactions are submitted to a channel which is shared between the TransactionConsumer and the TransactionClient
/// and are pulled every time the `next` method is called.
pub(crate) struct TransactionConsumer {
    tx_receiver: metered_channel::Receiver<TransactionGuard>,
    max_consumed_bytes_per_request: u64,
    max_consumed_transactions_per_request: u64,
    pending_transaction: Option<TransactionGuard>,
}

impl TransactionConsumer {
    pub(crate) fn new(
        tx_receiver: metered_channel::Receiver<TransactionGuard>,
        context: Arc<Context>,
        max_consumed_transactions_per_request: Option<u64>,
    ) -> Self {
        Self {
            tx_receiver,
            max_consumed_bytes_per_request: context
                .protocol_config
                .consensus_max_transactions_in_block_bytes(),
            max_consumed_transactions_per_request: max_consumed_transactions_per_request
                .unwrap_or(MAX_CONSUMED_TRANSACTIONS_PER_REQUEST),
            pending_transaction: None,
        }
    }

    // Attempts to fetch the next transactions that have been submitted for sequence. Also a `max_consumed_bytes_per_request` parameter
    // is given in order to ensure up to `max_consumed_bytes_per_request` bytes of transactions are retrieved.
    pub(crate) fn next(&mut self) -> Vec<TransactionGuard> {
        let mut transactions = Vec::new();
        let mut total_size: usize = 0;

        if let Some(t) = self.pending_transaction.take() {
            // Here we assume that a transaction can always fit in `max_fetched_bytes_per_request`
            total_size += t.transaction.data().len();
            transactions.push(t);
        }

        while let Ok(t) = self.tx_receiver.try_recv() {
            total_size += t.transaction.data().len();

            // If we went over the max size with this transaction, just cache it for the next pull.
            if total_size as u64 > self.max_consumed_bytes_per_request {
                self.pending_transaction = Some(t);
                break;
            }

            transactions.push(t);

            if transactions.len() as u64 >= self.max_consumed_transactions_per_request {
                break;
            }
        }
        transactions
    }
}

#[derive(Clone)]
pub struct TransactionClient {
    sender: metered_channel::Sender<TransactionGuard>,
    max_transaction_size: u64,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Failed to submit transaction, consensus is shutting down: {0}")]
    ConsensusShuttingDown(String),

    #[error("Transaction size ({0}B) is over limit ({1}B)")]
    OversizedTransaction(u64, u64),
}

impl TransactionClient {
    pub(crate) fn new(
        context: Arc<Context>,
    ) -> (Self, metered_channel::Receiver<TransactionGuard>) {
        let (sender, receiver) = channel_with_total(
            MAX_PENDING_TRANSACTIONS,
            &context.metrics.channel_metrics.tx_transactions_submit,
            &context.metrics.channel_metrics.tx_transactions_submit_total,
        );

        (
            Self {
                sender,
                max_transaction_size: context
                    .protocol_config
                    .consensus_max_transaction_size_bytes(),
            },
            receiver,
        )
    }

    /// Submits a transaction to be sequenced. The method returns when the transaction has been successfully
    /// included to the next proposed block.
    pub async fn submit(&self, transaction: Vec<u8>) -> Result<(), ClientError> {
        let included_in_block = self.submit_no_wait(transaction).await?;
        included_in_block
            .await
            .tap_err(|e| error!("Transaction acknowledge failed with {:?}", e))
            .map_err(|e| ClientError::ConsensusShuttingDown(e.to_string()))
    }

    /// Submits a transaction to be sequenced. The transaction length gets evaluated and rejected from consensus if too big.
    /// That shouldn't be the common case as sizes should be aligned between consensus and client. The method returns
    /// a receiver to wait on until the transactions has been included in the next block to get proposed. The consumer should
    /// wait on it to consider as inclusion acknowledgement. If the receiver errors then consensus is shutting down and transaction
    /// has not been included to any block.
    pub(crate) async fn submit_no_wait(
        &self,
        transaction: Vec<u8>,
    ) -> Result<oneshot::Receiver<()>, ClientError> {
        let (included_in_block_ack_send, included_in_block_ack_receive) = oneshot::channel();
        if transaction.len() as u64 > self.max_transaction_size {
            return Err(ClientError::OversizedTransaction(
                transaction.len() as u64,
                self.max_transaction_size,
            ));
        }

        let t = TransactionGuard {
            transaction: Transaction::new(transaction),
            included_in_block_ack: included_in_block_ack_send,
        };
        self.sender
            .send(t)
            .await
            .tap_err(|e| error!("Submit transaction failed with {:?}", e))
            .map_err(|e| ClientError::ConsensusShuttingDown(e.to_string()))?;
        Ok(included_in_block_ack_receive)
    }
}

/// `TransactionVerifier` implementation is supplied by Sui to validate transactions in a block,
/// before acceptance of the block.
pub trait TransactionVerifier: Send + Sync + 'static {
    /// Determines if this batch can be voted on
    fn verify_batch(
        &self,
        protocol_config: &ProtocolConfig,
        batch: &[&[u8]],
    ) -> Result<(), ValidationError>;
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Invalid transaction: {0}")]
    InvalidTransaction(String),
}

/// `NoopTransactionVerifier` accepts all transactions.
pub(crate) struct NoopTransactionVerifier;

impl TransactionVerifier for NoopTransactionVerifier {
    fn verify_batch(
        &self,
        _protocol_config: &ProtocolConfig,
        _batch: &[&[u8]],
    ) -> Result<(), ValidationError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::context::Context;
    use crate::transaction::{TransactionClient, TransactionConsumer, TransactionGuard};
    use futures::stream::FuturesUnordered;
    use futures::StreamExt;
    use std::sync::Arc;
    use std::time::Duration;
    use sui_protocol_config::ProtocolConfig;
    use tokio::time::timeout;

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn basic_submit_and_consume() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_consensus_max_transaction_size_bytes(2_000); // 2KB
            config.set_consensus_max_transactions_in_block_bytes(2_000);
            config
        });

        let context = Arc::new(Context::new_for_test(4).0);
        let (client, tx_receiver) = TransactionClient::new(context.clone());
        let mut consumer = TransactionConsumer::new(tx_receiver, context.clone(), None);

        // submit asynchronously the transactions and keep the waiters
        let mut included_in_block_waiters = FuturesUnordered::new();
        for i in 0..3 {
            let transaction =
                bcs::to_bytes(&format!("transaction {i}")).expect("Serialization should not fail.");
            let w = client
                .submit_no_wait(transaction)
                .await
                .expect("Shouldn't submit successfully transaction");
            included_in_block_waiters.push(w);
        }

        // now pull the transactions from the consumer
        let transactions = consumer.next();
        assert_eq!(transactions.len(), 3);

        for (i, t) in transactions.iter().enumerate() {
            let t: String = bcs::from_bytes(t.transaction.data()).unwrap();
            assert_eq!(format!("transaction {i}").to_string(), t);
        }

        assert!(
            timeout(Duration::from_secs(1), included_in_block_waiters.next())
                .await
                .is_err(),
            "We should expect to timeout as none of the transactions have been acknowledged yet"
        );

        // Now acknowledge the inclusion of transactions
        transactions
            .into_iter()
            .for_each(TransactionGuard::acknowledge);

        // Now make sure that all the waiters have returned
        while let Some(result) = included_in_block_waiters.next().await {
            assert!(result.is_ok());
        }

        // try to pull again transactions, result should be empty
        assert!(consumer.next().is_empty());
    }

    #[tokio::test]
    async fn submit_over_max_fetch_size_and_consume() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_consensus_max_transaction_size_bytes(100);
            config.set_consensus_max_transactions_in_block_bytes(100);
            config
        });

        let context = Arc::new(Context::new_for_test(4).0);
        let (client, tx_receiver) = TransactionClient::new(context.clone());
        let mut consumer = TransactionConsumer::new(tx_receiver, context.clone(), None);

        // submit some transactions
        for i in 0..10 {
            let transaction =
                bcs::to_bytes(&format!("transaction {i}")).expect("Serialization should not fail.");
            let _w = client
                .submit_no_wait(transaction)
                .await
                .expect("Shouldn't submit successfully transaction");
        }

        // now pull the transactions from the consumer
        let mut all_transactions = Vec::new();
        let transactions = consumer.next();
        assert_eq!(transactions.len(), 7);

        // ensure their total size is less than `max_bytes_to_fetch`
        let total_size: u64 = transactions
            .iter()
            .map(|t| t.transaction.data().len() as u64)
            .sum();
        assert!(
            total_size
                <= context
                    .protocol_config
                    .consensus_max_transactions_in_block_bytes(),
            "Should have fetched transactions up to {}",
            context
                .protocol_config
                .consensus_max_transactions_in_block_bytes()
        );
        all_transactions.extend(transactions);

        // try to pull again transactions, next should be provided
        let transactions = consumer.next();
        assert_eq!(transactions.len(), 3);

        // ensure their total size is less than `max_bytes_to_fetch`
        let total_size: u64 = transactions
            .iter()
            .map(|t| t.transaction.data().len() as u64)
            .sum();
        assert!(
            total_size
                <= context
                    .protocol_config
                    .consensus_max_transactions_in_block_bytes(),
            "Should have fetched transactions up to {}",
            context
                .protocol_config
                .consensus_max_transactions_in_block_bytes()
        );
        all_transactions.extend(transactions);

        // try to pull again transactions, result should be empty
        assert!(consumer.next().is_empty());

        for (i, t) in all_transactions.iter().enumerate() {
            let t: String = bcs::from_bytes(t.transaction.data()).unwrap();
            assert_eq!(format!("transaction {i}").to_string(), t);
        }
    }
}
