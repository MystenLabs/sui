// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use consensus_types::block::Round;
use mysten_common::sync::notify_read::NotifyRead;
use parking_lot::{RwLock, RwLockWriteGuard};
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
    // Transaction is voted to accept by a quorum of validators on fastpath.
    FastpathCertified,
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
    // GC depth in consensus.
    consensus_gc_depth: u32,

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
    /// Consensus positions that are currently in the fastpath certified state.
    fastpath_certified: BTreeSet<ConsensusPosition>,
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
            consensus_gc_depth,
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
        self.set_transaction_status_inner(&mut inner, pos, status);
    }

    fn set_transaction_status_inner(
        &self,
        inner: &mut RwLockWriteGuard<Inner>,
        pos: ConsensusPosition,
        status: ConsensusTxStatus,
    ) {
        // Calls to set_transaction_status are async and can be out of order.
        // Makes sure this is tolerated by handling state transitions properly.
        let status_entry = inner.transaction_status.entry(pos);
        match status_entry {
            Entry::Vacant(entry) => {
                // Set the status for the first time.
                entry.insert(status);
                if status == ConsensusTxStatus::FastpathCertified {
                    // Only path where a status can be set to fastpath certified.
                    assert!(inner.fastpath_certified.insert(pos));
                }
            }
            Entry::Occupied(mut entry) => {
                let old_status = *entry.get();
                match (old_status, status) {
                    // If the statuses are the same, no update is needed.
                    (s1, s2) if s1 == s2 => return,
                    // FastpathCertified is transient and can be updated to other statuses.
                    (ConsensusTxStatus::FastpathCertified, _) => {
                        entry.insert(status);
                        if old_status == ConsensusTxStatus::FastpathCertified {
                            // Only path where a status can transition out of fastpath certified.
                            assert!(inner.fastpath_certified.remove(&pos));
                        }
                    }
                    // This happens when statuses arrive out-of-order, and is a no-op.
                    (
                        ConsensusTxStatus::Rejected
                        | ConsensusTxStatus::Dropped
                        | ConsensusTxStatus::Finalized,
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
        debug!("Transaction status is set for {}: {:?}", pos, status);
        self.status_notify_read.notify(&pos, &status);
    }

    /// Given a known previous status provided by `old_status`, this function will return a new
    /// status once the transaction status has changed, or if the consensus position has expired.
    pub(crate) async fn notify_read_transaction_status_change(
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
            if let Some(status) = inner.transaction_status.get(&consensus_position)
                && Some(status) != old_status.as_ref()
            {
                if let Some(old_status) = old_status {
                    // The only scenario where the status may change, is when the transaction
                    // is initially fastpath certified, and then later finalized or rejected.
                    assert_eq!(old_status, ConsensusTxStatus::FastpathCertified);
                }
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
                let (pos, status) = inner.transaction_status.pop_first().unwrap();
                // Ensure the transaction is not in the fastpath certified set.
                if status == ConsensusTxStatus::FastpathCertified {
                    assert!(inner.fastpath_certified.remove(&pos));
                }
            } else {
                break;
            }
        }

        // GC fastpath certified transactions.
        // In theory, notify_read_transaction_status_change() could return `Rejected` status directly
        // to waiters on GC'ed transactions.
        // But it is necessary to track the number of fastpath certified status anyway for end of epoch.
        // So rejecting every fastpath certified transaction here.
        while let Some(position) = inner.fastpath_certified.first().cloned() {
            if position.block.round + self.consensus_gc_depth <= leader_round {
                // Reject GC'ed transactions that were previously fastpath certified.
                self.set_transaction_status_inner(
                    &mut inner,
                    position,
                    ConsensusTxStatus::Rejected,
                );
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

    pub(crate) fn get_num_fastpath_certified(&self) -> usize {
        self.inner.read().fastpath_certified.len()
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

    #[cfg(test)]
    pub(crate) fn get_transaction_status(
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
        let cache = ConsensusTxStatusCache::new(60);
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
        let cache = Arc::new(ConsensusTxStatusCache::new(60));
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
        let cache = ConsensusTxStatusCache::new(60);
        let tx_pos = create_test_tx_position(1, 0);

        // Set initial status
        cache.set_transaction_status(tx_pos, ConsensusTxStatus::FastpathCertified);

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
        let cache = ConsensusTxStatusCache::new(60);
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
        let cache = ConsensusTxStatusCache::new(60);

        // Add transactions for multiple rounds
        for round in 1..=5 {
            let tx_pos = create_test_tx_position(round, 0);
            cache.set_transaction_status(tx_pos, ConsensusTxStatus::FastpathCertified);
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
        let cache = Arc::new(ConsensusTxStatusCache::new(60));
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

    #[tokio::test]
    async fn test_fastpath_certified_tracking() {
        let cache = Arc::new(ConsensusTxStatusCache::new(60));

        // Initially, no fastpath certified transactions
        assert_eq!(cache.get_num_fastpath_certified(), 0);

        // Add fastpath certified transactions
        let tx_pos1 = create_test_tx_position(100, 0);
        let tx_pos2 = create_test_tx_position(100, 1);
        let tx_pos3 = create_test_tx_position(101, 2);
        let tx_pos4 = create_test_tx_position(102, 3);

        cache.set_transaction_status(tx_pos1, ConsensusTxStatus::FastpathCertified);
        assert_eq!(cache.get_num_fastpath_certified(), 1);

        cache.set_transaction_status(tx_pos2, ConsensusTxStatus::FastpathCertified);
        assert_eq!(cache.get_num_fastpath_certified(), 2);

        cache.set_transaction_status(tx_pos3, ConsensusTxStatus::FastpathCertified);
        assert_eq!(cache.get_num_fastpath_certified(), 3);

        cache.set_transaction_status(tx_pos4, ConsensusTxStatus::FastpathCertified);
        assert_eq!(cache.get_num_fastpath_certified(), 4);

        // Add a non-fastpath certified transaction
        let tx_pos5 = create_test_tx_position(103, 4);
        cache.set_transaction_status(tx_pos5, ConsensusTxStatus::Finalized);
        assert_eq!(cache.get_num_fastpath_certified(), 4);

        // Transition one fastpath certified to finalized
        cache.set_transaction_status(tx_pos1, ConsensusTxStatus::Finalized);
        assert_eq!(cache.get_num_fastpath_certified(), 3);
        assert_eq!(
            cache.get_transaction_status(&tx_pos1),
            Some(ConsensusTxStatus::Finalized)
        );

        // Transition another fastpath certified to rejected
        cache.set_transaction_status(tx_pos2, ConsensusTxStatus::Rejected);
        assert_eq!(cache.get_num_fastpath_certified(), 2);
        assert_eq!(
            cache.get_transaction_status(&tx_pos2),
            Some(ConsensusTxStatus::Rejected)
        );

        // Test GC of fastpath certified transactions
        // tx_pos3 is at round 101, with gc_depth=60, it will be GC'd when prev leader round >= 161
        // tx_pos4 is at round 102, with gc_depth=60, it will be GC'd when prev leader round >= 162

        // Set initial leader round which doesn't GC anything.
        cache.update_last_committed_leader_round(160).await;
        assert_eq!(cache.get_num_fastpath_certified(), 2);
        assert_eq!(
            cache.get_transaction_status(&tx_pos3),
            Some(ConsensusTxStatus::FastpathCertified)
        );
        assert_eq!(
            cache.get_transaction_status(&tx_pos4),
            Some(ConsensusTxStatus::FastpathCertified)
        );

        // Update to 161: uses 160 for GC
        // tx_pos3: 101 + 60 = 161, 161 <= 160 is false, so NOT GC'd yet
        cache.update_last_committed_leader_round(161).await;
        assert_eq!(cache.get_num_fastpath_certified(), 2);
        assert_eq!(
            cache.get_transaction_status(&tx_pos3),
            Some(ConsensusTxStatus::FastpathCertified)
        );
        assert_eq!(
            cache.get_transaction_status(&tx_pos4),
            Some(ConsensusTxStatus::FastpathCertified)
        );

        // Update to 162: uses 161 for GC
        // tx_pos3: 101 + 60 = 161, 161 <= 161 is true, so GC'd
        // tx_pos4: 102 + 60 = 162, 162 <= 161 is false, so NOT GC'd
        cache.update_last_committed_leader_round(162).await;
        assert_eq!(cache.get_num_fastpath_certified(), 1);
        assert_eq!(
            cache.get_transaction_status(&tx_pos3),
            Some(ConsensusTxStatus::Rejected)
        );
        assert_eq!(
            cache.get_transaction_status(&tx_pos4),
            Some(ConsensusTxStatus::FastpathCertified)
        );

        // Update to 163: uses 162 for GC
        // tx_pos4: 102 + 60 = 162, 162 <= 162 is true, so GC'd
        cache.update_last_committed_leader_round(163).await;
        assert_eq!(cache.get_num_fastpath_certified(), 0);
        assert_eq!(
            cache.get_transaction_status(&tx_pos4),
            Some(ConsensusTxStatus::Rejected)
        );

        // Test that setting a transaction directly to non-fastpath doesn't affect count
        let tx_pos6 = create_test_tx_position(200, 5);
        cache.set_transaction_status(tx_pos6, ConsensusTxStatus::Finalized);
        assert_eq!(cache.get_num_fastpath_certified(), 0);

        // Can't transition from finalized back to fastpath certified
        cache.set_transaction_status(tx_pos6, ConsensusTxStatus::FastpathCertified);
        assert_eq!(cache.get_num_fastpath_certified(), 0);
        assert_eq!(
            cache.get_transaction_status(&tx_pos6),
            Some(ConsensusTxStatus::Finalized)
        );
    }
}
