// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! BigTable-backed bitmap index scans.
//!
//! Generic DNF query planning and ordered bucket-stream merge-joins live in
//! `sui_inverted_index::bitmap_query`. This module only knows how to scan one
//! BigTable dimension key into the generic `(bucket_id, RoaringBitmap)` stream
//! shape.

use std::ops::Range;

use crate::tables::event_bitmap_index;
use crate::tables::transaction_bitmap_index;
use anyhow::Context;
use futures::StreamExt;
use roaring::RoaringBitmap;
use sui_inverted_index::BitmapBucketSource;
use sui_inverted_index::BucketStream;
use sui_inverted_index::ScanDirection;

use super::BigTableClient;

/// Identifies which inverted-index table a `BitmapQuery` evaluates against.
#[derive(Clone, Copy)]
pub struct BitmapIndexSpec {
    pub table_name: &'static str,
    pub schema_version: u32,
    pub bucket_size: u64,
    pub bucket_id_width: usize,
    pub bitmap_column: &'static str,
}

impl BitmapIndexSpec {
    pub const fn tx() -> Self {
        Self {
            table_name: transaction_bitmap_index::NAME,
            schema_version: transaction_bitmap_index::SCHEMA_VERSION,
            bucket_size: transaction_bitmap_index::BUCKET_SIZE,
            bucket_id_width: 10,
            bitmap_column: transaction_bitmap_index::col::BITMAP,
        }
    }

    pub const fn event() -> Self {
        Self {
            table_name: event_bitmap_index::NAME,
            schema_version: event_bitmap_index::SCHEMA_VERSION,
            bucket_size: event_bitmap_index::BUCKET_SIZE,
            bucket_id_width: 12,
            bitmap_column: event_bitmap_index::col::BITMAP,
        }
    }

    fn encode_row_key(&self, dimension_key: &[u8], bucket_id: u64) -> Vec<u8> {
        // Dispatch on `table_name` rather than `bucket_id_width`: cp- and
        // tx-bitmap rows share the same width, so width alone isn't a unique
        // discriminant.
        match self.table_name {
            transaction_bitmap_index::NAME => transaction_bitmap_index::encode_row_key(
                self.schema_version,
                dimension_key,
                bucket_id,
            ),
            event_bitmap_index::NAME => {
                event_bitmap_index::encode_row_key(self.schema_version, dimension_key, bucket_id)
            }
            name => panic!("unsupported bitmap table {name}"),
        }
    }
}

#[derive(Clone)]
pub struct BigTableBitmapSource {
    client: BigTableClient,
    spec: BitmapIndexSpec,
}

impl BigTableBitmapSource {
    pub fn new(client: BigTableClient, spec: BitmapIndexSpec) -> Self {
        Self { client, spec }
    }
}

impl BitmapBucketSource for BigTableBitmapSource {
    fn scan_bucket_stream(
        &self,
        dimension_key: Vec<u8>,
        range: Range<u64>,
        direction: ScanDirection,
    ) -> BucketStream {
        scan_bucket_stream(
            self.client.clone(),
            dimension_key,
            range,
            self.spec,
            direction,
        )
    }
}

/// Stream a single bitmap-index dimension's buckets in order, one
/// `RoaringBitmap` per bucket with **relative** bit positions.
fn scan_bucket_stream(
    mut client: BigTableClient,
    dimension_key: Vec<u8>,
    range: Range<u64>,
    spec: BitmapIndexSpec,
    direction: ScanDirection,
) -> BucketStream {
    async_stream::try_stream! {
        if range.is_empty() {
            return;
        }
        let start_bucket = range.start / spec.bucket_size;
        let end_bucket = (range.end - 1) / spec.bucket_size;

        let start_row = spec.encode_row_key(&dimension_key, start_bucket);
        let end_row = spec.encode_row_key(&dimension_key, end_bucket);

        let stream = client
            .range_scan_stream(
                spec.table_name,
                Some(bytes::Bytes::from(start_row)),
                Some(bytes::Bytes::from(end_row)),
                0,
                !direction.is_ascending(),
                None,
            )
            .await?;
        futures::pin_mut!(stream);

        while let Some(row) = stream.next().await {
            let (row_key, cells) = row?;
            let Some(bitmap_bytes) = cells
                .iter()
                .find(|(col, _)| col.as_ref() == spec.bitmap_column.as_bytes())
                .map(|(_, v)| v)
            else {
                continue;
            };

            let hash_pos = row_key
                .iter()
                .rposition(|&b| b == b'#')
                .context("malformed bitmap index row key: no '#' separator")?;
            let suffix = &row_key[hash_pos + 1..];
            let bucket_id: u64 = std::str::from_utf8(suffix)
                .context("non-ascii bucket_id suffix")?
                .parse()
                .context("invalid bucket_id in row key")?;

            let bitmap = RoaringBitmap::deserialize_from(bitmap_bytes.as_ref())
                .context("deserializing bitmap")?;
            yield (bucket_id, bitmap);
        }
    }
    .boxed()
}
