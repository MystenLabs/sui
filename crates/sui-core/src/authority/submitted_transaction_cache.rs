// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use lru::LruCache;
use parking_lot::RwLock;
use prometheus::{
    Histogram, IntCounter, IntGauge, Registry, register_histogram_with_registry,
    register_int_counter_with_registry, register_int_gauge_with_registry,
};
use std::collections::BTreeSet;
use std::net::IpAddr;
use std::num::NonZeroUsize;
use std::sync::Arc;
use sui_types::digests::TransactionDigest;
use sui_types::traffic_control::Weight;
use tracing::debug;

pub(crate) const DEFAULT_CACHE_CAPACITY: usize = 100_000;

pub struct SubmittedTransactionCacheMetrics {
    pub transactions_tracked: IntGauge,
    pub spam_detected: IntCounter,
    pub submission_count_exceeded: Histogram,
    pub amplification_factor_distribution: Histogram,
}

impl SubmittedTransactionCacheMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            transactions_tracked: register_int_gauge_with_registry!(
                "submitted_transaction_cache_transactions_tracked",
                "Number of transactions currently tracked in the submission cache",
                registry,
            )
            .unwrap(),
            spam_detected: register_int_counter_with_registry!(
                "submitted_transaction_cache_spam_detected",
                "Number of transactions that exceeded submission limits",
                registry,
            )
            .unwrap(),
            submission_count_exceeded: register_histogram_with_registry!(
                "submitted_transaction_cache_submission_count_exceeded",
                "Distribution of submission counts when spam is detected",
                vec![
                    1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0,
                    10000.0,
                ],
                registry,
            )
            .unwrap(),
            amplification_factor_distribution: register_histogram_with_registry!(
                "submitted_transaction_cache_amplification_factor_distribution",
                "Distribution of amplification factors used for transaction submissions",
                vec![
                    1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0,
                    10000.0,
                ],
                registry,
            )
            .unwrap(),
        }
    }

    #[cfg(test)]
    pub(crate) fn new_test() -> Self {
        Self::new(&Registry::new())
    }
}

/// Cache for tracking submitted transactions to prevent DoS through excessive resubmissions.
/// Uses LRU eviction to automatically remove least recently used entries when at capacity.
/// Tracks submission counts and enforces gas-price-based amplification limits.
pub(crate) struct SubmittedTransactionCache {
    inner: RwLock<Inner>,
    metrics: Arc<SubmittedTransactionCacheMetrics>,
}

struct Inner {
    transactions: LruCache<TransactionDigest, SubmissionMetadata>,
}

#[derive(Debug, Clone)]
struct SubmissionMetadata {
    /// Number of times this transaction has been submitted
    submission_count: u32,
    /// Maximum allowed submissions based on gas price amplification
    max_allowed_submissions: u32,
    /// Set of client IP addresses that have submitted this transaction
    submitter_client_addrs: BTreeSet<IpAddr>,
}

impl SubmittedTransactionCache {
    pub(crate) fn new(
        cache_capacity: Option<usize>,
        metrics: Arc<SubmittedTransactionCacheMetrics>,
    ) -> Self {
        let capacity = cache_capacity
            .and_then(NonZeroUsize::new)
            .unwrap_or_else(|| NonZeroUsize::new(DEFAULT_CACHE_CAPACITY).unwrap());

        Self {
            inner: RwLock::new(Inner {
                transactions: LruCache::new(capacity),
            }),
            metrics,
        }
    }

    pub(crate) fn metrics(&self) -> Arc<SubmittedTransactionCacheMetrics> {
        self.metrics.clone()
    }

    pub(crate) fn record_submitted_tx(
        &self,
        digest: &TransactionDigest,
        amplification_factor: u32,
        submitter_client_addr: Option<IpAddr>,
    ) {
        let mut inner = self.inner.write();

        let max_allowed_submissions = amplification_factor;

        if let Some(metadata) = inner.transactions.get_mut(digest) {
            // Track additional client addresses for resubmissions
            if let Some(addr) = submitter_client_addr
                && metadata.submitter_client_addrs.insert(addr)
            {
                debug!("Added new client address {addr} for transaction {digest}");
            }
            debug!("Transaction {digest} already tracked in submission cache");
        } else {
            // First time we're submitting this transaction, however we will wait till
            // we see the transaction in consensus output to increment the submission count.
            let submitter_client_addrs = submitter_client_addr.into_iter().collect();
            let metadata = SubmissionMetadata {
                submission_count: 0,
                max_allowed_submissions,
                submitter_client_addrs,
            };

            inner.transactions.put(*digest, metadata);

            self.metrics
                .transactions_tracked
                .set(inner.transactions.len() as i64);
            self.metrics
                .amplification_factor_distribution
                .observe(amplification_factor as f64);

            debug!(
                "First submission of transaction {digest} (max_allowed: {max_allowed_submissions})",
            );
        }
    }

    /// Increments the submission count when we see a transaction in consensus output.
    /// This tracks how many times the transaction has appeared in consensus (from any validator).
    /// Returns the spam weight and set of submitter client addresses if the transaction exceeds allowed submissions.
    pub(crate) fn increment_submission_count(
        &self,
        digest: &TransactionDigest,
    ) -> Option<(Weight, BTreeSet<IpAddr>)> {
        let mut inner = self.inner.write();

        if let Some(metadata) = inner.transactions.get_mut(digest) {
            metadata.submission_count += 1;

            if metadata.submission_count > metadata.max_allowed_submissions {
                let spam_weight = Weight::one();
                self.metrics.spam_detected.inc();
                self.metrics
                    .submission_count_exceeded
                    .observe(metadata.submission_count as f64);

                debug!(
                    "Transaction {} seen in consensus {} times, exceeds limit {} (spam_weight: {:?})",
                    digest,
                    metadata.submission_count,
                    metadata.max_allowed_submissions,
                    spam_weight
                );

                return Some((spam_weight, metadata.submitter_client_addrs.clone()));
            }
        }
        // If we don't know about this transaction, it was submitted by another validator
        // We don't track spam weight for transactions we didn't submit
        None
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, digest: &TransactionDigest) -> bool {
        self.inner.read().transactions.contains(digest)
    }

    #[cfg(test)]
    pub(crate) fn get_submission_count(&self, digest: &TransactionDigest) -> Option<u32> {
        self.inner
            .read()
            .transactions
            .peek(digest)
            .map(|m| m.submission_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn create_test_digest(val: u8) -> TransactionDigest {
        let mut bytes = [0u8; 32];
        bytes[0] = val;
        TransactionDigest::new(bytes)
    }

    #[test]
    fn test_first_submission_allowed() {
        let cache = SubmittedTransactionCache::new(
            None,
            Arc::new(SubmittedTransactionCacheMetrics::new_test()),
        );
        let digest = create_test_digest(1);

        cache.record_submitted_tx(&digest, 1, None);
        assert!(cache.contains(&digest));
        assert_eq!(cache.get_submission_count(&digest), Some(0));

        let spam_weight = cache.increment_submission_count(&digest);
        assert_eq!(spam_weight, None);
        assert_eq!(cache.get_submission_count(&digest), Some(1));
    }

    #[test]
    fn test_amplification_factor() {
        let cache = SubmittedTransactionCache::new(
            None,
            Arc::new(SubmittedTransactionCacheMetrics::new_test()),
        );
        let digest = create_test_digest(1);

        // Record with amplification_factor=5, should allow 5 submissions
        cache.record_submitted_tx(&digest, 5, None);

        // Should allow 5 submissions
        for i in 0..5 {
            let spam_weight = cache.increment_submission_count(&digest);
            assert_eq!(spam_weight, None, "Submission {} should be allowed", i + 1);
        }

        // 6th submission should trigger spam weight
        let spam_weight = cache.increment_submission_count(&digest);
        assert_eq!(spam_weight.map(|(w, _)| w), Some(Weight::one()));

        // Additional submissions should also trigger spam weight
        for i in 6..10 {
            let spam_weight = cache.increment_submission_count(&digest);
            assert_eq!(
                spam_weight.map(|(w, _)| w),
                Some(Weight::one()),
                "Submission {} should trigger spam weight",
                i + 1
            );
        }
    }

    #[test]
    fn test_lru_eviction() {
        // Create a cache with capacity for only 3 transactions
        let cache = SubmittedTransactionCache::new(
            Some(3),
            Arc::new(SubmittedTransactionCacheMetrics::new_test()),
        );

        // Add 3 transactions
        for i in 1..=3 {
            let digest = create_test_digest(i);
            cache.record_submitted_tx(&digest, 1, None);
        }

        // Verify all 3 transactions are in cache
        for i in 1..=3 {
            let digest = create_test_digest(i);
            assert!(cache.contains(&digest));
        }

        // Add a 4th transaction, which should evict the least recently used (digest 1)
        let digest4 = create_test_digest(4);
        cache.record_submitted_tx(&digest4, 1, None);

        // Transaction 1 should be evicted (least recently used)
        assert!(!cache.contains(&create_test_digest(1)));
        // Transactions 2, 3, and 4 should still be in cache
        assert!(cache.contains(&create_test_digest(2)));
        assert!(cache.contains(&create_test_digest(3)));
        assert!(cache.contains(&digest4));
    }

    #[test]
    fn test_lru_access_updates_position() {
        // Create a cache with capacity for only 3 transactions
        let cache = SubmittedTransactionCache::new(
            Some(3),
            Arc::new(SubmittedTransactionCacheMetrics::new_test()),
        );

        // Add 3 transactions
        for i in 1..=3 {
            let digest = create_test_digest(i);
            cache.record_submitted_tx(&digest, 1, None);
        }

        // Access transaction 1 (moves it to front of LRU)
        let digest1 = create_test_digest(1);
        cache.increment_submission_count(&digest1);

        // Add a 4th transaction, which should now evict transaction 2 (now least recently used)
        let digest4 = create_test_digest(4);
        cache.record_submitted_tx(&digest4, 1, None);

        // Transaction 2 should be evicted
        assert!(!cache.contains(&create_test_digest(2)));
        // Transactions 1, 3, and 4 should still be in cache
        assert!(cache.contains(&digest1));
        assert!(cache.contains(&create_test_digest(3)));
        assert!(cache.contains(&digest4));
    }

    #[test]
    fn test_multiple_client_addresses() {
        let cache = SubmittedTransactionCache::new(
            None,
            Arc::new(SubmittedTransactionCacheMetrics::new_test()),
        );
        let digest = create_test_digest(1);
        let addr1 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let addr2 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
        let addr3 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 3));

        // First submission with addr1
        cache.record_submitted_tx(&digest, 2, Some(addr1));

        // Resubmission with addr2 - should track both addresses
        cache.record_submitted_tx(&digest, 2, Some(addr2));

        // Resubmission with addr1 again - should not duplicate
        cache.record_submitted_tx(&digest, 2, Some(addr1));

        // Resubmission with addr3 - should track all three
        cache.record_submitted_tx(&digest, 2, Some(addr3));

        // Increment submission count twice to exceed limit
        cache.increment_submission_count(&digest);
        cache.increment_submission_count(&digest);

        // Third submission should trigger spam weight for all addresses
        let result = cache.increment_submission_count(&digest);
        assert!(result.is_some());

        let (spam_weight, addrs) = result.unwrap();
        assert_eq!(spam_weight, Weight::one());
        assert_eq!(addrs.len(), 3);
        assert!(addrs.contains(&addr1));
        assert!(addrs.contains(&addr2));
        assert!(addrs.contains(&addr3));
    }

    #[test]
    fn test_retry_tracking() {
        // Create a cache with capacity for only 3 transactions
        let cache = SubmittedTransactionCache::new(
            Some(3),
            Arc::new(SubmittedTransactionCacheMetrics::new_test()),
        );
        let digest1 = create_test_digest(1);
        let digest2 = create_test_digest(2);
        let digest3 = create_test_digest(3);
        let digest4 = create_test_digest(4);

        // Add 3 transactions
        cache.record_submitted_tx(&digest1, 1, None);
        cache.record_submitted_tx(&digest2, 1, None);
        cache.record_submitted_tx(&digest3, 1, None);

        // Verify all 3 transactions are in cache
        assert!(cache.contains(&digest1));
        assert!(cache.contains(&digest2));
        assert!(cache.contains(&digest3));

        // Retry digest1 - this should move it to the front of LRU
        cache.record_submitted_tx(&digest1, 1, None);

        // Add a 4th transaction, which should evict the least recently used (digest2)
        cache.record_submitted_tx(&digest4, 1, None);

        // digest1 should still be in cache (moved to front by retry)
        assert!(cache.contains(&digest1));
        // digest2 should be evicted (was least recently used)
        assert!(!cache.contains(&digest2));
        // digest3 and digest4 should still be in cache
        assert!(cache.contains(&digest3));
        assert!(cache.contains(&digest4));
    }
}
