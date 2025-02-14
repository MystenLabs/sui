// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::{collections::BTreeMap, sync::Arc};

use mysten_common::debug_fatal;
use mysten_metrics::monitored_mpsc::{channel, Receiver, Sender};
use parking_lot::Mutex;
use tap::tap::TapFallible;
use thiserror::Error;
use tokio::sync::oneshot;
use tracing::{error, warn};

use crate::{
    block::{BlockRef, Transaction, TransactionIndex},
    context::Context,
    Round,
};

/// The maximum number of transactions pending to the queue to be pulled for block proposal
const MAX_PENDING_TRANSACTIONS: usize = 2_000;

/// The guard acts as an acknowledgment mechanism for the inclusion of the transactions to a block.
/// When its last transaction is included to a block then `included_in_block_ack` will be signalled.
/// If the guard is dropped without getting acknowledged that means the transactions have not been
/// included to a block and the consensus is shutting down.
pub(crate) struct TransactionsGuard {
    // Holds a list of transactions to be included in the block.
    // A TransactionsGuard may be partially consumed by `TransactionConsumer`, in which case, this holds the remaining transactions.
    transactions: Vec<Transaction>,

    included_in_block_ack: oneshot::Sender<(BlockRef, oneshot::Receiver<BlockStatus>)>,
}

/// The TransactionConsumer is responsible for fetching the next transactions to be included for the block proposals.
/// The transactions are submitted to a channel which is shared between the TransactionConsumer and the TransactionClient
/// and are pulled every time the `next` method is called.
pub(crate) struct TransactionConsumer {
    context: Arc<Context>,
    tx_receiver: Receiver<TransactionsGuard>,
    max_transactions_in_block_bytes: u64,
    max_num_transactions_in_block: u64,
    pending_transactions: Option<TransactionsGuard>,
    block_status_subscribers: Arc<Mutex<BTreeMap<BlockRef, Vec<oneshot::Sender<BlockStatus>>>>>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[allow(unused)]
pub enum BlockStatus {
    /// The block has been sequenced as part of a committed sub dag. That means that any transaction that has been included in the block
    /// has been committed as well.
    Sequenced(BlockRef),
    /// The block has been garbage collected and will never be committed. Any transactions that have been included in the block should also
    /// be considered as impossible to be committed as part of this block and might need to be retried
    GarbageCollected(BlockRef),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LimitReached {
    // The maximum number of transactions have been included
    MaxNumOfTransactions,
    // The maximum number of bytes have been included
    MaxBytes,
    // All available transactions have been included
    AllTransactionsIncluded,
}

impl TransactionConsumer {
    pub(crate) fn new(tx_receiver: Receiver<TransactionsGuard>, context: Arc<Context>) -> Self {
        Self {
            tx_receiver,
            max_transactions_in_block_bytes: context
                .protocol_config
                .max_transactions_in_block_bytes(),
            max_num_transactions_in_block: context.protocol_config.max_num_transactions_in_block(),
            context,
            pending_transactions: None,
            block_status_subscribers: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    // Attempts to fetch the next transactions that have been submitted for sequence. Respects the `max_transactions_in_block_bytes`
    // and `max_num_transactions_in_block` parameters specified via protocol config.
    // This returns one or more transactions to be included in the block and a callback to acknowledge the inclusion of those transactions.
    // Also returns a `LimitReached` enum to indicate which limit type has been reached.
    pub(crate) fn next(&mut self) -> (Vec<Transaction>, Box<dyn FnOnce(BlockRef)>, LimitReached) {
        let mut transactions = Vec::new();
        let mut acks = Vec::new();
        let mut total_bytes = 0;
        let mut limit_reached = LimitReached::AllTransactionsIncluded;

        // Handle one batch of incoming transactions from TransactionGuard.
        // The method will return `None` if all the transactions can be included in the block. Otherwise none of the transactions will be
        // included in the block and the method will return the TransactionGuard.
        let mut handle_txs = |t: TransactionsGuard| -> Option<TransactionsGuard> {
            let transactions_bytes =
                t.transactions.iter().map(|t| t.data().len()).sum::<usize>() as u64;
            let transactions_num = t.transactions.len() as u64;

            if total_bytes + transactions_bytes > self.max_transactions_in_block_bytes {
                limit_reached = LimitReached::MaxBytes;
                return Some(t);
            }
            if transactions.len() as u64 + transactions_num > self.max_num_transactions_in_block {
                limit_reached = LimitReached::MaxNumOfTransactions;
                return Some(t);
            }

            total_bytes += transactions_bytes;

            // The transactions can be consumed, register its ack.
            acks.push(t.included_in_block_ack);
            transactions.extend(t.transactions);
            None
        };

        if let Some(t) = self.pending_transactions.take() {
            if let Some(pending_transactions) = handle_txs(t) {
                debug_fatal!("Previously pending transaction(s) should fit into an empty block! Dropping: {:?}", pending_transactions.transactions);
            }
        }

        // Until we have reached the limit for the pull.
        // We may have already reached limit in the first iteration above, in which case we stop immediately.
        while self.pending_transactions.is_none() {
            if let Ok(t) = self.tx_receiver.try_recv() {
                self.pending_transactions = handle_txs(t);
            } else {
                break;
            }
        }

        let block_status_subscribers = self.block_status_subscribers.clone();
        let gc_enabled = self.context.protocol_config.gc_depth() > 0;
        (
            transactions,
            Box::new(move |block_ref: BlockRef| {
                let mut block_status_subscribers = block_status_subscribers.lock();

                for ack in acks {
                    let (status_tx, status_rx) = oneshot::channel();

                    if gc_enabled {
                        block_status_subscribers
                            .entry(block_ref)
                            .or_default()
                            .push(status_tx);
                    } else {
                        // When gc is not enabled, then report directly the block as sequenced while tx is acknowledged for inclusion.
                        // As blocks can never get garbage collected it is there is actually no meaning to do otherwise and also is safer for edge cases.
                        status_tx.send(BlockStatus::Sequenced(block_ref)).ok();
                    }

                    let _ = ack.send((block_ref, status_rx));
                }
            }),
            limit_reached,
        )
    }

    /// Notifies all the transaction submitters who are waiting to receive an update on the status of the block.
    /// The `committed_blocks` are the blocks that have been committed and the `gc_round` is the round up to which the blocks have been garbage collected.
    /// First we'll notify for all the committed blocks, and then for all the blocks that have been garbage collected.
    pub(crate) fn notify_own_blocks_status(
        &self,
        committed_blocks: Vec<BlockRef>,
        gc_round: Round,
    ) {
        // Notify for all the committed blocks first
        let mut block_status_subscribers = self.block_status_subscribers.lock();
        for block_ref in committed_blocks {
            if let Some(subscribers) = block_status_subscribers.remove(&block_ref) {
                subscribers.into_iter().for_each(|s| {
                    let _ = s.send(BlockStatus::Sequenced(block_ref));
                });
            }
        }

        // Now notify everyone <= gc_round that their block has been garbage collected and clean up the entries
        while let Some((block_ref, subscribers)) = block_status_subscribers.pop_first() {
            if block_ref.round <= gc_round {
                subscribers.into_iter().for_each(|s| {
                    let _ = s.send(BlockStatus::GarbageCollected(block_ref));
                });
            } else {
                block_status_subscribers.insert(block_ref, subscribers);
                break;
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn subscribe_for_block_status_testing(
        &self,
        block_ref: BlockRef,
    ) -> oneshot::Receiver<BlockStatus> {
        let (tx, rx) = oneshot::channel();
        let mut block_status_subscribers = self.block_status_subscribers.lock();
        block_status_subscribers
            .entry(block_ref)
            .or_default()
            .push(tx);
        rx
    }

    #[cfg(test)]
    fn is_empty(&mut self) -> bool {
        if self.pending_transactions.is_some() {
            return false;
        }
        if let Ok(t) = self.tx_receiver.try_recv() {
            self.pending_transactions = Some(t);
            return false;
        }
        true
    }
}

#[derive(Clone)]
pub struct TransactionClient {
    sender: Sender<TransactionsGuard>,
    max_transaction_size: u64,
    max_transactions_in_block_bytes: u64,
    max_transactions_in_block_count: u64,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Failed to submit transaction, consensus is shutting down: {0}")]
    ConsensusShuttingDown(String),

    #[error("Transaction size ({0}B) is over limit ({1}B)")]
    OversizedTransaction(u64, u64),

    #[error("Transaction bundle size ({0}B) is over limit ({1}B)")]
    OversizedTransactionBundleBytes(u64, u64),

    #[error("Transaction bundle count ({0}) is over limit ({1})")]
    OversizedTransactionBundleCount(u64, u64),
}

impl TransactionClient {
    pub(crate) fn new(context: Arc<Context>) -> (Self, Receiver<TransactionsGuard>) {
        let (sender, receiver) = channel("consensus_input", MAX_PENDING_TRANSACTIONS);

        (
            Self {
                sender,
                max_transaction_size: context.protocol_config.max_transaction_size_bytes(),
                max_transactions_in_block_bytes: context
                    .protocol_config
                    .max_transactions_in_block_bytes(),
                max_transactions_in_block_count: context
                    .protocol_config
                    .max_num_transactions_in_block(),
            },
            receiver,
        )
    }

    /// Submits a list of transactions to be sequenced. The method returns when all the transactions have been successfully included
    /// to next proposed blocks.
    pub async fn submit(
        &self,
        transactions: Vec<Vec<u8>>,
    ) -> Result<(BlockRef, oneshot::Receiver<BlockStatus>), ClientError> {
        // TODO: Support returning the block refs for transactions that span multiple blocks
        let included_in_block = self.submit_no_wait(transactions).await?;
        included_in_block
            .await
            .tap_err(|e| warn!("Transaction acknowledge failed with {:?}", e))
            .map_err(|e| ClientError::ConsensusShuttingDown(e.to_string()))
    }

    /// Submits a list of transactions to be sequenced.
    /// If any transaction's length exceeds `max_transaction_size`, no transaction will be submitted.
    /// That shouldn't be the common case as sizes should be aligned between consensus and client. The method returns
    /// a receiver to wait on until the transactions has been included in the next block to get proposed. The consumer should
    /// wait on it to consider as inclusion acknowledgement. If the receiver errors then consensus is shutting down and transaction
    /// has not been included to any block.
    /// If multiple transactions are submitted, the method will attempt to bundle them together in a single block. If the total size of
    /// the transactions exceeds `max_transactions_in_block_bytes`, no transaction will be submitted and an error will be returned instead.
    /// Similar if transactions exceed `max_transactions_in_block_count` an error will be returned.
    pub(crate) async fn submit_no_wait(
        &self,
        transactions: Vec<Vec<u8>>,
    ) -> Result<oneshot::Receiver<(BlockRef, oneshot::Receiver<BlockStatus>)>, ClientError> {
        let (included_in_block_ack_send, included_in_block_ack_receive) = oneshot::channel();

        let mut bundle_size = 0;

        if transactions.len() as u64 > self.max_transactions_in_block_count {
            return Err(ClientError::OversizedTransactionBundleCount(
                transactions.len() as u64,
                self.max_transactions_in_block_count,
            ));
        }

        for transaction in &transactions {
            if transaction.len() as u64 > self.max_transaction_size {
                return Err(ClientError::OversizedTransaction(
                    transaction.len() as u64,
                    self.max_transaction_size,
                ));
            }
            bundle_size += transaction.len() as u64;

            if bundle_size > self.max_transactions_in_block_bytes {
                return Err(ClientError::OversizedTransactionBundleBytes(
                    bundle_size,
                    self.max_transactions_in_block_bytes,
                ));
            }
        }

        let t = TransactionsGuard {
            transactions: transactions.into_iter().map(Transaction::new).collect(),
            included_in_block_ack: included_in_block_ack_send,
        };
        self.sender
            .send(t)
            .await
            .tap_err(|e| error!("Submit transactions failed with {:?}", e))
            .map_err(|e| ClientError::ConsensusShuttingDown(e.to_string()))?;
        Ok(included_in_block_ack_receive)
    }
}

/// `TransactionVerifier` implementation is supplied by Sui to validate transactions in a block,
/// before acceptance of the block.
pub trait TransactionVerifier: Send + Sync + 'static {
    /// Determines if this batch of transactions is valid.
    /// Fails if any one of the transactions is invalid.
    fn verify_batch(&self, batch: &[&[u8]]) -> Result<(), ValidationError>;

    /// Returns indices of transactions to reject, or a transaction validation error.
    /// Currently only uncertified user transactions can be voted to reject, which are created
    /// by Mysticeti fastpath client.
    /// Honest validators may disagree on voting for uncertified user transactions.
    /// The other types of transactions are implicitly voted to be accepted if they pass validation.
    ///
    /// Honest validators should produce the same validation outcome on the same batch of
    /// transactions. So if a batch from a peer fails validation, the peer is equivocating.
    fn verify_and_vote_batch(
        &self,
        batch: &[&[u8]],
    ) -> Result<Vec<TransactionIndex>, ValidationError>;
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Invalid transaction: {0}")]
    InvalidTransaction(String),
}

/// `NoopTransactionVerifier` accepts all transactions.
#[cfg(any(test, msim))]
pub struct NoopTransactionVerifier;

#[cfg(any(test, msim))]
impl TransactionVerifier for NoopTransactionVerifier {
    fn verify_batch(&self, _batch: &[&[u8]]) -> Result<(), ValidationError> {
        Ok(())
    }

    fn verify_and_vote_batch(
        &self,
        _batch: &[&[u8]],
    ) -> Result<Vec<TransactionIndex>, ValidationError> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use consensus_config::AuthorityIndex;
    use futures::{stream::FuturesUnordered, StreamExt};
    use sui_protocol_config::ProtocolConfig;
    use tokio::time::timeout;

    use crate::transaction::NoopTransactionVerifier;
    use crate::{
        block::{BlockDigest, BlockRef},
        block_verifier::SignedBlockVerifier,
        context::Context,
        transaction::{BlockStatus, LimitReached, TransactionClient, TransactionConsumer},
    };

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn basic_submit_and_consume() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_consensus_max_transaction_size_bytes_for_testing(2_000); // 2KB
            config.set_consensus_max_transactions_in_block_bytes_for_testing(2_000);
            config
        });

        let context = Arc::new(Context::new_for_test(4).0);
        let (client, tx_receiver) = TransactionClient::new(context.clone());
        let mut consumer = TransactionConsumer::new(tx_receiver, context.clone());

        // submit asynchronously the transactions and keep the waiters
        let mut included_in_block_waiters = FuturesUnordered::new();
        for i in 0..3 {
            let transaction =
                bcs::to_bytes(&format!("transaction {i}")).expect("Serialization should not fail.");
            let w = client
                .submit_no_wait(vec![transaction])
                .await
                .expect("Shouldn't submit successfully transaction");
            included_in_block_waiters.push(w);
        }

        // now pull the transactions from the consumer
        let (transactions, ack_transactions, _limit_reached) = consumer.next();
        assert_eq!(transactions.len(), 3);

        for (i, t) in transactions.iter().enumerate() {
            let t: String = bcs::from_bytes(t.data()).unwrap();
            assert_eq!(format!("transaction {i}").to_string(), t);
        }

        assert!(
            timeout(Duration::from_secs(1), included_in_block_waiters.next())
                .await
                .is_err(),
            "We should expect to timeout as none of the transactions have been acknowledged yet"
        );

        // Now acknowledge the inclusion of transactions
        ack_transactions(BlockRef::MIN);

        // Now make sure that all the waiters have returned
        while let Some(result) = included_in_block_waiters.next().await {
            assert!(result.is_ok());
        }

        // try to pull again transactions, result should be empty
        assert!(consumer.is_empty());
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn block_status_update_gc_enabled() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_consensus_max_transaction_size_bytes_for_testing(2_000); // 2KB
            config.set_consensus_max_transactions_in_block_bytes_for_testing(2_000);
            config.set_consensus_gc_depth_for_testing(10);
            config
        });

        let context = Arc::new(Context::new_for_test(4).0);
        let (client, tx_receiver) = TransactionClient::new(context.clone());
        let mut consumer = TransactionConsumer::new(tx_receiver, context.clone());

        // submit the transactions and include 2 of each on a new block
        let mut included_in_block_waiters = FuturesUnordered::new();
        for i in 1..=10 {
            let transaction =
                bcs::to_bytes(&format!("transaction {i}")).expect("Serialization should not fail.");
            let w = client
                .submit_no_wait(vec![transaction])
                .await
                .expect("Shouldn't submit successfully transaction");
            included_in_block_waiters.push(w);

            // Every 2 transactions simulate the creation of a new block and acknowledge the inclusion of the transactions
            if i % 2 == 0 {
                let (transactions, ack_transactions, _limit_reached) = consumer.next();
                assert_eq!(transactions.len(), 2);
                ack_transactions(BlockRef::new(
                    i,
                    AuthorityIndex::new_for_test(0),
                    BlockDigest::MIN,
                ));
            }
        }

        // Now iterate over all the waiters. Everyone should have been acknowledged.
        let mut block_status_waiters = Vec::new();
        while let Some(result) = included_in_block_waiters.next().await {
            let (block_ref, block_status_waiter) =
                result.expect("Block inclusion waiter shouldn't fail");
            block_status_waiters.push((block_ref, block_status_waiter));
        }

        // Now acknowledge the commit of the blocks 6, 8, 10 and set gc_round = 5, which should trigger the garbage collection of blocks 1..=5
        let gc_round = 5;
        consumer.notify_own_blocks_status(
            vec![
                BlockRef::new(6, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
                BlockRef::new(8, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
                BlockRef::new(10, AuthorityIndex::new_for_test(0), BlockDigest::MIN),
            ],
            gc_round,
        );

        // Now iterate over all the block status waiters. Everyone should have been notified.
        for (block_ref, waiter) in block_status_waiters {
            let block_status = waiter.await.expect("Block status waiter shouldn't fail");

            if block_ref.round <= gc_round {
                assert!(matches!(block_status, BlockStatus::GarbageCollected(_)))
            } else {
                assert!(matches!(block_status, BlockStatus::Sequenced(_)));
            }
        }

        // Ensure internal structure is clear
        assert!(consumer.block_status_subscribers.lock().is_empty());
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn block_status_update_gc_disabled() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_consensus_max_transaction_size_bytes_for_testing(2_000); // 2KB
            config.set_consensus_max_transactions_in_block_bytes_for_testing(2_000);
            config.set_consensus_gc_depth_for_testing(0);
            config
        });

        let context = Arc::new(Context::new_for_test(4).0);
        let (client, tx_receiver) = TransactionClient::new(context.clone());
        let mut consumer = TransactionConsumer::new(tx_receiver, context.clone());

        // submit the transactions and include 2 of each on a new block
        let mut included_in_block_waiters = FuturesUnordered::new();
        for i in 1..=10 {
            let transaction =
                bcs::to_bytes(&format!("transaction {i}")).expect("Serialization should not fail.");
            let w = client
                .submit_no_wait(vec![transaction])
                .await
                .expect("Shouldn't submit successfully transaction");
            included_in_block_waiters.push(w);

            // Every 2 transactions simulate the creation of a new block and acknowledge the inclusion of the transactions
            if i % 2 == 0 {
                let (transactions, ack_transactions, _limit_reached) = consumer.next();
                assert_eq!(transactions.len(), 2);
                ack_transactions(BlockRef::new(
                    i,
                    AuthorityIndex::new_for_test(0),
                    BlockDigest::MIN,
                ));
            }
        }

        // Now iterate over all the waiters. Everyone should have been acknowledged.
        let mut block_status_waiters = Vec::new();
        while let Some(result) = included_in_block_waiters.next().await {
            let (block_ref, block_status_waiter) =
                result.expect("Block inclusion waiter shouldn't fail");
            block_status_waiters.push((block_ref, block_status_waiter));
        }

        // Now iterate over all the block status waiters. Everyone should have been notified and everyone should be considered sequenced.
        for (_block_ref, waiter) in block_status_waiters {
            let block_status = waiter.await.expect("Block status waiter shouldn't fail");
            assert!(matches!(block_status, BlockStatus::Sequenced(_)));
        }

        // Ensure internal structure is clear
        assert!(consumer.block_status_subscribers.lock().is_empty());
    }

    #[tokio::test]
    async fn submit_over_max_fetch_size_and_consume() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_consensus_max_transaction_size_bytes_for_testing(100);
            config.set_consensus_max_transactions_in_block_bytes_for_testing(100);
            config
        });

        let context = Arc::new(Context::new_for_test(4).0);
        let (client, tx_receiver) = TransactionClient::new(context.clone());
        let mut consumer = TransactionConsumer::new(tx_receiver, context.clone());

        // submit some transactions
        for i in 0..10 {
            let transaction =
                bcs::to_bytes(&format!("transaction {i}")).expect("Serialization should not fail.");
            let _w = client
                .submit_no_wait(vec![transaction])
                .await
                .expect("Shouldn't submit successfully transaction");
        }

        // now pull the transactions from the consumer
        let mut all_transactions = Vec::new();
        let (transactions, _ack_transactions, _limit_reached) = consumer.next();
        assert_eq!(transactions.len(), 7);

        // ensure their total size is less than `max_bytes_to_fetch`
        let total_size: u64 = transactions.iter().map(|t| t.data().len() as u64).sum();
        assert!(
            total_size <= context.protocol_config.max_transactions_in_block_bytes(),
            "Should have fetched transactions up to {}",
            context.protocol_config.max_transactions_in_block_bytes()
        );
        all_transactions.extend(transactions);

        // try to pull again transactions, next should be provided
        let (transactions, _ack_transactions, _limit_reached) = consumer.next();
        assert_eq!(transactions.len(), 3);

        // ensure their total size is less than `max_bytes_to_fetch`
        let total_size: u64 = transactions.iter().map(|t| t.data().len() as u64).sum();
        assert!(
            total_size <= context.protocol_config.max_transactions_in_block_bytes(),
            "Should have fetched transactions up to {}",
            context.protocol_config.max_transactions_in_block_bytes()
        );
        all_transactions.extend(transactions);

        // try to pull again transactions, result should be empty
        assert!(consumer.is_empty());

        for (i, t) in all_transactions.iter().enumerate() {
            let t: String = bcs::from_bytes(t.data()).unwrap();
            assert_eq!(format!("transaction {i}").to_string(), t);
        }
    }

    #[tokio::test]
    async fn submit_large_batch_and_ack() {
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_consensus_max_transaction_size_bytes_for_testing(15);
            config.set_consensus_max_transactions_in_block_bytes_for_testing(200);
            config
        });

        let context = Arc::new(Context::new_for_test(4).0);
        let (client, tx_receiver) = TransactionClient::new(context.clone());
        let mut consumer = TransactionConsumer::new(tx_receiver, context.clone());
        let mut all_receivers = Vec::new();
        // submit a few transactions individually.
        for i in 0..10 {
            let transaction =
                bcs::to_bytes(&format!("transaction {i}")).expect("Serialization should not fail.");
            let w = client
                .submit_no_wait(vec![transaction])
                .await
                .expect("Should submit successfully transaction");
            all_receivers.push(w);
        }

        // construct an acceptable batch and submit, it should be accepted
        {
            let transactions: Vec<_> = (10..15)
                .map(|i| {
                    bcs::to_bytes(&format!("transaction {i}"))
                        .expect("Serialization should not fail.")
                })
                .collect();
            let w = client
                .submit_no_wait(transactions)
                .await
                .expect("Should submit successfully transaction");
            all_receivers.push(w);
        }

        // submit another individual transaction.
        {
            let i = 15;
            let transaction =
                bcs::to_bytes(&format!("transaction {i}")).expect("Serialization should not fail.");
            let w = client
                .submit_no_wait(vec![transaction])
                .await
                .expect("Shouldn't submit successfully transaction");
            all_receivers.push(w);
        }

        // construct a over-size-limit batch and submit, it should not be accepted
        {
            let transactions: Vec<_> = (16..32)
                .map(|i| {
                    bcs::to_bytes(&format!("transaction {i}"))
                        .expect("Serialization should not fail.")
                })
                .collect();
            let result = client.submit_no_wait(transactions).await.unwrap_err();
            assert_eq!(
                result.to_string(),
                "Transaction bundle size (210B) is over limit (200B)"
            );
        }

        // now pull the transactions from the consumer.
        // we expect all transactions are fetched in order, not missing any, and not exceeding the size limit.
        let mut all_acks: Vec<Box<dyn FnOnce(BlockRef)>> = Vec::new();
        let mut batch_index = 0;
        while !consumer.is_empty() {
            let (transactions, ack_transactions, _limit_reached) = consumer.next();

            assert!(
                transactions.len() as u64
                    <= context.protocol_config.max_num_transactions_in_block(),
                "Should have fetched transactions up to {}",
                context.protocol_config.max_num_transactions_in_block()
            );

            let total_size: u64 = transactions.iter().map(|t| t.data().len() as u64).sum();
            assert!(
                total_size <= context.protocol_config.max_transactions_in_block_bytes(),
                "Should have fetched transactions up to {}",
                context.protocol_config.max_transactions_in_block_bytes()
            );

            // first batch should contain all transactions from 0..10. The softbundle it is to big to fit as well, so it's parked.
            if batch_index == 0 {
                assert_eq!(transactions.len(), 10);
                for (i, transaction) in transactions.iter().enumerate() {
                    let t: String = bcs::from_bytes(transaction.data()).unwrap();
                    assert_eq!(format!("transaction {}", i).to_string(), t);
                }
            // second batch will contain the soft bundle and the additional last transaction.
            } else if batch_index == 1 {
                assert_eq!(transactions.len(), 6);
                for (i, transaction) in transactions.iter().enumerate() {
                    let t: String = bcs::from_bytes(transaction.data()).unwrap();
                    assert_eq!(format!("transaction {}", i + 10).to_string(), t);
                }
            } else {
                panic!("Unexpected batch index");
            }

            batch_index += 1;

            all_acks.push(ack_transactions);
        }

        // now acknowledge the inclusion of all transactions.
        for ack in all_acks {
            ack(BlockRef::MIN);
        }

        // expect all receivers to be resolved.
        for w in all_receivers {
            let r = w.await;
            assert!(r.is_ok());
        }
    }

    #[tokio::test]
    async fn test_submit_over_max_block_size_and_validate_block_size() {
        // submit transactions individually so we make sure that we have reached the block size limit of 10
        {
            let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
                config.set_consensus_max_transaction_size_bytes_for_testing(100);
                config.set_consensus_max_num_transactions_in_block_for_testing(10);
                config.set_consensus_max_transactions_in_block_bytes_for_testing(300);
                config
            });

            let context = Arc::new(Context::new_for_test(4).0);
            let (client, tx_receiver) = TransactionClient::new(context.clone());
            let mut consumer = TransactionConsumer::new(tx_receiver, context.clone());
            let mut all_receivers = Vec::new();

            // create enough transactions
            let max_num_transactions_in_block =
                context.protocol_config.max_num_transactions_in_block();
            for i in 0..2 * max_num_transactions_in_block {
                let transaction = bcs::to_bytes(&format!("transaction {i}"))
                    .expect("Serialization should not fail.");
                let w = client
                    .submit_no_wait(vec![transaction])
                    .await
                    .expect("Should submit successfully transaction");
                all_receivers.push(w);
            }

            // Fetch the next transactions to be included in a block
            let (transactions, _ack_transactions, limit) = consumer.next();
            assert_eq!(limit, LimitReached::MaxNumOfTransactions);
            assert_eq!(transactions.len() as u64, max_num_transactions_in_block);

            // Now create a block and verify that transactions are within the size limits
            let block_verifier =
                SignedBlockVerifier::new(context.clone(), Arc::new(NoopTransactionVerifier {}));

            let batch: Vec<_> = transactions.iter().map(|t| t.data()).collect();
            assert!(
                block_verifier.check_transactions(&batch).is_ok(),
                "Number of transactions limit verification failed"
            );
        }

        // submit transactions individually so we make sure that we have reached the block size bytes 300
        {
            let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
                config.set_consensus_max_transaction_size_bytes_for_testing(100);
                config.set_consensus_max_num_transactions_in_block_for_testing(1_000);
                config.set_consensus_max_transactions_in_block_bytes_for_testing(300);
                config
            });

            let context = Arc::new(Context::new_for_test(4).0);
            let (client, tx_receiver) = TransactionClient::new(context.clone());
            let mut consumer = TransactionConsumer::new(tx_receiver, context.clone());
            let mut all_receivers = Vec::new();

            let max_transactions_in_block_bytes =
                context.protocol_config.max_transactions_in_block_bytes();
            let mut total_size = 0;
            loop {
                let transaction = bcs::to_bytes(&"transaction".to_string())
                    .expect("Serialization should not fail.");
                total_size += transaction.len() as u64;
                let w = client
                    .submit_no_wait(vec![transaction])
                    .await
                    .expect("Should submit successfully transaction");
                all_receivers.push(w);

                // create enough transactions to reach the block size limit
                if total_size >= 2 * max_transactions_in_block_bytes {
                    break;
                }
            }

            // Fetch the next transactions to be included in a block
            let (transactions, _ack_transactions, limit) = consumer.next();
            let batch: Vec<_> = transactions.iter().map(|t| t.data()).collect();
            let size = batch.iter().map(|t| t.len() as u64).sum::<u64>();

            assert_eq!(limit, LimitReached::MaxBytes);
            assert!(
                batch.len()
                    < context
                        .protocol_config
                        .consensus_max_num_transactions_in_block() as usize,
                "Should have submitted less than the max number of transactions in a block"
            );
            assert!(size <= max_transactions_in_block_bytes);

            // Now create a block and verify that transactions are within the size limits
            let block_verifier =
                SignedBlockVerifier::new(context.clone(), Arc::new(NoopTransactionVerifier {}));

            assert!(
                block_verifier.check_transactions(&batch).is_ok(),
                "Total size of transactions limit verification failed"
            );
        }
    }
}
