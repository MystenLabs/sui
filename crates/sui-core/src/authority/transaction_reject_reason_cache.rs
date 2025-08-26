// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use consensus_config::AuthorityIndex;
use consensus_types::block::{BlockDigest, BlockRef, TransactionIndex};
use mysten_metrics::monitored_scope;
use parking_lot::RwLock;
use sui_types::committee::EpochId;
use sui_types::error::SuiError;
use sui_types::messages_consensus::ConsensusPosition;
use tracing::trace;

use crate::authority::consensus_tx_status_cache::CONSENSUS_STATUS_RETENTION_ROUNDS;

#[cfg(test)]
use consensus_types::block::Round;

/// A cache that maintains rejection reasons (SuiError) when validators cast reject votes for transactions
/// during the Mysticeti consensus fast path voting process.
///
/// This cache serves as a bridge between the consensus voting mechanism and client-facing APIs,
/// allowing detailed error information to be returned when querying transaction status.
///
/// ## Key Characteristics:
/// - **Mysticeti Fast Path Only**: Only populated when transactions are voted on via the mysticeti
///   fast path, as it relies on consensus position (epoch, block, index) to uniquely identify transactions
/// - **Pre-consensus Rejections**: Direct rejections during transaction submission (before consensus
///   propagation) are not cached since these transactions never enter the consensus pipeline
/// - **Automatic Cleanup**: Maintains a retention period based on the last committed leader round
///   and automatically purges older entries to prevent unbounded memory growth
///
/// ## Use Cases:
/// - Providing detailed rejection reasons to clients querying transaction status
/// - Debugging transaction failures in the fast path voting process
pub(crate) struct TransactionRejectReasonCache {
    cache: RwLock<BTreeMap<ConsensusPosition, SuiError>>,
    retention_rounds: u32,
    epoch: EpochId,
}

impl TransactionRejectReasonCache {
    pub fn new(retention_rounds: Option<u32>, epoch: EpochId) -> Self {
        Self {
            cache: Default::default(),
            retention_rounds: retention_rounds.unwrap_or(CONSENSUS_STATUS_RETENTION_ROUNDS),
            epoch,
        }
    }

    /// Records a rejection vote reason for a transaction at the specified consensus position. The consensus `position` that
    /// uniquely identifies the transaction and the `reason` (SuiError) that caused the transaction to be rejected during voting
    /// should be provided.
    pub fn set_rejection_vote_reason(&self, position: ConsensusPosition, reason: &SuiError) {
        debug_assert_eq!(position.epoch, self.epoch, "Epoch mismatch");
        self.cache.write().insert(position, reason.clone());
    }

    /// Returns the rejection vote reason for the transaction at the specified consensus position. The result will be `None` when:
    /// * this node has never casted a reject vote for the transaction in question (either accepted or not processed it).
    /// * the transaction vote reason has been cleaned up due to the retention policy.
    pub fn get_rejection_vote_reason(&self, position: ConsensusPosition) -> Option<SuiError> {
        debug_assert_eq!(position.epoch, self.epoch, "Epoch mismatch");
        self.cache.read().get(&position).cloned()
    }

    /// Sets the last committed leader round. This is used to clean up the cache based on the retention policy.
    pub fn set_last_committed_leader_round(&self, round: u32) {
        let _scope =
            monitored_scope("TransactionRejectReasonCache::set_last_committed_leader_round");
        let cut_off_round = round.saturating_sub(self.retention_rounds) + 1;
        let cut_off_position = ConsensusPosition {
            epoch: self.epoch,
            block: BlockRef::new(cut_off_round, AuthorityIndex::MIN, BlockDigest::MIN),
            index: TransactionIndex::MIN,
        };

        let mut cache = self.cache.write();
        let remaining = cache.split_off(&cut_off_position);
        trace!("Cleaned up {} entries", cache.len());
        *cache = remaining;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_set_rejection_vote_reason_and_get_reason() {
        let cache = TransactionRejectReasonCache::new(None, 1);
        let position = ConsensusPosition {
            epoch: 1,
            block: BlockRef::new(1, AuthorityIndex::MAX, BlockDigest::MAX),
            index: 1,
        };

        // Set the reject reason for the position once
        {
            let reason = SuiError::ValidatorHaltedAtEpochEnd;
            cache.set_rejection_vote_reason(position, &reason);
            assert_eq!(cache.get_rejection_vote_reason(position), Some(reason));
        }

        // Set the reject reason for the position again will overwrite the previous reason
        {
            let reason = SuiError::InvalidTransactionDigest;
            cache.set_rejection_vote_reason(position, &reason);
            assert_eq!(cache.get_rejection_vote_reason(position), Some(reason));
        }

        // Get the reject reason for a non existing position will return None
        {
            let position = ConsensusPosition {
                epoch: 1,
                block: BlockRef::new(1, AuthorityIndex::MAX, BlockDigest::MIN),
                index: 2,
            };
            assert_eq!(cache.get_rejection_vote_reason(position), None);
        }
    }

    #[tokio::test]
    async fn test_set_last_committed_leader_round() {
        const RETENTION_ROUNDS: u32 = 4;
        const TOTAL_ROUNDS: u32 = 10;
        let cache = TransactionRejectReasonCache::new(Some(RETENTION_ROUNDS), 1);

        let position = |round: Round, transaction_index: u16| ConsensusPosition {
            epoch: 1,
            block: BlockRef::new(
                round,
                AuthorityIndex::new_for_test(transaction_index as u32),
                BlockDigest::MAX,
            ),
            index: transaction_index,
        };

        // Set a few reject reasons for different positions before and after the last committed leader round (6)
        for round in 0..TOTAL_ROUNDS {
            for transaction_index in 0..5 {
                cache.set_rejection_vote_reason(
                    position(round, transaction_index),
                    &SuiError::InvalidTransactionDigest,
                );
            }
        }

        // Set the last committed leader round to 6, which should clean up the cache up to round (including) 6-4 = 2.
        cache.set_last_committed_leader_round(6);

        // The reject reasons from rounds 0-2 should be cleaned up
        for round in 0..TOTAL_ROUNDS {
            for transaction_index in 0..5 {
                let position = position(round, transaction_index);
                if round <= 2 {
                    assert_eq!(cache.get_rejection_vote_reason(position), None);
                } else {
                    assert_eq!(
                        cache.get_rejection_vote_reason(position),
                        Some(SuiError::InvalidTransactionDigest)
                    );
                }
            }
        }

        // Now set the last committed leader round to 10, which should clean up the cache up to round (including) 10-4 = 6.
        cache.set_last_committed_leader_round(10);

        // The reject reasons from rounds 0-6 should be cleaned up
        for round in 0..TOTAL_ROUNDS {
            for transaction_index in 0..5 {
                let position = position(round, transaction_index);
                if round <= 6 {
                    assert_eq!(cache.get_rejection_vote_reason(position), None);
                } else {
                    assert_eq!(
                        cache.get_rejection_vote_reason(position),
                        Some(SuiError::InvalidTransactionDigest)
                    );
                }
            }
        }
    }
}
