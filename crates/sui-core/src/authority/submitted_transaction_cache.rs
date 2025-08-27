// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use consensus_types::block::Round;
use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::IpAddr;
use sui_types::digests::TransactionDigest;
use sui_types::traffic_control::Weight;
use tracing::debug;

/// The number of consensus rounds to retain transaction submission information before garbage collection.
pub(crate) const SUBMISSION_RETENTION_ROUNDS: u32 = 400;

/// TODO(fastpath): Evaluate if this should be a configurable parameter.
/// The allowed number of retries allowed regardless of gas price.
pub(crate) const DEFAULT_RETRY_TOLERANCE: u32 = 20;

/// Cache for tracking submitted transactions to prevent DoS through excessive resubmissions.
/// Tracks submission counts and enforces gas-price-based amplification + retry tolerance limits.
pub(crate) struct SubmittedTransactionCache {
    inner: RwLock<Inner>,
    retention_rounds: u32,
    retry_tolerance: u32,
}

#[derive(Default)]
struct Inner {
    /// Map of transaction digest to submission metadata
    transactions: HashMap<TransactionDigest, SubmissionMetadata>,
    /// Transactions indexed by submitted round for efficient GC
    transactions_by_round: BTreeMap<Round, HashSet<TransactionDigest>>,
    /// The last committed leader round for GC purposes
    last_committed_round: Option<Round>,
}

#[derive(Debug, Clone)]
struct SubmissionMetadata {
    /// Round when transaction was last seen (for GC)
    submitted_round: Round,
    /// Number of times this transaction has been submitted
    submission_count: u32,
    /// Maximum allowed submissions based on gas price amplification + retry tolerance
    max_allowed_submissions: u32,
    /// Client IP address that submitted this transaction
    submitter_client_addr: Option<IpAddr>,
}

impl SubmittedTransactionCache {
    pub(crate) fn new(retention_rounds: Option<u32>, retry_tolerance: Option<u32>) -> Self {
        let retention_rounds = retention_rounds.unwrap_or(SUBMISSION_RETENTION_ROUNDS);

        Self {
            inner: Default::default(),
            retention_rounds,
            retry_tolerance: retry_tolerance.unwrap_or(DEFAULT_RETRY_TOLERANCE),
        }
    }

    pub(crate) fn record_submitted_tx(
        &self,
        digest: &TransactionDigest,
        submitted_round: Round,
        amplification_factor: u32,
        submitter_client_addr: Option<IpAddr>,
    ) {
        let mut inner = self.inner.write();

        // We allow 1 submission for the initial submission, and then the retry tolerance + amplification factor
        // for subsequent submissions.
        let max_allowed_submissions = 1 + self.retry_tolerance + amplification_factor;

        if let Some(metadata) = inner.transactions.get(digest) {
            if metadata.submitted_round < submitted_round {
                let old_round = metadata.submitted_round;
                debug!(
                    "Transaction {digest} re-submitted at round {submitted_round} (previously at round {old_round})",
                );

                if let Some(txns) = inner.transactions_by_round.get_mut(&old_round) {
                    txns.remove(digest);
                    if txns.is_empty() {
                        inner.transactions_by_round.remove(&old_round);
                    }
                }

                inner
                    .transactions_by_round
                    .entry(submitted_round)
                    .or_default()
                    .insert(*digest);

                let metadata = inner.transactions.get_mut(digest).unwrap();
                metadata.submitted_round = submitted_round;
            }
        } else {
            // First time we're submitting this transaction, however we will wait till
            // we see the transaction in consensus output to increment the submission count.
            let metadata = SubmissionMetadata {
                submitted_round,
                submission_count: 0,
                max_allowed_submissions,
                submitter_client_addr,
            };

            inner.transactions.insert(*digest, metadata);
            inner
                .transactions_by_round
                .entry(submitted_round)
                .or_default()
                .insert(*digest);

            debug!(
                "First submission of transaction {digest} at round {submitted_round} (max_allowed: {max_allowed_submissions})",
            );
        }
    }

    /// Increments the submission count when we see a transaction in consensus output.
    /// This tracks how many times the transaction has appeared in consensus (from any validator).
    /// Returns the spam weight and submitter client address if the transaction exceeds allowed submissions.
    pub(crate) fn increment_submission_count(
        &self,
        digest: &TransactionDigest,
    ) -> Option<(Weight, Option<IpAddr>)> {
        let mut inner = self.inner.write();

        if let Some(metadata) = inner.transactions.get_mut(digest) {
            metadata.submission_count += 1;

            if metadata.submission_count > metadata.max_allowed_submissions {
                // TODO(fastpath): Reevaluate spam weight calculation. For simplicity, we use a fixed spam weight of 1 for now.
                let spam_weight = Weight::one();

                debug!(
                    "Transaction {} seen in consensus {} times, exceeds limit {} (spam_weight: {:?})",
                    digest,
                    metadata.submission_count,
                    metadata.max_allowed_submissions,
                    spam_weight
                );

                return Some((spam_weight, metadata.submitter_client_addr));
            }
        }
        // If we don't know about this transaction, it was submitted by another validator
        // We don't track spam weight for transactions we didn't submit
        None
    }

    /// Update the last committed leader round and clean up old entries.
    pub(crate) fn update_last_committed_round(&self, last_committed_leader_round: Round) {
        debug!("Updating last committed leader round: {last_committed_leader_round}");

        let mut inner = self.inner.write();

        let Some(previous_round) = inner
            .last_committed_round
            .replace(last_committed_leader_round)
        else {
            return;
        };

        let cutoff_round = previous_round.saturating_sub(self.retention_rounds);

        let rounds_to_remove: Vec<_> = inner
            .transactions_by_round
            .range(..=cutoff_round)
            .map(|(round, _)| *round)
            .collect();

        let mut removed_count = 0;
        for round in rounds_to_remove {
            if let Some(digests) = inner.transactions_by_round.remove(&round) {
                for digest in digests {
                    if inner.transactions.remove(&digest).is_some() {
                        removed_count += 1;
                    }
                }
            }
        }

        if removed_count > 0 {
            debug!(
                "Cleaned up {removed_count} old transaction entries from SubmittedTransactionCache"
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, digest: &TransactionDigest) -> bool {
        self.inner.read().transactions.contains_key(digest)
    }

    #[cfg(test)]
    pub(crate) fn get_submission_count(&self, digest: &TransactionDigest) -> Option<u32> {
        self.inner
            .read()
            .transactions
            .get(digest)
            .map(|m| m.submission_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_digest(val: u8) -> TransactionDigest {
        let mut bytes = [0u8; 32];
        bytes[0] = val;
        TransactionDigest::new(bytes)
    }

    #[test]
    fn test_first_submission_allowed() {
        let cache = SubmittedTransactionCache::new(None, None);
        let digest = create_test_digest(1);

        cache.record_submitted_tx(&digest, 100, 0, None);
        assert!(cache.contains(&digest));
        assert_eq!(cache.get_submission_count(&digest), Some(0));

        let spam_weight = cache.increment_submission_count(&digest);
        assert_eq!(spam_weight, None);
        assert_eq!(cache.get_submission_count(&digest), Some(1));
    }

    #[test]
    fn test_retry_tolerance() {
        let cache = SubmittedTransactionCache::new(None, Some(10));
        let digest = create_test_digest(1);

        cache.record_submitted_tx(&digest, 100, 0, None);

        for i in 0..11 {
            let spam_weight = cache.increment_submission_count(&digest);
            assert_eq!(spam_weight, None, "Submission {} should be allowed", i + 1);
        }
        assert_eq!(cache.get_submission_count(&digest), Some(11));

        // 12th submission should trigger spam weight
        let spam_weight = cache.increment_submission_count(&digest);
        assert_eq!(spam_weight.map(|(w, _)| w), Some(Weight::one()));
        assert_eq!(cache.get_submission_count(&digest), Some(12));
    }

    #[test]
    fn test_amplification_factor() {
        let cache = SubmittedTransactionCache::new(None, Some(10));
        let digest = create_test_digest(1);

        // Record with amplification_factor=5, should allow 1+10+5=16 submissions
        cache.record_submitted_tx(&digest, 100, 5, None);

        // Should allow 16 submissions
        for i in 0..16 {
            let spam_weight = cache.increment_submission_count(&digest);
            assert_eq!(spam_weight, None, "Submission {} should be allowed", i + 1);
        }

        // 17th submission should trigger spam weight
        let spam_weight = cache.increment_submission_count(&digest);
        assert_eq!(spam_weight.map(|(w, _)| w), Some(Weight::one()));
    }

    #[test]
    fn test_garbage_collection() {
        let cache = SubmittedTransactionCache::new(Some(10), None);

        // Add transactions at different rounds
        for round in 1..=5 {
            let digest = create_test_digest(round as u8);
            cache.record_submitted_tx(&digest, round as Round, 0, None);
        }

        // Verify all 5 transactions are in cache
        for round in 1..=5 {
            let digest = create_test_digest(round as u8);
            assert!(cache.contains(&digest));
        }

        // First update doesn't GC anything (no previous round to use)
        cache.update_last_committed_round(15);
        for round in 1..=5 {
            let digest = create_test_digest(round as u8);
            assert!(cache.contains(&digest));
        }

        // Second update uses previous round (15) for GC
        // Cutoff = 15 - 10 = 5, so rounds 1-5 should be removed
        cache.update_last_committed_round(16);
        for round in 1..=5 {
            let digest = create_test_digest(round as u8);
            assert!(!cache.contains(&digest));
        }
    }

    #[test]
    fn test_retry_updates_round() {
        let cache = SubmittedTransactionCache::new(None, None);
        let digest = create_test_digest(1);

        // Initial submission at round 100
        cache.record_submitted_tx(&digest, 100, 0, None);

        // Verify it's tracked at round 100
        let inner = cache.inner.read();
        assert_eq!(
            inner.transactions.get(&digest).unwrap().submitted_round,
            100
        );
        assert!(inner
            .transactions_by_round
            .get(&100)
            .unwrap()
            .contains(&digest));
        drop(inner);

        // Retry at same round doesn't update
        cache.record_submitted_tx(&digest, 100, 0, None);
        let inner = cache.inner.read();
        assert_eq!(
            inner.transactions.get(&digest).unwrap().submitted_round,
            100
        );
        drop(inner);

        // Retry at later round updates the round
        cache.record_submitted_tx(&digest, 150, 0, None);
        let inner = cache.inner.read();
        assert_eq!(
            inner.transactions.get(&digest).unwrap().submitted_round,
            150
        );
        assert!(inner.transactions_by_round.get(&100).is_none()); // Removed from old round
        assert!(inner
            .transactions_by_round
            .get(&150)
            .unwrap()
            .contains(&digest)); // Added to new round
    }
}
