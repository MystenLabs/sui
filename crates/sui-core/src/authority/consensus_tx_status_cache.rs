// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use consensus_types::block::Round;
use mysten_common::sync::notify_read::NotifyRead;
use parking_lot::RwLock;
use sui_types::{
    error::{SuiErrorKind, SuiResult},
    messages_consensus::ConsensusPosition,
};
use tokio::sync::watch;
use tracing::debug;

/// The number of consensus rounds to retain transaction status information before garbage collection.
/// Used to expire positions from old rounds, as well as to check if a transaction is too far ahead of the last committed round.
/// Assuming a max round rate of 15/sec, this allows status updates to be valid within a window of ~25-30 seconds.
pub(crate) const CONSENSUS_STATUS_RETENTION_ROUNDS: u32 = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConsensusTxStatus {
    // Transaction is rejected, either by a quorum of validators or indirectly post-commit.
    Rejected,
    // Transaction is finalized post commit.
    Finalized,
    // Transaction is dropped post-consensus.
    // This decision must be consistent across all validators.
    //
    // Currently, only invalid owned object inputs (using stale versions)
    // can cause a transaction to be dropped without execution.
    Dropped,
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
    /// The last leader round updated in update_last_committed_leader_round().
    last_committed_leader_round: Option<Round>,
}

impl ConsensusTxStatusCache {
    pub(crate) fn new(consensus_gc_depth: Round) -> Self {
        assert!(
            consensus_gc_depth < CONSENSUS_STATUS_RETENTION_ROUNDS,
            "{} vs {}",
            consensus_gc_depth,
            CONSENSUS_STATUS_RETENTION_ROUNDS
        );
        let (last_committed_leader_round_tx, last_committed_leader_round_rx) = watch::channel(None);
        Self {
            inner: Default::default(),
            status_notify_read: Default::default(),
            last_committed_leader_round_tx,
            last_committed_leader_round_rx,
        }
    }

    pub(crate) fn set_transaction_status(&self, pos: ConsensusPosition, status: ConsensusTxStatus) {
        if let Some(last_committed_leader_round) = *self.last_committed_leader_round_rx.borrow()
            && pos.block.round + CONSENSUS_STATUS_RETENTION_ROUNDS <= last_committed_leader_round
        {
            // Ignore stale status updates.
            return;
        }

        let mut inner = self.inner.write();
        let old_status = inner.transaction_status.insert(pos, status);
        if let Some(old_status) = old_status {
            panic!(
                "Conflicting status updates for transaction {:?}: {:?} -> {:?}",
                pos, old_status, status
            );
        }

        // All code paths leading to here should have set the status.
        debug!("Transaction status is set for {}: {:?}", pos, status);
        self.status_notify_read.notify(&pos, &status);
    }

    /// Given a known previous status provided by `old_status`, this function will return a new
    /// status once the transaction status has changed, or if the consensus position has expired.
    pub(crate) async fn notify_read_transaction_status(
        &self,
        consensus_position: ConsensusPosition,
    ) -> NotifyReadConsensusTxStatusResult {
        let registration = self.status_notify_read.register_one(&consensus_position);
        let mut round_rx = self.last_committed_leader_round_rx.clone();
        {
            let inner = self.inner.read();
            if let Some(status) = inner.transaction_status.get(&consensus_position) {
                return NotifyReadConsensusTxStatusResult::Status(*status);
            }
            // Inner read lock dropped here.
        }
        let expiration_check = async {
            loop {
                if let Some(last_committed_leader_round) = *round_rx.borrow()
                    && consensus_position.block.round + CONSENSUS_STATUS_RETENTION_ROUNDS
                        <= last_committed_leader_round
                {
                    return last_committed_leader_round;
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

    pub(crate) async fn update_last_committed_leader_round(
        &self,
        last_committed_leader_round: u32,
    ) {
        debug!(
            "Updating last committed leader round: {}",
            last_committed_leader_round
        );

        let mut inner = self.inner.write();

        // Consensus only bumps GC round after generating a commit. So if we expire and GC transactions
        // based on the latest committed leader round, we may expire transactions in the current commit, or
        // make these transactions' statuses very short lived.
        // So we only expire and GC transactions with the previous committed leader round.
        let Some(leader_round) = inner
            .last_committed_leader_round
            .replace(last_committed_leader_round)
        else {
            // This is the first update. Do not expire or GC any transactions.
            return;
        };

        // Remove transactions that are expired.
        while let Some((position, _)) = inner.transaction_status.first_key_value() {
            if position.block.round + CONSENSUS_STATUS_RETENTION_ROUNDS <= leader_round {
                inner.transaction_status.pop_first();
            } else {
                break;
            }
        }

        // Send update through watch channel.
        let _ = self.last_committed_leader_round_tx.send(Some(leader_round));
    }

    pub(crate) fn get_last_committed_leader_round(&self) -> Option<u32> {
        *self.last_committed_leader_round_rx.borrow()
    }

    /// Returns true if the position is too far ahead of the last committed round.
    pub(crate) fn check_position_too_ahead(&self, position: &ConsensusPosition) -> SuiResult<()> {
        if let Some(last_committed_leader_round) = *self.last_committed_leader_round_rx.borrow()
            && position.block.round
                > last_committed_leader_round + CONSENSUS_STATUS_RETENTION_ROUNDS
        {
            return Err(SuiErrorKind::ValidatorConsensusLagging {
                round: position.block.round,
                last_committed_round: last_committed_leader_round,
            }
            .into());
        }
        Ok(())
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
        let cache = ConsensusTxStatusCache::new(60);
        let tx_pos = create_test_tx_position(1, 0);

        // Set initial status
        cache.set_transaction_status(tx_pos, ConsensusTxStatus::Finalized);

        // Read status immediately
        let result = cache.notify_read_transaction_status(tx_pos).await;
        assert!(matches!(
            result,
            NotifyReadConsensusTxStatusResult::Status(ConsensusTxStatus::Finalized)
        ));
    }

    #[tokio::test]
    async fn test_status_notification() {
        let cache = Arc::new(ConsensusTxStatusCache::new(60));
        let tx_pos = create_test_tx_position(1, 0);

        // Spawn a task that waits for status update
        let cache_clone = cache.clone();
        let handle =
            tokio::spawn(async move { cache_clone.notify_read_transaction_status(tx_pos).await });

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
        let cache = ConsensusTxStatusCache::new(60);
        let tx_pos = create_test_tx_position(1, 0);

        // Set initial status
        cache.set_transaction_status(tx_pos, ConsensusTxStatus::Finalized);

        // Set initial leader round which doesn't GC anything.
        cache
            .update_last_committed_leader_round(CONSENSUS_STATUS_RETENTION_ROUNDS + 1)
            .await;

        // Update with round that will trigger GC using previous round (CONSENSUS_STATUS_RETENTION_ROUNDS + 1)
        // This will expire transactions up to and including round 1
        cache
            .update_last_committed_leader_round(CONSENSUS_STATUS_RETENTION_ROUNDS + 2)
            .await;

        // Try to read status - should be expired
        let result = cache.notify_read_transaction_status(tx_pos).await;
        assert!(matches!(
            result,
            NotifyReadConsensusTxStatusResult::Expired(_)
        ));
    }

    #[tokio::test]
    async fn test_cleanup_expired_rounds() {
        let cache = ConsensusTxStatusCache::new(60);

        // Add transactions for multiple rounds
        for round in 1..=5 {
            let tx_pos = create_test_tx_position(round, 0);
            cache.set_transaction_status(tx_pos, ConsensusTxStatus::Rejected);
        }

        // Set initial leader round which doesn't GC anything.
        cache
            .update_last_committed_leader_round(CONSENSUS_STATUS_RETENTION_ROUNDS + 2)
            .await;

        // No rounds should be cleaned up yet since this was the initial update
        {
            let inner = cache.inner.read();
            let rounds = inner
                .transaction_status
                .keys()
                .map(|p| p.block.round)
                .collect::<Vec<_>>();
            assert_eq!(rounds, vec![1, 2, 3, 4, 5]);
        }

        // Update that triggers GC using previous round (CONSENSUS_STATUS_RETENTION_ROUNDS + 2)
        // This will expire transactions up to and including round 2
        cache
            .update_last_committed_leader_round(CONSENSUS_STATUS_RETENTION_ROUNDS + 3)
            .await;

        // Verify rounds 1-2 are cleaned up, 3-5 remain
        {
            let inner = cache.inner.read();
            let rounds = inner
                .transaction_status
                .keys()
                .map(|p| p.block.round)
                .collect::<Vec<_>>();
            assert_eq!(rounds, vec![3, 4, 5]);
        }

        // Another update using previous round (CONSENSUS_STATUS_RETENTION_ROUNDS + 3) for GC
        // This will expire transactions up to and including round 3
        cache
            .update_last_committed_leader_round(CONSENSUS_STATUS_RETENTION_ROUNDS + 4)
            .await;

        // Verify rounds 1-3 are cleaned up, 4-5 remain
        {
            let inner = cache.inner.read();
            let rounds = inner
                .transaction_status
                .keys()
                .map(|p| p.block.round)
                .collect::<Vec<_>>();
            assert_eq!(rounds, vec![4, 5]);
        }
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        let cache = Arc::new(ConsensusTxStatusCache::new(60));
        let tx_pos = create_test_tx_position(1, 0);

        // Spawn multiple tasks that wait for status
        let mut handles = vec![];
        for _ in 0..3 {
            let cache_clone = cache.clone();
            handles.push(tokio::spawn(async move {
                cache_clone.notify_read_transaction_status(tx_pos).await
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
    #[should_panic(expected = "Conflicting status updates")]
    async fn test_out_of_order_status_updates() {
        let cache = Arc::new(ConsensusTxStatusCache::new(60));
        let tx_pos = create_test_tx_position(1, 0);

        // First update status to Finalized.
        cache.set_transaction_status(tx_pos, ConsensusTxStatus::Finalized);

        // This should cause a panic.
        cache.set_transaction_status(tx_pos, ConsensusTxStatus::Rejected);
    }
}
