// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap, HashSet};
use sui_types::{
    error::{SuiError, SuiResult},
    messages_consensus::ConsensusPosition,
};
use tokio::sync::watch;
use tracing::debug;

use mysten_common::sync::notify_read::NotifyRead;

/// The number of consensus rounds to retain transaction status information before garbage collection.
/// Used to expire positions from old rounds, as well as to check if a transaction is too far ahead of the last committed round.
/// Assuming a max round rate of 15/sec, this allows status updates to be valid within a window of ~25-30 seconds.
pub(crate) const CONSENSUS_STATUS_RETENTION_ROUNDS: u64 = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConsensusTxStatus {
    // Transaction is voted to accept by a quorum of validators on fastpath.
    FastpathCertified,
    // Transaction is rejected, either by a quorum of validators or indirectly post-commit.
    Rejected,
    // Transaction is finalized post commit.
    Finalized,
}

#[derive(Debug, Clone)]
pub(crate) enum NotifyReadConsensusTxStatusResult {
    // The consensus position to be read has been updated with a new status.
    Status(ConsensusTxStatus),
    // The consensus position to be read has expired.
    // Provided with the last committed round that was used to check for expiration.
    Expired(u64),
}

pub(crate) struct ConsensusTxStatusCache {
    inner: RwLock<Inner>,
    status_notify_read: NotifyRead<ConsensusPosition, ConsensusTxStatus>,
    /// Watch channel for last committed leader round updates
    last_committed_leader_round_tx: watch::Sender<Option<u64>>,
    last_committed_leader_round_rx: watch::Receiver<Option<u64>>,
}

#[derive(Default)]
struct Inner {
    /// A map of transaction position to its status from consensus.
    transaction_status: HashMap<ConsensusPosition, ConsensusTxStatus>,
    /// A map of consensus round to all transactions that were updated in that round.
    round_lookup_map: BTreeMap<u64, HashSet<ConsensusPosition>>,
}

impl ConsensusTxStatusCache {
    pub fn new() -> Self {
        let (last_committed_leader_round_tx, last_committed_leader_round_rx) = watch::channel(None);
        Self {
            inner: Default::default(),
            status_notify_read: Default::default(),
            last_committed_leader_round_tx,
            last_committed_leader_round_rx,
        }
    }

    pub fn set_transaction_status(
        &self,
        transaction_position: ConsensusPosition,
        status: ConsensusTxStatus,
    ) {
        debug!(
            "Setting transaction status for {:?}: {:?}",
            transaction_position, status
        );
        let mut inner = self.inner.write();
        if let Some(last_committed_leader_round) = *self.last_committed_leader_round_rx.borrow() {
            if transaction_position.block.round as u64 + CONSENSUS_STATUS_RETENTION_ROUNDS
                < last_committed_leader_round
            {
                return;
            }
        }
        let old_status = inner
            .transaction_status
            .insert(transaction_position, status);
        // Calls to set_transaction_status are async and can be out of order.
        // We need to handle cases where new status is in fact older than the old status,
        // or did not change.
        if old_status == Some(status) {
            return;
        }
        if let Some(old_status) = old_status {
            if status == ConsensusTxStatus::FastpathCertified {
                // If the new status is FastpathCertified, it must be older than the old status.
                // We need to reset the status back to the old status.
                inner
                    .transaction_status
                    .insert(transaction_position, old_status);
            } else if old_status != ConsensusTxStatus::FastpathCertified {
                // If neither old nor new status is FastpathCertified,
                // we must have a conflict (either from Rejected to Finalized, or from Finalized to Rejected).
                panic!(
                    "Conflicting status updates for transaction {:?}: {:?} -> {:?}",
                    transaction_position, old_status, status
                );
            }
        } else {
            // This is the first time we are setting the status for this transaction.
            // We need to add it to the round lookup map to track its expiration.
            inner
                .round_lookup_map
                .entry(transaction_position.block.round as u64)
                .or_default()
                .insert(transaction_position);
        }
        self.status_notify_read
            .notify(&transaction_position, &status);
    }

    pub async fn notify_read_transaction_status(
        &self,
        transaction_position: ConsensusPosition,
        old_status: Option<ConsensusTxStatus>,
    ) -> NotifyReadConsensusTxStatusResult {
        // TODO(fastpath): We should track the typical distance between the last committed round
        // and the requested round notified as metrics.
        let registration = self.status_notify_read.register_one(&transaction_position);
        let mut round_rx = self.last_committed_leader_round_rx.clone();
        {
            let inner = self.inner.read();
            if let Some(status) = inner.transaction_status.get(&transaction_position) {
                if Some(status) != old_status.as_ref() {
                    if let Some(old_status) = old_status {
                        // The only scenario where the status may change, is when the transaction
                        // is initially fastpath certified, and then later finalized or rejected.
                        assert_eq!(old_status, ConsensusTxStatus::FastpathCertified);
                    }
                    return NotifyReadConsensusTxStatusResult::Status(*status);
                }
            }
            // Inner read lock dropped here.
        }

        let expiration_check = async {
            loop {
                if let Some(last_committed_leader_round) = *round_rx.borrow() {
                    if transaction_position.block.round as u64 + CONSENSUS_STATUS_RETENTION_ROUNDS
                        < last_committed_leader_round
                    {
                        return last_committed_leader_round;
                    }
                }
                // Channel closed - this should never happen in practice, so panic
                round_rx
                    .changed()
                    .await
                    .expect("last_committed_leader_round watch channel closed unexpectedly");
            }
        };
        tokio::select! {
            status = registration => NotifyReadConsensusTxStatusResult::Status(status),
            last_committed_leader_round = expiration_check => NotifyReadConsensusTxStatusResult::Expired(last_committed_leader_round),
        }
    }

    pub async fn update_last_committed_leader_round(&self, round: u64) {
        debug!("Updating last committed leader round: {}", round);
        let mut inner = self.inner.write();
        while let Some(&next_round) = inner.round_lookup_map.keys().next() {
            if next_round + CONSENSUS_STATUS_RETENTION_ROUNDS < round {
                let transactions = inner.round_lookup_map.remove(&next_round).unwrap();
                for tx in transactions {
                    inner.transaction_status.remove(&tx);
                }
            } else {
                break;
            }
        }
        // Send update through watch channel
        let _ = self.last_committed_leader_round_tx.send(Some(round));
    }

    /// Returns true if the position is too far ahead of the last committed round.
    pub fn check_position_too_ahead(&self, position: &ConsensusPosition) -> SuiResult<()> {
        if let Some(last_committed_leader_round) = *self.last_committed_leader_round_rx.borrow() {
            if position.block.round as u64
                > last_committed_leader_round + CONSENSUS_STATUS_RETENTION_ROUNDS
            {
                return Err(SuiError::ValidatorConsensusLagging {
                    round: position.block.round as u64,
                    last_committed_round: last_committed_leader_round,
                });
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn get_transaction_status(
        &self,
        position: &ConsensusPosition,
    ) -> Option<ConsensusTxStatus> {
        let inner = self.inner.read();
        inner.transaction_status.get(position).cloned()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use super::*;
    use consensus_core::BlockRef;
    use futures::FutureExt;
    use sui_types::messages_consensus::TransactionIndex;

    fn create_test_tx_position(round: u64, index: u64) -> ConsensusPosition {
        ConsensusPosition {
            block: BlockRef {
                round: round as u32,
                author: Default::default(),
                digest: Default::default(),
            },
            index: index as TransactionIndex,
        }
    }

    #[tokio::test]
    async fn test_set_and_get_transaction_status() {
        let cache = ConsensusTxStatusCache::new();
        let tx_pos = create_test_tx_position(1, 0);

        // Set initial status
        cache.set_transaction_status(tx_pos, ConsensusTxStatus::FastpathCertified);

        // Read status immediately
        let result = cache.notify_read_transaction_status(tx_pos, None).await;
        assert!(matches!(
            result,
            NotifyReadConsensusTxStatusResult::Status(ConsensusTxStatus::FastpathCertified)
        ));
    }

    #[tokio::test]
    async fn test_status_notification() {
        let cache = Arc::new(ConsensusTxStatusCache::new());
        let tx_pos = create_test_tx_position(1, 0);

        // Spawn a task that waits for status update
        let cache_clone = cache.clone();
        let handle = tokio::spawn(async move {
            cache_clone
                .notify_read_transaction_status(tx_pos, None)
                .await
        });

        // Small delay to ensure the task is waiting
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Set the status
        cache.set_transaction_status(tx_pos, ConsensusTxStatus::Finalized);

        // Verify the notification was received
        let result = handle.await.unwrap();
        assert!(matches!(
            result,
            NotifyReadConsensusTxStatusResult::Status(ConsensusTxStatus::Finalized)
        ));
    }

    #[tokio::test]
    async fn test_round_expiration() {
        let cache = ConsensusTxStatusCache::new();
        let tx_pos = create_test_tx_position(1, 0);

        // Set initial status
        cache.set_transaction_status(tx_pos, ConsensusTxStatus::FastpathCertified);

        // Update last committed round to trigger expiration
        cache
            .update_last_committed_leader_round(CONSENSUS_STATUS_RETENTION_ROUNDS + 2)
            .await;

        // Try to read status - should be expired
        let result = cache.notify_read_transaction_status(tx_pos, None).await;
        assert!(matches!(
            result,
            NotifyReadConsensusTxStatusResult::Expired(_)
        ));
    }

    #[tokio::test]
    async fn test_multiple_status_updates() {
        let cache = ConsensusTxStatusCache::new();
        let tx_pos = create_test_tx_position(1, 0);

        // Set initial status
        cache.set_transaction_status(tx_pos, ConsensusTxStatus::FastpathCertified);

        // Update status
        cache.set_transaction_status(tx_pos, ConsensusTxStatus::Finalized);

        // Read with old status
        let result = cache
            .notify_read_transaction_status(tx_pos, Some(ConsensusTxStatus::FastpathCertified))
            .await;
        assert!(matches!(
            result,
            NotifyReadConsensusTxStatusResult::Status(ConsensusTxStatus::Finalized)
        ));
    }

    #[tokio::test]
    async fn test_cleanup_expired_rounds() {
        let cache = ConsensusTxStatusCache::new();

        // Add transactions for multiple rounds
        for round in 1..=5 {
            let tx_pos = create_test_tx_position(round, 0);
            cache.set_transaction_status(tx_pos, ConsensusTxStatus::FastpathCertified);
        }

        // Update last committed round to expire early rounds
        cache
            .update_last_committed_leader_round(CONSENSUS_STATUS_RETENTION_ROUNDS + 3)
            .await;

        // Verify early rounds are cleaned up
        let inner = cache.inner.read();
        assert!(!inner.round_lookup_map.contains_key(&1));
        assert!(!inner.round_lookup_map.contains_key(&2));
        assert!(inner.round_lookup_map.contains_key(&4));
        assert!(inner.round_lookup_map.contains_key(&5));
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        let cache = Arc::new(ConsensusTxStatusCache::new());
        let tx_pos = create_test_tx_position(1, 0);

        // Spawn multiple tasks that wait for status
        let mut handles = vec![];
        for _ in 0..3 {
            let cache_clone = cache.clone();
            handles.push(tokio::spawn(async move {
                cache_clone
                    .notify_read_transaction_status(tx_pos, None)
                    .await
            }));
        }

        // Small delay to ensure tasks are waiting
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Set the status
        cache.set_transaction_status(tx_pos, ConsensusTxStatus::Finalized);

        // Verify all notifications were received
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(matches!(
                result,
                NotifyReadConsensusTxStatusResult::Status(ConsensusTxStatus::Finalized)
            ));
        }
    }

    #[tokio::test]
    async fn test_out_of_order_status_updates() {
        let cache = ConsensusTxStatusCache::new();
        let tx_pos = create_test_tx_position(1, 0);

        // First update status to Rejected
        cache.set_transaction_status(tx_pos, ConsensusTxStatus::Rejected);
        let result = cache.notify_read_transaction_status(tx_pos, None).await;
        assert!(matches!(
            result,
            NotifyReadConsensusTxStatusResult::Status(ConsensusTxStatus::Rejected)
        ));

        // We should not receive a new status update since the new status is older than the old status.
        cache.set_transaction_status(tx_pos, ConsensusTxStatus::FastpathCertified);
        let result = cache
            .notify_read_transaction_status(tx_pos, Some(ConsensusTxStatus::Rejected))
            .now_or_never();
        assert!(result.is_none());
        assert_eq!(
            cache.get_transaction_status(&tx_pos),
            Some(ConsensusTxStatus::Rejected)
        );
    }
}
