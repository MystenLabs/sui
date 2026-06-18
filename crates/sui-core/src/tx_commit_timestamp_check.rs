// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Test-only check that the consensus handler and the checkpoint executor assign a transaction the
//! same consensus commit timestamp. Both call [`record_or_check`]; the first records, later ones
//! compare. The bounded LRU caps memory — eviction only drops a comparison, it can't false-positive
//! (digests are unique, timestamps deterministic).

use std::num::NonZeroUsize;
use std::sync::OnceLock;

use lru::LruCache;
use mysten_common::in_test_configuration;
use parking_lot::Mutex;
use sui_types::base_types::TransactionDigest;

// Exceeds the lag between consensus scheduling a transaction and the checkpoint executor reaching
// the checkpoint that contains it.
const CAPACITY: usize = 100_000;

fn seen() -> &'static Mutex<LruCache<TransactionDigest, u64>> {
    static SEEN: OnceLock<Mutex<LruCache<TransactionDigest, u64>>> = OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(LruCache::new(NonZeroUsize::new(CAPACITY).unwrap())))
}

/// In test configs, assert `digest` wasn't already assigned a different commit timestamp, then
/// record it. No-op in production.
pub(crate) fn record_or_check(digest: &TransactionDigest, timestamp_ms: u64) {
    if !in_test_configuration() {
        return;
    }
    if let Some(prev) = seen().lock().put(*digest, timestamp_ms) {
        assert_eq!(
            prev, timestamp_ms,
            "consensus commit timestamp mismatch for {digest:?}: recorded {prev}, now {timestamp_ms}",
        );
    }
}
