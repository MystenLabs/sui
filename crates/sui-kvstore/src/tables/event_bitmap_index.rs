// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Roaring-bitmap inverted index keyed by `event_seq` (a packing of
//! `(tx_seq, event_idx)` into a single u64, see [`encode_event_seq`]).
//!
//! Row layout mirrors [`crate::tables::transaction_bitmap_index`] but in event-space:
//! bits within a bucket correspond to packed event_seqs, not tx_seqs. Gaps
//! in the namespace (tx_seqs with no event at a given index) are unused;
//! RoaringBitmap handles the sparsity via run-length encoding.

pub const NAME: &str = "event_bitmap_index";

pub const SCHEMA_VERSION: u32 = 1;
/// Number of packed `event_seq`s per bitmap bucket. Must stay ≤ u32::MAX
/// because bucket-relative bit positions are stored as u32.
///
/// Sized up relative to the tx-keyed index because the event_seq namespace
/// is sparse: at `EVENT_BITS = 16` only ~1/16k positions correspond to real
/// events, so a larger bucket keeps a similar number of *live* bits per row.
pub const BUCKET_SIZE: u64 = 8_388_608;

/// Number of low bits of `event_seq` reserved for the per-tx event index.
///
/// `max_num_event_emit` is currently 1024 (sui-protocol-config). 16 bits
/// gives 64× headroom while still leaving 48 bits (~280T) of tx_seq space.
pub const EVENT_BITS: u32 = 16;

/// Maximum number of events per tx representable in a packed event_seq.
pub const MAX_EVENTS_PER_TX: u32 = 1 << EVENT_BITS;

pub mod col {
    pub const BITMAP: &str = "b";
}

/// Pack `(tx_seq, event_idx)` into a single globally-ordered u64 event_seq.
///
/// `event_idx` must be `< MAX_EVENTS_PER_TX`. Caller is responsible for
/// respecting the protocol-level limit on events-per-tx.
#[inline]
pub fn encode_event_seq(tx_seq: u64, event_idx: u32) -> u64 {
    debug_assert!(event_idx < MAX_EVENTS_PER_TX);
    (tx_seq << EVENT_BITS) | (event_idx as u64)
}

/// Inverse of [`encode_event_seq`]: unpack a packed event_seq back into
/// `(tx_seq, event_idx)`.
#[inline]
pub fn decode_event_seq(event_seq: u64) -> (u64, u32) {
    let tx_seq = event_seq >> EVENT_BITS;
    let event_idx = (event_seq & (MAX_EVENTS_PER_TX as u64 - 1)) as u32;
    (tx_seq, event_idx)
}

/// Lowest possible event_seq for a given tx_seq (idx 0).
#[inline]
pub fn event_seq_lo(tx_seq: u64) -> u64 {
    tx_seq << EVENT_BITS
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
    let prefix = format!("v{version}#");
    let suffix = format!("#{bucket_id:012}");
    out.clear();
    out.reserve(prefix.len() + dimension_key.len() + suffix.len());
    out.extend_from_slice(prefix.as_bytes());
    out.extend_from_slice(dimension_key);
    out.extend_from_slice(suffix.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_event_seq_roundtrip() {
        for (tx_seq, event_idx) in [
            (0, 0),
            (0, 1023),
            (1, 0),
            (1_000_000, 42),
            (u64::MAX >> EVENT_BITS, MAX_EVENTS_PER_TX - 1),
        ] {
            let packed = encode_event_seq(tx_seq, event_idx);
            assert_eq!(decode_event_seq(packed), (tx_seq, event_idx));
        }
    }

    #[test]
    fn test_event_seq_ordering() {
        // Same tx, increasing idx → increasing event_seq
        let a = encode_event_seq(100, 0);
        let b = encode_event_seq(100, 5);
        assert!(a < b);
        // Different tx, same idx → tx_seq dominates
        let c = encode_event_seq(101, 0);
        assert!(b < c);
    }

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
