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
    *MAX_MUTATIONS
        .get()
        .expect("max_mutations not set; call set_max_mutations first")
}
