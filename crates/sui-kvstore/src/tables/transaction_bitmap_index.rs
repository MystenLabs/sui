// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::BitmapSchema;

/// Table of [`BitmapSchema::V1`] rows.
pub const NAME: &str = "transaction_bitmap_index";
/// Table of [`BitmapSchema::V2`] rows.
pub const NAME_V2: &str = "transaction_bitmap_index_v2";

pub const BUCKET_ID_WIDTH: usize = 10;
/// Number of tx_sequence_numbers per bitmap bucket. Changing it requires a new table and a
/// backfill.
pub const BUCKET_SIZE: u64 = 65536;

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

/// Encode a full row key for the bitmap index.
///
/// Format: `{prefix}{dimension_key}#{bucket_id:010}`
///
/// - `prefix`: `v1#` for [`BitmapSchema::V1`], empty for [`BitmapSchema::V2`]
/// - `dimension_key`: `[tag_byte][value_bytes]` from `sui-inverted-index`
/// - `bucket_id`: `tx_sequence_number / bucket_size`, zero-padded to 10 digits
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
        let dim_key = vec![0x01, 0xAA, 0xBB];
        assert_eq!(
            encode_row_key(BitmapSchema::V2, &dim_key, 42),
            b"\x01\xAA\xBB#0000000042",
        );
    }

    #[test]
    fn test_encode_row_key_ordering() {
        // Same dimension, different buckets should sort lexicographically
        let dim_key = vec![0x01, 0x42];
        let key1 = encode_row_key(BitmapSchema::V2, &dim_key, 0);
        let key2 = encode_row_key(BitmapSchema::V2, &dim_key, 1);
        let key3 = encode_row_key(BitmapSchema::V2, &dim_key, 100);
        assert!(key1 < key2);
        assert!(key2 < key3);
    }
}
