// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use roaring::RoaringBitmap;
use sui_index_dimensions::IndexDimension;
use sui_types::full_checkpoint_content::Checkpoint;

mod event_bitmap;
mod handler;
mod transaction_bitmap;

pub use event_bitmap::EventBitmapProcessor;
pub(crate) use handler::BitmapBatch;
pub use handler::BitmapIndexHandler;
pub use transaction_bitmap::TransactionBitmapProcessor;

/// Bits contributed to one bitmap-index row for a single framework commit.
pub struct BitmapIndexValue {
    pub row_key: Bytes,
    pub bucket_id: u64,
    pub bitmap: RoaringBitmap,
    // Max checkpoint for which this BitmapIndexValue contains bits
    pub max_cp: u64,
    // Checkpoint timestamp for max_cp
    pub max_ts_ms: u64,
    // Bitmaps for the same row are accumulated across commits. The tasks
    // that merge the bitmaps together are "sharded." This allows parallelization
    // of the compute and also reduces the working set size of the task.
    // The shard is computed by hashing the row key modulo the desired number of shards.
    // This is just an in-memory value used to route bitmaps to worker tasks, it's not
    // stored in the database.
    pub shard_id: u8,
}

/// A Roaring-bitmap inverted index definition.
///
/// The generic handler owns the shared checkpoint-to-row accumulation logic.
/// Individual processors only describe how a checkpoint contributes bits and
/// how those bits map into table-specific row keys.
pub trait BitmapIndexProcessor {
    const NAME: &'static str;
    /// The BigTable table that holds this index.
    const TABLE: &'static str;
    /// The column qualifier that holds the serialized bitmap.
    const COLUMN: &'static str;
    /// Row-key schema version prefix.
    const SCHEMA_VERSION: u32;
    /// Decimal width used to zero-pad bucket ids in row keys.
    const BUCKET_ID_WIDTH: usize;

    fn for_each_indexed_bit<F>(&self, checkpoint: &Checkpoint, emit: F)
    where
        F: FnMut(u64, u32, IndexDimension, &[u8]);

    /// Smallest `tx_hi_exclusive` that, once covered by the persisted
    /// committer watermark's `tx_hi`, guarantees no future checkpoint can
    /// contribute a bit to the bucket. Used for row eviction after the
    /// bucket is sealed.
    fn seal_tx_hi_exclusive(bucket_id: u64) -> u64;
}
