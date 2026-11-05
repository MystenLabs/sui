// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Roaring-bitmap inverted index keyed by packed event sequence number.
//!
//! The packed event-sequence encoding is owned by
//! [`sui_inverted_index::event_seq`]. This table stores those packed values in
//! bitmap buckets; its row-key layout stays byte-identical across encoding
//! refactors.
//!
//! The packed namespace is sparse: most transactions use only a tiny prefix of
//! their reserved event slots. Each row spans 268,435,456 packed event positions
//! (`BUCKET_SIZE = 2^28`), which is exactly 4,096 transactions worth of event
//! namespace at `EVENT_BITS = 16`, while leaving bucket-relative bit positions
//! (max `2^28 - 1`) well within RoaringBitmap's `u32` limit.

use super::BitmapSchema;

/// Table of [`BitmapSchema::V1`] rows.
pub const NAME: &str = "event_bitmap_index";
/// Table of [`BitmapSchema::V2`] rows.
pub const NAME_V2: &str = "event_bitmap_index_v2";

pub const BUCKET_ID_WIDTH: usize = 12;
/// Number of packed `event_seq`s per bitmap bucket. Changing it requires a new table and a
/// backfill.
pub const BUCKET_SIZE: u64 = 268_435_456;

pub mod col {
    pub const BITMAP: &str = "b";
}

/// The table that holds the rows of `schema`.
pub const fn table_name(schema: BitmapSchema) -> &'static str {
    match schema {
        BitmapSchema::V1 => NAME,
        BitmapSchema::V2 => NAME_V2,
    }
}

/// Encode a full row key for the event-keyed bitmap index.
///
/// Format: `{prefix}{dimension_key}#{bucket_id:012}`, where the prefix is `v1#` for
/// [`BitmapSchema::V1`] and empty for [`BitmapSchema::V2`].
///
/// Uses 12-digit zero-padded bucket ids because the event_seq namespace is
/// ~2^16 larger than the tx_seq namespace, overflowing the 10-digit format
/// used by the tx-keyed index.
pub fn encode_row_key(schema: BitmapSchema, dimension_key: &[u8], bucket_id: u64) -> Vec<u8> {
    let mut key = Vec::new();
    super::encode_bitmap_row_key_into(&mut key, schema, BUCKET_ID_WIDTH, dimension_key, bucket_id);
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_row_key_format() {
        let dim_key = vec![0x01, 0xAA];
        assert_eq!(
            encode_row_key(BitmapSchema::V2, &dim_key, 42),
            b"\x01\xAA#000000000042",
        );
    }

    #[test]
    fn test_bucket_size_fits_u32() {
        assert!(BUCKET_SIZE <= u32::MAX as u64);
    }
}
