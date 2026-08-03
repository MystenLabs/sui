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

pub const NAME: &str = "event_bitmap_index";

pub const SCHEMA_VERSION: u32 = 1;
pub const BUCKET_ID_WIDTH: usize = 12;
/// Number of packed `event_seq`s per bitmap bucket.
pub const BUCKET_SIZE: u64 = 268_435_456;

pub mod col {
    pub const BITMAP: &str = "b";
}

/// Encode a full row key for the event-keyed bitmap index.
///
/// Format: `v{version}#{dimension_key}#{bucket_id:012}`
///
/// Uses 12-digit zero-padded bucket ids because the event_seq namespace is
/// ~2^16 larger than the tx_seq namespace, overflowing the 10-digit format
/// used by the tx-keyed index.
pub fn encode_row_key(version: u32, dimension_key: &[u8], bucket_id: u64) -> Vec<u8> {
    let mut key = Vec::new();
    encode_row_key_into(&mut key, version, dimension_key, bucket_id);
    key
}

pub fn encode_row_key_into(out: &mut Vec<u8>, version: u32, dimension_key: &[u8], bucket_id: u64) {
    super::encode_bitmap_row_key_into(out, version, BUCKET_ID_WIDTH, dimension_key, bucket_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_row_key_format() {
        let dim_key = vec![0x01, 0xAA];
        let key = encode_row_key(1, &dim_key, 42);
        let key_str = String::from_utf8_lossy(&key);
        assert!(key_str.starts_with("v1#"));
        assert!(key_str.ends_with("#000000000042"));
    }

    #[test]
    fn test_bucket_size_fits_u32() {
        assert!(BUCKET_SIZE <= u32::MAX as u64);
    }
}
