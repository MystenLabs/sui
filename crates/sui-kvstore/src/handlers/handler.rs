// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::RwLock;

use bytes::Bytes;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::BatchStatus;
use sui_indexer_alt_framework::pipeline::concurrent::Handler;
use sui_indexer_alt_framework_store_traits::Store;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::bigtable::client::PartialWriteError;
use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::bigtable::store::BigTableStore;

/// BigTable's hard limit for mutations per batch.
pub const BIGTABLE_MAX_MUTATIONS: usize = 100_000;

const DEFAULT_MAX_MUTATIONS: usize = 10_000;
static MAX_MUTATIONS: OnceLock<usize> = OnceLock::new();

/// Extension of `Processor` that specifies a BigTable table name.
pub trait BigTableProcessor: Processor<Value = Entry> {
    /// The BigTable table to write rows to.
    const TABLE: &'static str;

    /// How much concurrency to use when processing checkpoint data (default: 10).
    const FANOUT: usize = 10;

    /// Minimum rows before eager commit (default: 50).
    const MIN_EAGER_ROWS: usize = 50;
}

/// Generic wrapper that implements `concurrent::Handler` for any `BigTableProcessor`.
///
/// This adapter wraps a `BigTableProcessor` and provides the common batching and commit logic
/// for writing entries to BigTable. Individual pipelines implement `BigTableProcessor`.
pub struct BigTableHandler<P>(P);

/// Batch of BigTable entries.
/// Uses RwLock for interior mutability so we can remove succeeded entries on partial write failures.
#[derive(Default)]
pub struct BigTableBatch {
    inner: RwLock<BigTableBatchInner>,
}

#[derive(Default)]
struct BigTableBatchInner {
    entries: BTreeMap<Bytes, Entry>,
    total_mutations: usize,
}

impl<P> BigTableHandler<P>
where
    P: BigTableProcessor,
{
    pub fn new(processor: P) -> Self {
        Self(processor)
    }
}

#[async_trait::async_trait]
impl<P> Processor for BigTableHandler<P>
where
    P: BigTableProcessor + Send + Sync,
{
    const NAME: &'static str = P::NAME;
    const FANOUT: usize = <P as BigTableProcessor>::FANOUT;
    type Value = Entry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        self.0.process(checkpoint).await
    }
}

#[async_trait::async_trait]
impl<P> Handler for BigTableHandler<P>
where
    P: BigTableProcessor + Send + Sync,
{
    type Store = BigTableStore;
    type Batch = BigTableBatch;

    const MIN_EAGER_ROWS: usize = P::MIN_EAGER_ROWS;

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        let mut inner = batch.inner.write().unwrap();

        for entry in values {
            inner.total_mutations += entry.mutations.len();
            inner.entries.insert(entry.row_key.clone(), entry);

            if inner.total_mutations == max_mutations() {
                return BatchStatus::Ready;
            }
        }

        BatchStatus::Pending
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        let entries_to_write: Vec<Entry> = {
            let inner = batch.inner.read().unwrap();
            if inner.entries.is_empty() {
                return Ok(0);
            }
            inner.entries.values().cloned().collect()
        };
        let count = entries_to_write.len();

        match conn
            .client()
            .write_entries(P::TABLE, entries_to_write)
            .await
        {
            Ok(()) => Ok(count),
            Err(e) => {
                if let Some(partial) = e.downcast_ref::<PartialWriteError>() {
                    let mut inner = batch.inner.write().unwrap();
                    // Remove successful writes to reduce write load on the retry.
                    for key in &partial.succeeded_keys {
                        inner.entries.remove(key);
                    }
                }
                Err(e)
            }
        }
    }
}

/// Set the maximum mutations per batch. Must be called before creating any BigTableHandler.
/// Panics if called more than once or if value >= BIGTABLE_MAX_MUTATIONS.
pub fn set_max_mutations(value: usize) {
    assert!(
        value < BIGTABLE_MAX_MUTATIONS,
        "max_mutations must be less than {BIGTABLE_MAX_MUTATIONS}"
    );
    MAX_MUTATIONS.set(value).expect("max_mutations already set");
}

fn max_mutations() -> usize {
    *MAX_MUTATIONS.get_or_init(|| DEFAULT_MAX_MUTATIONS)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::bigtable::client::BigTableClient;
    use crate::bigtable::mock_server::{FailureConfig, MockBigtableServer};
    use crate::bigtable::store::BigTableStore;
    use crate::tables;

    /// Simple test processor for testing the handler.
    struct TestProcessor;

    #[async_trait::async_trait]
    impl Processor for TestProcessor {
        const NAME: &'static str = "test_pipeline";
        type Value = Entry;

        async fn process(&self, _: &Arc<Checkpoint>) -> anyhow::Result<Vec<Entry>> {
            Ok(vec![])
        }
    }

    impl BigTableProcessor for TestProcessor {
        const TABLE: &'static str = "test_table";
    }

    fn make_entry(key: &[u8]) -> Entry {
        tables::make_entry(key.to_vec(), [("col", Bytes::from_static(b"value"))], None)
    }

    #[tokio::test]
    async fn test_commit_removes_succeeded_entries_on_partial_failure() {
        // Start mock server
        let mock = MockBigtableServer::new();

        // First call: entry at index 1 fails with code 4 (DEADLINE_EXCEEDED)
        // BigTable returns status for ALL entries, so entries 0 and 2 succeed while entry 1 fails.
        // The handler removes succeeded entries (row0 and row2) leaving only row1.
        let mut failures = HashMap::new();
        failures.insert(1, 4);
        mock.expect(FailureConfig {
            entry_failures: failures,
        })
        .await;

        // Second call: all succeed
        mock.expect(FailureConfig::default()).await;

        let (addr, _handle) = mock.start().await.unwrap();

        // Create client and handler
        let client =
            BigTableClient::new_for_host(addr.to_string(), "test".to_string(), "test").unwrap();
        let store = BigTableStore::new(client);
        let mut conn = store.connect().await.unwrap();

        let handler = BigTableHandler::new(TestProcessor);

        // Build batch with 3 entries
        let mut batch = BigTableBatch::default();
        let entries = vec![
            make_entry(b"row0"),
            make_entry(b"row1"),
            make_entry(b"row2"),
        ];
        handler.batch(&mut batch, &mut entries.into_iter());

        // First commit: should fail with partial write error
        let result = handler.commit(&batch, &mut conn).await;
        assert!(result.is_err(), "Expected error on partial failure");

        // Verify batch has only 1 remaining entry (row1)
        // row0 and row2 succeeded, so they were removed
        {
            let inner = batch.inner.read().unwrap();
            assert_eq!(
                inner.entries.len(),
                1,
                "Batch should have 1 entry after partial failure"
            );
            assert!(
                !inner.entries.contains_key(&Bytes::from_static(b"row0")),
                "Succeeded entry row0 should be removed"
            );
            assert!(
                inner.entries.contains_key(&Bytes::from_static(b"row1")),
                "Failed entry row1 should remain"
            );
            assert!(
                !inner.entries.contains_key(&Bytes::from_static(b"row2")),
                "Succeeded entry row2 should be removed"
            );
        }

        // Second commit: should succeed with the remaining 1 entry
        let result = handler.commit(&batch, &mut conn).await;
        assert!(result.is_ok(), "Second commit should succeed");

        // Verify recorded calls
        let calls = mock.calls().await;
        assert_eq!(calls.len(), 2, "Should have made 2 MutateRows calls");

        // First call had 3 entries (in sorted order: row0, row1, row2)
        assert_eq!(
            calls[0].row_keys.len(),
            3,
            "First call should have 3 entries"
        );

        // Second call had 1 entry (row1)
        assert_eq!(
            calls[1].row_keys.len(),
            1,
            "Second call should have 1 entry"
        );
    }

    #[tokio::test]
    async fn test_commit_all_succeed() {
        let mock = MockBigtableServer::new();
        mock.expect(FailureConfig::default()).await;

        let (addr, _handle) = mock.start().await.unwrap();

        let client =
            BigTableClient::new_for_host(addr.to_string(), "test".to_string(), "test").unwrap();
        let store = BigTableStore::new(client);
        let mut conn = store.connect().await.unwrap();

        let handler = BigTableHandler::new(TestProcessor);

        let mut batch = BigTableBatch::default();
        let entries = vec![
            make_entry(b"row0"),
            make_entry(b"row1"),
            make_entry(b"row2"),
        ];
        handler.batch(&mut batch, &mut entries.into_iter());

        let result = handler.commit(&batch, &mut conn).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 3);

        // Batch entries are NOT cleared on success (that's the framework's job)
        let inner = batch.inner.read().unwrap();
        assert_eq!(inner.entries.len(), 3);
    }

    #[tokio::test]
    async fn test_commit_all_fail() {
        let mock = MockBigtableServer::new();

        // All entries fail
        let mut failures = HashMap::new();
        failures.insert(0, 4);
        failures.insert(1, 4);
        failures.insert(2, 4);
        mock.expect(FailureConfig {
            entry_failures: failures,
        })
        .await;

        let (addr, _handle) = mock.start().await.unwrap();

        let client =
            BigTableClient::new_for_host(addr.to_string(), "test".to_string(), "test").unwrap();
        let store = BigTableStore::new(client);
        let mut conn = store.connect().await.unwrap();

        let handler = BigTableHandler::new(TestProcessor);

        let mut batch = BigTableBatch::default();
        let entries = vec![
            make_entry(b"row0"),
            make_entry(b"row1"),
            make_entry(b"row2"),
        ];
        handler.batch(&mut batch, &mut entries.into_iter());

        let result = handler.commit(&batch, &mut conn).await;
        assert!(result.is_err());

        // All entries should remain since none succeeded
        let inner = batch.inner.read().unwrap();
        assert_eq!(
            inner.entries.len(),
            3,
            "All entries should remain when all fail"
        );
    }

    #[tokio::test]
    async fn test_commit_first_entry_fails() {
        let mock = MockBigtableServer::new();

        // First entry fails - but entries 1 and 2 succeed
        let mut failures = HashMap::new();
        failures.insert(0, 4);
        mock.expect(FailureConfig {
            entry_failures: failures,
        })
        .await;

        let (addr, _handle) = mock.start().await.unwrap();

        let client =
            BigTableClient::new_for_host(addr.to_string(), "test".to_string(), "test").unwrap();
        let store = BigTableStore::new(client);
        let mut conn = store.connect().await.unwrap();

        let handler = BigTableHandler::new(TestProcessor);

        let mut batch = BigTableBatch::default();
        let entries = vec![
            make_entry(b"row0"),
            make_entry(b"row1"),
            make_entry(b"row2"),
        ];
        handler.batch(&mut batch, &mut entries.into_iter());

        let result = handler.commit(&batch, &mut conn).await;
        assert!(result.is_err());

        // Only row0 should remain since entries 1 and 2 succeeded
        let inner = batch.inner.read().unwrap();
        assert_eq!(
            inner.entries.len(),
            1,
            "Only failed entry should remain when first fails"
        );
        assert!(inner.entries.contains_key(&Bytes::from_static(b"row0")));
        assert!(!inner.entries.contains_key(&Bytes::from_static(b"row1")));
        assert!(!inner.entries.contains_key(&Bytes::from_static(b"row2")));
    }

    #[tokio::test]
    async fn test_commit_last_entry_fails() {
        let mock = MockBigtableServer::new();

        // Last entry fails - entries 0 and 1 succeed before it
        let mut failures = HashMap::new();
        failures.insert(2, 4);
        mock.expect(FailureConfig {
            entry_failures: failures,
        })
        .await;

        let (addr, _handle) = mock.start().await.unwrap();

        let client =
            BigTableClient::new_for_host(addr.to_string(), "test".to_string(), "test").unwrap();
        let store = BigTableStore::new(client);
        let mut conn = store.connect().await.unwrap();

        let handler = BigTableHandler::new(TestProcessor);

        let mut batch = BigTableBatch::default();
        let entries = vec![
            make_entry(b"row0"),
            make_entry(b"row1"),
            make_entry(b"row2"),
        ];
        handler.batch(&mut batch, &mut entries.into_iter());

        let result = handler.commit(&batch, &mut conn).await;
        assert!(result.is_err());

        // Only the last entry should remain (row0 and row1 succeeded before the failure)
        let inner = batch.inner.read().unwrap();
        assert_eq!(inner.entries.len(), 1, "Only failed entry should remain");
        assert!(inner.entries.contains_key(&Bytes::from_static(b"row2")));
    }
}
