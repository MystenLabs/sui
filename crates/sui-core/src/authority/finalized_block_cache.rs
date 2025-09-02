// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use consensus_types::block::Round;
use mysten_common::sync::notify_read::NotifyRead;
use parking_lot::RwLock;
use sui_types::messages_consensus::ConsensusPosition;
use tokio::sync::watch;
use tracing::debug;

#[cfg(test)]
use sui_types::error::{SuiError, SuiResult};

/// The number of consensus rounds to retain finalized block information before garbage collection.
/// Used to expire positions from old rounds, as well as to check if a block is too far ahead of the last committed round.
/// Assuming a max round rate of 15/sec, this allows status updates to be valid within a window of ~25-30 seconds.
pub(crate) const FINALIZED_BLOCK_RETENTION_ROUNDS: u32 = 400;

#[derive(Debug, Clone)]
pub(crate) enum NotifyReadFinalizedBlockResult {
    /// The block at the consensus position has been finalized.
    Finalized,
    /// The consensus position has expired.
    /// Provided with the last committed round that was used to check for expiration.
    Expired(u32),
}

pub(crate) struct FinalizedBlockCache {
    inner: RwLock<Inner>,

    finalized_notify_read: NotifyRead<ConsensusPosition, ()>,
    /// Watch channel for last committed leader round updates
    last_committed_leader_round_tx: watch::Sender<Option<u32>>,
    last_committed_leader_round_rx: watch::Receiver<Option<u32>>,
}

#[derive(Default)]
struct Inner {
    /// A map of consensus positions to finalized blocks.
    /// The presence of a key indicates the block has been finalized.
    finalized_blocks: BTreeMap<ConsensusPosition, ()>,
    /// The last leader round updated in update_last_committed_leader_round().
    last_committed_leader_round: Option<Round>,
}

impl FinalizedBlockCache {
    pub(crate) fn new() -> Self {
        let (last_committed_leader_round_tx, last_committed_leader_round_rx) = watch::channel(None);
        Self {
            inner: Default::default(),
            finalized_notify_read: Default::default(),
            last_committed_leader_round_tx,
            last_committed_leader_round_rx,
        }
    }

    /// Marks a block at the given consensus position as finalized.
    /// If the position is too old (beyond retention window), the update is ignored.
    pub(crate) fn mark_block_finalized(&self, pos: ConsensusPosition) {
        if let Some(last_committed_leader_round) = *self.last_committed_leader_round_rx.borrow() {
            if pos.block.round + FINALIZED_BLOCK_RETENTION_ROUNDS <= last_committed_leader_round {
                // Ignore stale finalization updates.
                return;
            }
        }

        let mut inner = self.inner.write();

        // Only insert if not already present to avoid redundant work
        if inner.finalized_blocks.insert(pos, ()).is_none() {
            debug!("Block finalized at position: {}", pos);
            self.finalized_notify_read.notify(&pos, &());
        }
    }

    /// Checks if a block at the given consensus position is finalized.
    #[cfg(test)]
    pub(crate) fn is_block_finalized(&self, pos: &ConsensusPosition) -> bool {
        let inner = self.inner.read();
        inner.finalized_blocks.contains_key(pos)
    }

    /// Waits until a block at the given consensus position becomes finalized,
    /// or until the position expires due to being too old.
    pub(crate) async fn wait_for_block_finalized(
        &self,
        consensus_position: ConsensusPosition,
    ) -> NotifyReadFinalizedBlockResult {
        let registration = self.finalized_notify_read.register_one(&consensus_position);
        let mut round_rx = self.last_committed_leader_round_rx.clone();

        {
            let inner = self.inner.read();
            if inner.finalized_blocks.contains_key(&consensus_position) {
                return NotifyReadFinalizedBlockResult::Finalized;
            }
            // Inner read lock dropped here.
        }

        let expiration_check = async {
            loop {
                if let Some(last_committed_leader_round) = *round_rx.borrow() {
                    if consensus_position.block.round + FINALIZED_BLOCK_RETENTION_ROUNDS
                        <= last_committed_leader_round
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
            _ = registration => NotifyReadFinalizedBlockResult::Finalized,
            last_committed_leader_round = expiration_check => NotifyReadFinalizedBlockResult::Expired(last_committed_leader_round),
        }
    }

    /// Updates the last committed leader round and performs garbage collection
    /// of expired finalized block entries.
    pub(crate) async fn update_last_committed_leader_round(
        &self,
        last_committed_leader_round: u32,
    ) {
        debug!(
            "Updating last committed leader round: {}",
            last_committed_leader_round
        );

        let mut inner = self.inner.write();

        // Similar to consensus_tx_status_cache, we use the previous committed leader round for GC
        // to avoid expiring blocks from the current commit.
        let Some(leader_round) = inner
            .last_committed_leader_round
            .replace(last_committed_leader_round)
        else {
            // This is the first update. Do not expire or GC any blocks.
            return;
        };

        // Remove finalized blocks that are expired.
        while let Some((position, _)) = inner.finalized_blocks.first_key_value() {
            if position.block.round + FINALIZED_BLOCK_RETENTION_ROUNDS <= leader_round {
                inner.finalized_blocks.pop_first();
            } else {
                break;
            }
        }

        // Send update through watch channel.
        let _ = self.last_committed_leader_round_tx.send(Some(leader_round));
    }

    /// Returns the number of finalized blocks currently cached.
    #[cfg(test)]
    pub(crate) fn get_num_finalized_blocks(&self) -> usize {
        self.inner.read().finalized_blocks.len()
    }

    /// Returns true if the position is too far ahead of the last committed round.
    #[cfg(test)]
    pub(crate) fn check_position_too_ahead(&self, position: &ConsensusPosition) -> SuiResult<()> {
        if let Some(last_committed_leader_round) = *self.last_committed_leader_round_rx.borrow() {
            if position.block.round > last_committed_leader_round + FINALIZED_BLOCK_RETENTION_ROUNDS
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
    pub(crate) fn contains_finalized_block(&self, position: &ConsensusPosition) -> bool {
        let inner = self.inner.read();
        inner.finalized_blocks.contains_key(position)
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use super::*;
    use consensus_types::block::{BlockRef, TransactionIndex};

    fn create_test_consensus_position(round: u64, index: u64) -> ConsensusPosition {
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
    async fn test_mark_and_check_finalized() {
        let cache = FinalizedBlockCache::new();
        let pos = create_test_consensus_position(1, 0);

        // Initially not finalized
        assert!(!cache.is_block_finalized(&pos));

        // Mark as finalized
        cache.mark_block_finalized(pos);

        // Should now be finalized
        assert!(cache.is_block_finalized(&pos));
    }

    #[tokio::test]
    async fn test_wait_for_finalized_immediate() {
        let cache = FinalizedBlockCache::new();
        let pos = create_test_consensus_position(1, 0);

        // Mark as finalized first
        cache.mark_block_finalized(pos);

        // Should return immediately
        let result = cache.wait_for_block_finalized(pos).await;
        assert!(matches!(result, NotifyReadFinalizedBlockResult::Finalized));
    }

    #[tokio::test]
    async fn test_wait_for_finalized_notification() {
        let cache = Arc::new(FinalizedBlockCache::new());
        let pos = create_test_consensus_position(1, 0);

        // Spawn a task that waits for finalization
        let cache_clone = cache.clone();
        let handle = tokio::spawn(async move { cache_clone.wait_for_block_finalized(pos).await });

        // Small delay to ensure the task is waiting
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Mark as finalized
        cache.mark_block_finalized(pos);

        // Verify the notification was received
        let result = handle.await.unwrap();
        assert!(matches!(result, NotifyReadFinalizedBlockResult::Finalized));
    }

    #[tokio::test]
    async fn test_expiration() {
        let cache = FinalizedBlockCache::new();
        let pos = create_test_consensus_position(1, 0);

        // Mark as finalized
        cache.mark_block_finalized(pos);

        // Set initial leader round which doesn't GC anything.
        cache
            .update_last_committed_leader_round(FINALIZED_BLOCK_RETENTION_ROUNDS + 1)
            .await;

        // Update with round that will trigger GC using previous round (FINALIZED_BLOCK_RETENTION_ROUNDS + 1)
        // This will expire blocks up to and including round 1
        cache
            .update_last_committed_leader_round(FINALIZED_BLOCK_RETENTION_ROUNDS + 2)
            .await;

        // Try to wait for finalization - should be expired
        let result = cache.wait_for_block_finalized(pos).await;
        assert!(matches!(result, NotifyReadFinalizedBlockResult::Expired(_)));
    }

    #[tokio::test]
    async fn test_cleanup_expired_rounds() {
        let cache = FinalizedBlockCache::new();

        // Add finalized blocks for multiple rounds
        for round in 1..=5 {
            let pos = create_test_consensus_position(round, 0);
            cache.mark_block_finalized(pos);
        }

        assert_eq!(cache.get_num_finalized_blocks(), 5);

        // Set initial leader round which doesn't GC anything.
        cache
            .update_last_committed_leader_round(FINALIZED_BLOCK_RETENTION_ROUNDS + 2)
            .await;

        // No rounds should be cleaned up yet since this was the initial update
        assert_eq!(cache.get_num_finalized_blocks(), 5);

        // Update that triggers GC using previous round (FINALIZED_BLOCK_RETENTION_ROUNDS + 2)
        // This will expire blocks up to and including round 2
        cache
            .update_last_committed_leader_round(FINALIZED_BLOCK_RETENTION_ROUNDS + 3)
            .await;

        // Verify rounds 1-2 are cleaned up, 3-5 remain
        assert_eq!(cache.get_num_finalized_blocks(), 3);

        // Verify specific blocks
        assert!(!cache.contains_finalized_block(&create_test_consensus_position(1, 0)));
        assert!(!cache.contains_finalized_block(&create_test_consensus_position(2, 0)));
        assert!(cache.contains_finalized_block(&create_test_consensus_position(3, 0)));
        assert!(cache.contains_finalized_block(&create_test_consensus_position(4, 0)));
        assert!(cache.contains_finalized_block(&create_test_consensus_position(5, 0)));
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        let cache = Arc::new(FinalizedBlockCache::new());
        let pos = create_test_consensus_position(1, 0);

        // Spawn multiple tasks that wait for finalization
        let mut handles = vec![];
        for _ in 0..3 {
            let cache_clone = cache.clone();
            handles.push(tokio::spawn(async move {
                cache_clone.wait_for_block_finalized(pos).await
            }));
        }

        // Small delay to ensure tasks are waiting
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Mark as finalized
        cache.mark_block_finalized(pos);

        // Verify all notifications were received
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(matches!(result, NotifyReadFinalizedBlockResult::Finalized));
        }
    }

    #[tokio::test]
    async fn test_duplicate_finalization() {
        let cache = FinalizedBlockCache::new();
        let pos = create_test_consensus_position(1, 0);

        // Mark as finalized multiple times
        cache.mark_block_finalized(pos);
        cache.mark_block_finalized(pos);
        cache.mark_block_finalized(pos);

        // Should still be finalized and count should be 1
        assert!(cache.is_block_finalized(&pos));
        assert_eq!(cache.get_num_finalized_blocks(), 1);
    }

    #[tokio::test]
    async fn test_stale_finalization_ignored() {
        let cache = FinalizedBlockCache::new();
        let old_pos = create_test_consensus_position(1, 0);

        // Set a high committed round first
        cache
            .update_last_committed_leader_round(FINALIZED_BLOCK_RETENTION_ROUNDS + 10)
            .await;

        // Try to mark an old position as finalized - should be ignored
        cache.mark_block_finalized(old_pos);

        // Should not be finalized
        assert!(!cache.is_block_finalized(&old_pos));
        assert_eq!(cache.get_num_finalized_blocks(), 0);
    }

    #[tokio::test]
    async fn test_position_too_ahead() {
        let cache = FinalizedBlockCache::new();

        // Set committed round
        cache.update_last_committed_leader_round(100).await;

        // Position that's too far ahead
        let ahead_pos =
            create_test_consensus_position((100 + FINALIZED_BLOCK_RETENTION_ROUNDS + 1) as u64, 0);

        // Should return error
        assert!(cache.check_position_too_ahead(&ahead_pos).is_err());

        // Position within range should be ok
        let ok_pos =
            create_test_consensus_position((100 + FINALIZED_BLOCK_RETENTION_ROUNDS) as u64, 0);
        assert!(cache.check_position_too_ahead(&ok_pos).is_ok());
    }
}
