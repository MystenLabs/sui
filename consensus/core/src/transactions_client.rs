// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::block::Transaction;
use crate::context::Context;
use mysten_metrics::metered_channel;
use mysten_metrics::metered_channel::channel_with_total;
use std::sync::Arc;
use tap::tap::TapFallible;
use thiserror::Error;
use tracing::error;

/// Maximum number of transactions to be fetched per request of `next`
const MAX_FETCHED_TRANSACTIONS: usize = 100;
/// The maximum number of transactions pending to the queue to be pulled for block proposal
const MAX_PENDING_TRANSACTIONS: usize = 2_000;

/// The TransactionsConsumer is responsible for fetching the next transactions to be included for the block proposals.
/// The transactions are submitted to a channel which is shared between the TransactionsConsumer and the TransactionsClient
/// and are pulled every time the `next` method is called.
#[allow(dead_code)]
pub(crate) struct TransactionsConsumer {
    tx_receiver: metered_channel::Receiver<Transaction>,
    max_fetched_per_request: usize,
}

#[allow(dead_code)]
impl TransactionsConsumer {
    pub(crate) fn new(tx_receiver: metered_channel::Receiver<Transaction>) -> Self {
        Self {
            tx_receiver,
            max_fetched_per_request: MAX_FETCHED_TRANSACTIONS,
        }
    }

    pub(crate) fn with_max_fetched_per_request(mut self, max_fetched_per_request: usize) -> Self {
        self.max_fetched_per_request = max_fetched_per_request;
        self
    }

    // Attempts to fetch the next transactions that have been submitted for sequence.
    pub(crate) fn next(&mut self) -> Vec<Transaction> {
        let mut transactions = Vec::new();
        while let Ok(transaction) = self.tx_receiver.try_recv() {
            transactions.push(transaction);

            if transactions.len() >= self.max_fetched_per_request {
                break;
            }
        }
        transactions
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct TransactionsClient {
    sender: metered_channel::Sender<Transaction>,
}

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Failed to submit transaction to consensus: {0}")]
    SubmitError(String),
}

#[allow(dead_code)]
impl TransactionsClient {
    pub(crate) fn new(context: Arc<Context>) -> (Self, metered_channel::Receiver<Transaction>) {
        let (sender, receiver) = channel_with_total(
            MAX_PENDING_TRANSACTIONS,
            &context.metrics.channel_metrics.tx_transactions_submit,
            &context.metrics.channel_metrics.tx_transactions_submit_total,
        );

        (Self { sender }, receiver)
    }

    // Submits a transaction to be sequenced.
    pub async fn submit(&self, transaction: Vec<u8>) -> Result<(), ClientError> {
        self.sender
            .send(Transaction::new(transaction))
            .await
            .tap_err(|e| error!("Submit transaction failed with {:?}", e))
            .map_err(|e| ClientError::SubmitError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use crate::context::Context;
    use crate::transactions_client::{TransactionsClient, TransactionsConsumer};
    use std::sync::Arc;

    #[tokio::test]
    async fn basic_submit_and_consume() {
        let context = Arc::new(Context::new_for_test(None));
        let (client, tx_receiver) = TransactionsClient::new(context);
        let mut consumer = TransactionsConsumer::new(tx_receiver);

        // submit some transactions
        for i in 0..3 {
            let transaction =
                bcs::to_bytes(&format!("transaction {i}")).expect("Serialization should not fail.");
            client
                .submit(transaction)
                .await
                .expect("Shouldn't submit successfully transaction")
        }

        // now pull the transactions from the consumer
        let transactions = consumer.next();
        assert_eq!(transactions.len(), 3);

        for (i, transaction) in transactions.iter().enumerate() {
            let t: String = bcs::from_bytes(transaction.data()).unwrap();
            assert_eq!(format!("transaction {i}").to_string(), t);
        }

        // try to pull again transactions, result should be empty
        assert!(consumer.next().is_empty());
    }
}
