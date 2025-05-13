// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;
use tokio::time::sleep;
use tracing::debug;

use mysten_common::sync::notify_read::NotifyRead;

use crate::wait_for_effects_request::ConsensusTxPosition;

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Default)]
pub(crate) struct ConsensusTxStatusCache {
    inner: RwLock<Inner>,
    status_notify_read: NotifyRead<ConsensusTxPosition, ConsensusTxStatus>,
    /// The depth of the garbage collection.
    /// We use this to expire positions from old rounds.
    gc_depth: u64,
}

#[derive(Default)]
struct Inner {
    /// A map of transaction position to its status from consensus.
    transaction_status: HashMap<ConsensusTxPosition, ConsensusTxStatus>,
    /// A map of consensus round to all transactions that were updated in that round.
    round_lookup_map: BTreeMap<u64, HashSet<ConsensusTxPosition>>,
    /// The last round that was committed, serving as a watermark for expired transactions.
    last_committed_round: Option<u64>,
}

impl ConsensusTxStatusCache {
    pub fn new(gc_depth: u64) -> Self {
        Self {
            gc_depth,
            ..Default::default()
        }
    }

    pub fn set_transaction_status(
        &self,
        transaction_position: ConsensusTxPosition,
        status: ConsensusTxStatus,
    ) {
        let mut inner = self.inner.write();
        if let Some(last_committed_round) = inner.last_committed_round {
            if transaction_position.block.round as u64 + self.gc_depth < last_committed_round {
                return;
            }
        }
        let old_status = inner
            .transaction_status
            .insert(transaction_position, status.clone());
        if old_status.is_none() {
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
        transaction_position: ConsensusTxPosition,
        old_status: Option<ConsensusTxStatus>,
    ) -> NotifyReadConsensusTxStatusResult {
        let registration = self.status_notify_read.register_one(&transaction_position);
        {
            let inner = self.inner.read();
            if let Some(status) = inner.transaction_status.get(&transaction_position) {
                if Some(status) != old_status.as_ref() {
                    return NotifyReadConsensusTxStatusResult::Status(status.clone());
                }
            }
            // Inner read lock dropped here.
        }

        let expiration_check = async {
            loop {
                {
                    let inner = self.inner.read();
                    if let Some(last_committed_round) = inner.last_committed_round {
                        if transaction_position.block.round as u64 + self.gc_depth
                            < last_committed_round
                        {
                            return last_committed_round;
                        }
                    }
                }
                sleep(Duration::from_millis(50)).await;
            }
        };
        tokio::select! {
            status = registration => NotifyReadConsensusTxStatusResult::Status(status),
            last_committed_round = expiration_check => NotifyReadConsensusTxStatusResult::Expired(last_committed_round),
        }
    }

    pub async fn update_last_committed_round(&self, round: u64) {
        debug!("Updating last committed round: {}", round);
        let mut inner = self.inner.write();
        while let Some(&next_round) = inner.round_lookup_map.keys().next() {
            if next_round + self.gc_depth < round {
                let transactions = inner.round_lookup_map.remove(&next_round).unwrap();
                for tx in transactions {
                    inner.transaction_status.remove(&tx);
                }
            } else {
                break;
            }
        }
        inner.last_committed_round = Some(round);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use consensus_core::BlockRef;
    use sui_types::messages_consensus::TransactionIndex;

    const GC_DEPTH: u64 = 10;

    fn create_test_tx_position(round: u64, index: u64) -> ConsensusTxPosition {
        ConsensusTxPosition {
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
        let cache = ConsensusTxStatusCache::new(GC_DEPTH);
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
        let cache = Arc::new(ConsensusTxStatusCache::new(GC_DEPTH));
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
        let cache = ConsensusTxStatusCache::new(GC_DEPTH);
        let tx_pos = create_test_tx_position(1, 0);

        // Set initial status
        cache.set_transaction_status(tx_pos, ConsensusTxStatus::FastpathCertified);

        // Update last committed round to trigger expiration
        cache.update_last_committed_round(GC_DEPTH + 2).await;

        // Try to read status - should be expired
        let result = cache.notify_read_transaction_status(tx_pos, None).await;
        assert!(matches!(
            result,
            NotifyReadConsensusTxStatusResult::Expired(_)
        ));
    }

    #[tokio::test]
    async fn test_multiple_status_updates() {
        let cache = ConsensusTxStatusCache::new(GC_DEPTH);
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
        let cache = ConsensusTxStatusCache::new(GC_DEPTH);

        // Add transactions for multiple rounds
        for round in 1..=5 {
            let tx_pos = create_test_tx_position(round, 0);
            cache.set_transaction_status(tx_pos, ConsensusTxStatus::FastpathCertified);
        }

        // Update last committed round to expire early rounds
        cache.update_last_committed_round(GC_DEPTH + 3).await;

        // Verify early rounds are cleaned up
        let inner = cache.inner.read();
        assert!(!inner.round_lookup_map.contains_key(&1));
        assert!(!inner.round_lookup_map.contains_key(&2));
        assert!(inner.round_lookup_map.contains_key(&4));
        assert!(inner.round_lookup_map.contains_key(&5));
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        let cache = Arc::new(ConsensusTxStatusCache::new(GC_DEPTH));
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
}
