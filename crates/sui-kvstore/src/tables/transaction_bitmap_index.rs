// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub const NAME: &str = "transaction_bitmap_index";

pub const SCHEMA_VERSION: u32 = 1;
pub const BUCKET_ID_WIDTH: usize = 10;
/// Number of tx_sequence_numbers per bitmap bucket. Tied to SCHEMA_VERSION —
/// changing this requires a version bump and backfill into the new version prefix.
pub const BUCKET_SIZE: u64 = 65536;

pub mod col {
    pub const BITMAP: &str = "b";
}

/// Encode a full row key for the bitmap index.
///
/// Format: `v{version}#{dimension_key}#{bucket_id:010}`
///
/// - `version`: schema version integer
/// - `dimension_key`: `[tag_byte][value_bytes]` from `sui-inverted-index`
/// - `bucket_id`: `tx_sequence_number / bucket_size`, zero-padded to 10 digits
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
        let dim_key = vec![0x01, 0xAA, 0xBB];
        let key = encode_row_key(1, &dim_key, 42);
        let key_str = String::from_utf8_lossy(&key);
        // Should start with version prefix, contain the raw dimension bytes,
        // and end with zero-padded bucket id.
        assert!(key_str.starts_with("v1#"));
        assert!(key_str.ends_with("#0000000042"));
    }

    #[test]
    fn test_encode_row_key_ordering() {
        // Same dimension, different buckets should sort lexicographically
        let dim_key = vec![0x01, 0x42];
        let key1 = encode_row_key(1, &dim_key, 0);
        let key2 = encode_row_key(1, &dim_key, 1);
        let key3 = encode_row_key(1, &dim_key, 100);
        assert!(key1 < key2);
        assert!(key2 < key3);
    }

    #[test]
    fn test_different_versions_sort_separately() {
        let dim_key = vec![0x01, 0x42];
        let v1 = encode_row_key(1, &dim_key, 0);
        let v2 = encode_row_key(2, &dim_key, 0);
        assert!(v1 < v2);
    }
}
