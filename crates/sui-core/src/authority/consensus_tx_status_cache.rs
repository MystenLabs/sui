// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use parking_lot::RwLock;
use std::collections::{btree_map::Entry, BTreeMap};
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
pub(crate) const CONSENSUS_STATUS_RETENTION_ROUNDS: u32 = 400;

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
    Expired(u32),
}

pub(crate) struct ConsensusTxStatusCache {
    inner: RwLock<Inner>,
    status_notify_read: NotifyRead<ConsensusPosition, ConsensusTxStatus>,
    /// Watch channel for last committed leader round updates
    last_committed_leader_round_tx: watch::Sender<Option<u32>>,
    last_committed_leader_round_rx: watch::Receiver<Option<u32>>,
}

#[derive(Default)]
struct Inner {
    /// A map of transaction position to its status from consensus.
    transaction_status: BTreeMap<ConsensusPosition, ConsensusTxStatus>,
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

    pub fn set_transaction_status(&self, pos: ConsensusPosition, status: ConsensusTxStatus) {
        let mut inner = self.inner.write();
        if let Some(last_committed_leader_round) = *self.last_committed_leader_round_rx.borrow() {
            if pos.block.round + CONSENSUS_STATUS_RETENTION_ROUNDS < last_committed_leader_round {
                // Ignore stale status updates.
                return;
            }
        }

        // Calls to set_transaction_status are async and can be out of order.
        // Makes sure this is tolerated by handling state transitions properly.
        let status_entry = inner.transaction_status.entry(pos);
        match status_entry {
            Entry::Vacant(entry) => {
                // Set the status for the first time.
                entry.insert(status);
            }
            Entry::Occupied(mut entry) => {
                let old_status = *entry.get();
                match (old_status, status) {
                    // If the statuses are the same, no update is needed.
                    (s1, s2) if s1 == s2 => return,
                    // FastpathCertified is transient and can be updated to other statuses.
                    (ConsensusTxStatus::FastpathCertified, _) => {
                        entry.insert(status);
                    }
                    // This happens when statuses arrive out-of-order, and is a no-op.
                    (
                        ConsensusTxStatus::Rejected | ConsensusTxStatus::Finalized,
                        ConsensusTxStatus::FastpathCertified,
                    ) => {
                        return;
                    }
                    // Transitions between terminal statuses are invalid.
                    _ => {
                        panic!(
                            "Conflicting status updates for transaction {:?}: {:?} -> {:?}",
                            pos, old_status, status
                        );
                    }
                }
            }
        };

        // All code paths leading to here should have set the status.
        debug!("Transaction status is set for {:?}: {:?}", pos, status);
        self.status_notify_read.notify(&pos, &status);
    }

    /// Given a known previous status provided by `old_status`, this function will return a new
    /// status once the transaction status has changed, or if the consensus position has expired.
    pub async fn notify_read_transaction_status_change(
        &self,
        consensus_position: ConsensusPosition,
        old_status: Option<ConsensusTxStatus>,
    ) -> NotifyReadConsensusTxStatusResult {
        // TODO(fastpath): We should track the typical distance between the last committed round
        // and the requested round notified as metrics.
        let registration = self.status_notify_read.register_one(&consensus_position);
        let mut round_rx = self.last_committed_leader_round_rx.clone();
        {
            let inner = self.inner.read();
            if let Some(status) = inner.transaction_status.get(&consensus_position) {
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
                    if consensus_position.block.round + CONSENSUS_STATUS_RETENTION_ROUNDS
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

    pub async fn update_last_committed_leader_round(&self, leader_round: u32) {
        debug!("Updating last committed leader round: {}", leader_round);
        let mut inner = self.inner.write();
        while let Some((position, _)) = inner.transaction_status.first_key_value() {
            if position.block.round + CONSENSUS_STATUS_RETENTION_ROUNDS <= leader_round {
                inner.transaction_status.pop_first();
            } else {
                break;
            }
        }
        // Send update through watch channel
        let _ = self.last_committed_leader_round_tx.send(Some(leader_round));
    }

    /// Returns true if the position is too far ahead of the last committed round.
    pub fn check_position_too_ahead(&self, position: &ConsensusPosition) -> SuiResult<()> {
        if let Some(last_committed_leader_round) = *self.last_committed_leader_round_rx.borrow() {
            if position.block.round
                > last_committed_leader_round + CONSENSUS_STATUS_RETENTION_ROUNDS
            {
                return Err(SuiError::ValidatorConsensusLagging {
                    round: position.block.round,
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
    use consensus_types::block::{BlockRef, TransactionIndex};

    fn create_test_tx_position(round: u64, index: u64) -> ConsensusPosition {
        ConsensusPosition {
            epoch: Default::default(),
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
        let result = cache
            .notify_read_transaction_status_change(tx_pos, None)
            .await;
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
                .notify_read_transaction_status_change(tx_pos, None)
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

        // Update last committed round to trigger expiration up to including round 2.
        cache
            .update_last_committed_leader_round(CONSENSUS_STATUS_RETENTION_ROUNDS + 2)
            .await;

        // Try to read status - should be expired
        let result = cache
            .notify_read_transaction_status_change(tx_pos, None)
            .await;
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
            .notify_read_transaction_status_change(
                tx_pos,
                Some(ConsensusTxStatus::FastpathCertified),
            )
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

        // Update last committed round to expire early rounds up to including round 3.
        cache
            .update_last_committed_leader_round(CONSENSUS_STATUS_RETENTION_ROUNDS + 3)
            .await;

        // Verify early rounds are cleaned up
        let inner = cache.inner.read();
        let rounds = inner
            .transaction_status
            .keys()
            .map(|p| p.block.round)
            .collect::<Vec<_>>();
        assert_eq!(rounds, vec![4, 5]);
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
                    .notify_read_transaction_status_change(tx_pos, None)
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
        let cache = Arc::new(ConsensusTxStatusCache::new());
        let tx_pos = create_test_tx_position(1, 0);

        // First update status to Finalized.
        cache.set_transaction_status(tx_pos, ConsensusTxStatus::Finalized);
        let result = cache
            .notify_read_transaction_status_change(tx_pos, None)
            .await;
        assert!(matches!(
            result,
            NotifyReadConsensusTxStatusResult::Status(ConsensusTxStatus::Finalized)
        ));

        let cache_clone = cache.clone();
        let notify_read_task = tokio::spawn(async move {
            cache_clone
                .notify_read_transaction_status_change(tx_pos, Some(ConsensusTxStatus::Finalized))
                .await
        });

        // We should never receive a new status update since the new status is older than the old status.
        cache.set_transaction_status(tx_pos, ConsensusTxStatus::FastpathCertified);
        let result = tokio::time::timeout(Duration::from_secs(3), notify_read_task).await;
        assert!(result.is_err());
        assert_eq!(
            cache.get_transaction_status(&tx_pos),
            Some(ConsensusTxStatus::Finalized)
        );
    }
}
