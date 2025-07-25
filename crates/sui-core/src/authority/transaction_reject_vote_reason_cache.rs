// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use consensus_types::block::{BlockDigest, BlockRef};
use parking_lot::RwLock;
use std::{collections::BTreeMap, u64};
use sui_types::messages_consensus::ConsensusPosition;

/// The number of consensus rounds to retain the reject vote reason information before garbage collection.
/// Assuming a max round rate of 15/sec, this allows status updates to be valid within a window of ~25-30 seconds.
const RETENTION_ROUNDS: u32 = 400;

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
struct TransactionRejectVoteReasonCache {
    cache: RwLock<BTreeMap<ConsensusPosition, SuiError>>,
}

impl TransactionRejectVoteReasonCache {
    pub fn new() -> Self {
        Self {
            cache: Default::default(),
        }
    }

    /// Records a rejection vote reason for a transaction at the specified consensus position. The consensus `position` that
    /// uniquely identifies the transaction and the `reason` (SuiError) that caused the transaction to be rejected during voting
    /// should be provided.
    pub fn set_rejection_vote_reason(&self, position: ConsensusPosition, reason: SuiError) {
        self.cache.write().insert(position, reason);
    }

    /// Returns the rejection vote reason for the transaction at the specified consensus position. The result will be `None` when:
    /// * this node has never casted a reject vote for the transaction in question (either accepted or not processed it).
    /// * the transaction vote reason has been cleaned up due to the retention policy.
    pub fn get_rejection_vote_reason(&self, position: ConsensusPosition) -> Option<SuiError> {
        self.cache.read().get(&position).cloned()
    }

    /// Sets the last committed leader round. This is used to clean up the cache based on the retention policy.
    pub fn set_last_committed_leader_round(&self, round: u64) {
        let cut_off_round = round.saturating_sub(RETENTION_ROUNDS);
        let cut_off_position = ConsensusPosition {
            epoch: u64::MAX,
            block: BlockRef::new(cut_off_round, u32::MAX, BlockDigest::MAX),
            index: u16::MAX,
        };

        let cache = self.cache.write();
        let remaining = cache.split_off(&cut_off_position);
        *cache = remaining;
    }
}
