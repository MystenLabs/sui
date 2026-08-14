// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_indexer_alt_reader::kv_loader::TransactionContents;
use sui_types::digests::TransactionDigest;

use crate::task::watermark::LEDGER_GRPC_PIPELINE;

use super::processed_checkpoint::ProcessedTransaction;
use super::streamed_cache_eviction::EvictableCache;
use super::streamed_store::StreamedStore;

/// Transaction store for streaming subscriptions that holds transaction contents not yet available
/// from the KV backend.
///
/// A streamed checkpoint runs ahead of the KV backend that `KvLoader` reads, so a by-digest lookup
/// for a just-streamed transaction (e.g. `Object.previousTransaction` on live data) misses the
/// backend and would resolve empty. Transactions from streamed checkpoints are indexed here so those
/// lookups resolve from memory; once the backend catches up, the eviction task removes them and the
/// backend serves them instead.
pub(crate) struct StreamedTransactionStore {
    cache: StreamedStore<TransactionDigest, Arc<TransactionContents>>,
}

impl StreamedTransactionStore {
    pub(crate) fn new() -> Self {
        Self {
            cache: StreamedStore::new(),
        }
    }

    /// Index a streamed checkpoint's transactions by digest. A transaction whose digest cannot be
    /// derived is skipped (it just falls through to the KV backend on lookup).
    pub(crate) fn index_transactions(
        &self,
        checkpoint_seq: u64,
        transactions: &[ProcessedTransaction],
    ) {
        for tx in transactions {
            if let Ok(digest) = tx.contents.digest() {
                self.cache
                    .insert(checkpoint_seq, digest, tx.contents.clone());
            }
        }
    }

    /// The contents of the transaction with `digest`, if held.
    pub(crate) fn get(&self, digest: &TransactionDigest) -> Option<Arc<TransactionContents>> {
        self.cache.get(digest)
    }
}

impl EvictableCache for StreamedTransactionStore {
    fn watermark_pipeline(&self) -> &'static str {
        LEDGER_GRPC_PIPELINE
    }

    fn evict_up_to(&self, indexed_checkpoint: u64) {
        self.cache.evict_up_to(indexed_checkpoint);
    }
}

#[cfg(test)]
mod tests {
    use super::super::streamed_cache_eviction::EvictableCache;
    use super::*;

    fn store() -> StreamedTransactionStore {
        StreamedTransactionStore::new()
    }

    fn digest(n: u8) -> TransactionDigest {
        TransactionDigest::new([n; 32])
    }

    fn processed(digest: TransactionDigest) -> ProcessedTransaction {
        ProcessedTransaction {
            tx_sequence_number: 0,
            contents: Arc::new(TransactionContents::for_test(digest)),
        }
    }

    #[test]
    fn index_then_get_hits_by_derived_digest() {
        let store = store();
        let tx = processed(digest(1));
        let contents = tx.contents.clone();
        store.index_transactions(5, std::slice::from_ref(&tx));

        assert!(Arc::ptr_eq(&store.get(&digest(1)).unwrap(), &contents));
        assert!(store.get(&digest(2)).is_none());
    }

    #[test]
    fn evict_up_to_removes_indexed_transaction() {
        let store = store();
        store.index_transactions(5, &[processed(digest(1))]);

        store.evict_up_to(5);

        assert!(store.get(&digest(1)).is_none());
    }

    #[test]
    fn evict_up_to_keeps_transaction_reindexed_at_later_checkpoint() {
        // A digest re-indexed at a later checkpoint survives eviction of the earlier one.
        let store = store();
        store.index_transactions(5, &[processed(digest(1))]);
        let reindexed = processed(digest(1));
        let contents = reindexed.contents.clone();
        store.index_transactions(10, std::slice::from_ref(&reindexed));

        store.evict_up_to(5);

        assert!(Arc::ptr_eq(&store.get(&digest(1)).unwrap(), &contents));
    }
}
