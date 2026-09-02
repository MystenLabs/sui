// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Async streaming bitmap-index handler.
//!
//! The handler owns processing and batching. Commit-time store side effects
//! live behind [`BigTableConnection`](crate::store::BigTableConnection):
//! it suppresses the framework's deferred watermark write and enqueues the
//! batch into the store-owned bitmap committer.

use std::sync::Arc;

use bytes::Bytes;
use roaring::RoaringBitmap;
use rustc_hash::FxHashMap;

use crate::bigtable::client::CheckpointSpan;
use crate::handlers::bitmap::BitmapIndexProcessor;
use crate::handlers::bitmap::BitmapIndexValue;
use crate::store::BigTableStore;
use crate::store::NUM_SHARDS;
use crate::store::shard_for;
use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential::Handler;
use sui_indexer_alt_framework_store_traits::Store;
use sui_types::full_checkpoint_content::Checkpoint;

pub struct BitmapIndexHandler<P> {
    processor: P,
}

/// Per-shard `Arc<Vec<BitmapIndexValue>>` slots, one per shard (indexed by
/// `shard_id`), plus the checkpoints that produced the batch's values.
pub struct BitmapBatch {
    shards: Vec<Arc<Vec<BitmapIndexValue>>>,
    checkpoints: Option<CheckpointSpan>,
}

impl Default for BitmapBatch {
    fn default() -> Self {
        Self {
            shards: (0..NUM_SHARDS).map(|_| Arc::new(Vec::new())).collect(),
            checkpoints: None,
        }
    }
}

impl BitmapBatch {
    pub(crate) fn clone_shards(&self) -> Vec<Arc<Vec<BitmapIndexValue>>> {
        self.shards.clone()
    }

    pub(crate) fn checkpoints(&self) -> Option<CheckpointSpan> {
        self.checkpoints
    }
}

impl<P> BitmapIndexHandler<P>
where
    P: BitmapIndexProcessor,
{
    pub(crate) fn new(processor: P) -> Self {
        Self { processor }
    }
}

#[async_trait]
impl<P> Processor for BitmapIndexHandler<P>
where
    P: BitmapIndexProcessor + Send + Sync + 'static,
{
    const NAME: &'static str = P::NAME;
    type Value = BitmapIndexValue;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let cp = checkpoint.summary.data();
        let max_cp = cp.sequence_number;
        let max_ts_ms = cp.timestamp_ms;
        let mut rows: FxHashMap<Vec<u8>, (u64, RoaringBitmap)> = FxHashMap::default();
        let mut row_key = Vec::new();
        self.processor
            .for_each_indexed_bit(checkpoint, |bucket_id, bit_position, dim, value| {
                crate::tables::encode_bitmap_row_key_parts_into(
                    &mut row_key,
                    P::SCHEMA_VERSION,
                    P::BUCKET_ID_WIDTH,
                    dim.tag_byte(),
                    value,
                    bucket_id,
                );
                if let Some((_, bm)) = rows.get_mut(row_key.as_slice()) {
                    bm.insert(bit_position);
                } else {
                    let mut bm = RoaringBitmap::new();
                    bm.insert(bit_position);
                    rows.insert(row_key.clone(), (bucket_id, bm));
                }
            });

        Ok(rows
            .into_iter()
            .map(|(row_key, (bucket_id, bitmap))| {
                let shard_id = shard_for(&row_key) as u8;
                BitmapIndexValue {
                    row_key: Bytes::from(row_key),
                    bucket_id,
                    bitmap,
                    max_cp,
                    max_ts_ms,
                    shard_id,
                }
            })
            .collect())
    }
}

#[async_trait]
impl<P> Handler for BitmapIndexHandler<P>
where
    P: BitmapIndexProcessor + Send + Sync + 'static,
{
    type Store = BigTableStore;
    type Batch = BitmapBatch;

    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Self::Value>) {
        for v in values {
            match &mut batch.checkpoints {
                Some(checkpoints) => checkpoints.include(v.max_cp),
                None => batch.checkpoints = Some(CheckpointSpan::single(v.max_cp)),
            }
            Arc::get_mut(&mut batch.shards[v.shard_id as usize])
                .expect("batch held exclusively during batch()")
                .push(v);
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        conn.commit_bitmap_batch::<P>(batch).await
    }
}
