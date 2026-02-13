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
///
/// Implementors define [`Self::process_sync`] with their CPU-bound serialization logic.
/// The async [`Processor::process`] method should just delegate to `process_sync`.
pub trait BigTableProcessor: Processor<Value = Entry> {
    /// The BigTable table to write rows to.
    const TABLE: &'static str;

    /// How much concurrency to use when processing checkpoint data (default: 10).
    const FANOUT: usize = 32;

    /// Minimum rows before eager commit (default: 50).
    const MIN_EAGER_ROWS: usize = 50;

    /// Max pending rows before backpressure (default: 5000).
    const MAX_PENDING_ROWS: usize = 5000;

    /// Max watermark updates per batch (default: 10_000).
    const MAX_WATERMARK_UPDATES: usize = 10_000;

    /// Synchronous checkpoint processing. Called on a blocking thread pool to avoid
    /// starving the tokio async worker threads.
    fn process_sync(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Entry>>;
}

/// Generic wrapper that implements `concurrent::Handler` for any `BigTableProcessor`.
///
/// This adapter wraps a `BigTableProcessor` and provides the common batching and commit logic
/// for writing entries to BigTable. Individual pipelines implement `BigTableProcessor`.
pub struct BigTableHandler<P>(Arc<P>);

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

impl BigTableBatch {
    pub fn total_mutations(&self) -> usize {
        self.inner.read().unwrap().total_mutations
    }
}

impl<P> BigTableHandler<P>
where
    P: BigTableProcessor,
{
    pub fn new(processor: P) -> Self {
        Self(Arc::new(processor))
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
        let processor = Arc::clone(&self.0);
        let checkpoint = Arc::clone(checkpoint);
        tokio::task::spawn_blocking(move || processor.process_sync(&checkpoint)).await?
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
    const MAX_PENDING_ROWS: usize = P::MAX_PENDING_ROWS;
    const MAX_WATERMARK_UPDATES: usize = P::MAX_WATERMARK_UPDATES;

    fn batch_weight(batch: &Self::Batch, _batch_len: usize) -> usize {
        batch.total_mutations()
    }

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        let mut inner = batch.inner.write().unwrap();

        for entry in values {
            inner.total_mutations += entry.mutations.len();
            inner.entries.insert(entry.row_key.clone(), entry);

            if inner.total_mutations >= max_mutations() {
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
        // Used only for metrics. On partial failure we return Err so the count
        // is not reported, but it would overcount if that ever changed.
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
                    let failed: std::collections::BTreeSet<&Bytes> =
                        partial.failed_keys.iter().map(|f| &f.key).collect();
                    inner.entries.retain(|key, _| failed.contains(key));
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
    use crate::bigtable::mock_server::{ExpectedCall, MockBigtableServer};
    use crate::bigtable::store::BigTableStore;
    use crate::tables;

    /// Simple test processor for testing the handler.
    struct TestProcessor;

    #[async_trait::async_trait]
    impl Processor for TestProcessor {
        const NAME: &'static str = "test_pipeline";
        type Value = Entry;

        async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Entry>> {
            self.process_sync(checkpoint)
        }
    }

    impl BigTableProcessor for TestProcessor {
        const TABLE: &'static str = "test_table";

        fn process_sync(&self, _: &Arc<Checkpoint>) -> anyhow::Result<Vec<Entry>> {
            Ok(vec![])
        }
    }

    fn make_entry(key: &[u8]) -> Entry {
        tables::make_entry(key.to_vec(), [("col", Bytes::from_static(b"value"))], None)
    }

    #[test]
    fn bigtable_batch_total_mutations() {
        let handler = BigTableHandler::new(TestProcessor);
        let mut batch = BigTableBatch::default();
        assert_eq!(batch.total_mutations(), 0);

        let entries: Vec<Entry> = (0..5)
            .map(|i| make_entry(format!("row{i}").as_bytes()))
            .collect();
        handler.batch(&mut batch, &mut entries.into_iter());

        // Each entry from make_entry has 1 mutation
        assert_eq!(batch.total_mutations(), 5);
    }

    #[tokio::test]
    async fn test_multi_round_partial_failure() {
        let mock = MockBigtableServer::new();

        // Round 1: 10 entries, odd indices fail (1,3,5,7,9) → 5 remain.
        mock.expect(ExpectedCall {
            row_keys: vec![
                b"row0", b"row1", b"row2", b"row3", b"row4", b"row5", b"row6", b"row7", b"row8",
                b"row9",
            ],
            failures: HashMap::from([(1, 8), (3, 8), (5, 8), (7, 8), (9, 8)]),
        })
        .await;

        // Round 2: 5 entries (row1,row3,row5,row7,row9 in sorted order),
        // positional indices 0,2,4 fail (row1,row5,row9) → 2 remain.
        mock.expect(ExpectedCall {
            row_keys: vec![b"row1", b"row3", b"row5", b"row7", b"row9"],
            failures: HashMap::from([(0, 8), (2, 8), (4, 8)]),
        })
        .await;

        // Round 3: 2 entries (row1,row5,row9), all succeed.
        mock.expect(ExpectedCall {
            row_keys: vec![b"row1", b"row5", b"row9"],
            failures: HashMap::new(),
        })
        .await;

        let (addr, _handle) = mock.start().await.unwrap();
        let client =
            BigTableClient::new_for_host(addr.to_string(), "test".to_string(), "test").unwrap();
        let store = BigTableStore::new(client);
        let mut conn = store.connect().await.unwrap();

        let handler = BigTableHandler::new(TestProcessor);
        let mut batch = BigTableBatch::default();
        let entries: Vec<Entry> = (0..10)
            .map(|i| make_entry(format!("row{i}").as_bytes()))
            .collect();
        handler.batch(&mut batch, &mut entries.into_iter());

        // Round 1: partial failure, batch shrinks from 10 to 5.
        let result = handler.commit(&batch, &mut conn).await;
        assert!(result.is_err());
        {
            let inner = batch.inner.read().unwrap();
            assert_eq!(inner.entries.len(), 5);
            for key in [b"row1", b"row3", b"row5", b"row7", b"row9"] {
                assert!(inner.entries.contains_key(key.as_slice()));
            }
        }

        // Round 2: partial failure, batch shrinks from 5 to 3.
        let result = handler.commit(&batch, &mut conn).await;
        assert!(result.is_err());
        {
            let inner = batch.inner.read().unwrap();
            assert_eq!(inner.entries.len(), 3);
            for key in [b"row1", b"row5", b"row9"] {
                assert!(inner.entries.contains_key(key.as_slice()));
            }
        }

        // Round 3: all succeed, batch still has 3 entries (framework clears, not handler).
        let result = handler.commit(&batch, &mut conn).await;
        assert_eq!(result.unwrap(), 3);
        {
            let inner = batch.inner.read().unwrap();
            assert_eq!(inner.entries.len(), 3);
        }
    }
}
