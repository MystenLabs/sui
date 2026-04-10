// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use roaring::RoaringBitmap;
use sui_indexer_alt_framework::pipeline::Processor;

mod accumulated;
mod async_pipeline;
mod event_bitmap;
mod handler;
mod reorder_buffer;
mod transaction_bitmap;

pub use event_bitmap::EventBitmapProcessor;
pub use handler::BitmapIndexHandler;
pub use transaction_bitmap::TransactionBitmapProcessor;

/// Bits contributed to a single bitmap-index row, tagged with the latest
/// checkpoint that contributed. Used as:
///
/// - A processor's output value (one per distinct row key per checkpoint —
///   `max_cp` / `max_ts_ms` are simply this checkpoint's values).
/// - The per-commit `BitmapIndexBatch`'s row type (OR'd across values for
///   the same `row_key`; `max_cp`/`max_ts_ms` track the highest).
/// - The per-pipeline `AccumulatedState`'s row type (OR'd across every
///   batch this process has committed).
///
/// `max_ts_ms` is written as the BigTable cell version so later cumulative
/// writes monotonically supersede earlier ones under `maxversions=1`.
/// Checkpoint timestamps are monotonically non-decreasing, so `max_cp` and
/// `max_ts_ms` always track together.
pub struct BitmapIndexValue {
    pub row_key: Bytes,
    pub bucket_id: u64,
    pub bitmap: RoaringBitmap,
    pub max_cp: u64,
    pub max_ts_ms: u64,
}

/// Extension of `Processor` that targets a Roaring-bitmap inverted index.
pub trait BitmapIndexProcessor: Processor<Value = BitmapIndexValue> {
    /// The BigTable table that holds this index.
    const TABLE: &'static str;
    /// The column qualifier that holds the serialized bitmap.
    const COLUMN: &'static str;

    /// Smallest `tx_hi_exclusive` that, once covered by the persisted
    /// committer watermark's `tx_hi`, guarantees no future checkpoint can
    /// contribute a bit to the bucket. Used for row eviction after the
    /// bucket is sealed.
    fn seal_tx_hi_exclusive(bucket_id: u64) -> u64;
}
