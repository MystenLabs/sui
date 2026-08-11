// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::atomic::Ordering;

use prometheus::Histogram;
use prometheus::IntCounter;
use prometheus::Registry;
use prometheus::register_histogram_with_registry;
use prometheus::register_int_counter_with_registry;

pub(crate) struct BitmapIndexMetrics {
    pub write_chunk_latency: Histogram,
    pub watermark_lag_ms: Histogram,
    pub retry_rows: IntCounter,
    /// Size of a row_key in bytes, observed when the shard builds a row
    /// flush.
    pub row_key_size_bytes: Histogram,
    /// Serialized size in bytes of the RoaringBitmap actually written to
    /// BigTable, observed when the shard builds a row flush.
    pub serialized_bitmap_size_bytes: Histogram,
}

impl BitmapIndexMetrics {
    pub(crate) fn new(pipeline: &'static str, registry: &Registry) -> Arc<Self> {
        let latency_buckets = prometheus::exponential_buckets(0.0005, 2.0, 18).unwrap();
        let lag_buckets = prometheus::exponential_buckets(1.0, 2.0, 18).unwrap();
        // 8B → ~16KB. Row keys are `v{version}#{dim_key}#{bucket_id:010}` —
        // typically tens of bytes (short dim tags + 32B address) but can
        // reach hundreds for struct-tag dimensions.
        let row_key_size_buckets = prometheus::exponential_buckets(8.0, 2.0, 12).unwrap();
        // 8B → ~8MB. Empty RoaringBitmap serializes to a few bytes; a dense
        // bitmap at BUCKET_SIZE = 2^20 tops out around ~128KB. The wide
        // range accommodates experimentation with larger bucket sizes.
        let bitmap_size_buckets = prometheus::exponential_buckets(8.0, 2.0, 21).unwrap();
        Arc::new(Self {
            write_chunk_latency: register_histogram_with_registry!(
                format!("bitmap_write_chunk_latency_seconds_{pipeline}"),
                "BigTable MutateRows latency per row-write chunk",
                latency_buckets,
                registry,
            )
            .unwrap(),
            watermark_lag_ms: register_histogram_with_registry!(
                format!("bitmap_watermark_lag_ms_{pipeline}"),
                "Wall-clock ms from commit observed until its watermark is persisted",
                lag_buckets,
                registry,
            )
            .unwrap(),
            retry_rows: register_int_counter_with_registry!(
                format!("bitmap_write_retry_rows_total_{pipeline}"),
                "Bitmap rows retried by the bitmap writer",
                registry,
            )
            .unwrap(),
            row_key_size_bytes: register_histogram_with_registry!(
                format!("bitmap_row_key_size_bytes_{pipeline}"),
                "Size in bytes of a row_key flushed by the bitmap committer",
                row_key_size_buckets,
                registry,
            )
            .unwrap(),
            serialized_bitmap_size_bytes: register_histogram_with_registry!(
                format!("bitmap_serialized_bitmap_size_bytes_{pipeline}"),
                "Serialized-size in bytes of a bitmap flushed by the bitmap committer",
                bitmap_size_buckets,
                registry,
            )
            .unwrap(),
        })
    }

    pub(crate) fn noop() -> Arc<Self> {
        Self::new_with_unique_prefix(&Registry::new())
    }

    fn new_with_unique_prefix(registry: &Registry) -> Arc<Self> {
        use std::sync::atomic::AtomicUsize;
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let name: &'static str = Box::leak(format!("test_{n}").into_boxed_str());
        Self::new(name, registry)
    }
}
