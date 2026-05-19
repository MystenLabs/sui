// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Roaring-bitmap inverted index keyed by packed event sequence number.
//!
//! Transactions have a global `tx_seq`, but events are only ordered within their
//! containing transaction. We pack `(tx_seq, event_idx)` into one globally
//! ordered `u64`:
//!
//! ```text
//! event_seq = (tx_seq << EVENT_BITS) | event_idx
//! ```
//!
//! `EVENT_BITS = 16` reserves 65,536 event slots per transaction. The protocol
//! config's `max_num_event_emit` is currently 1,024, so this gives 64x headroom
//! for future protocol changes while leaving 48 bits for `tx_seq` (~281T
//! historical transactions, over 55k times the chain's current transaction
//! count at the time this index was introduced). Even at a sustained 1M TPS,
//! 48 tx bits provide roughly 8.9 years of transaction history before a schema
//! bump would be needed.
//!
//! The packed namespace is sparse: most transactions use only a tiny prefix of
//! their reserved event slots. Each row spans 33,554,432 packed event positions
//! (`BUCKET_SIZE = 2^23`), which is exactly 128 transactions worth of event
//! namespace at `EVENT_BITS = 16`, while leaving bucket-relative bit positions
//! well within RoaringBitmap's `u32` limit.

pub const NAME: &str = "event_bitmap_index";

pub const SCHEMA_VERSION: u32 = 1;
pub const BUCKET_ID_WIDTH: usize = 12;
/// Number of packed `event_seq`s per bitmap bucket.
pub const BUCKET_SIZE: u64 = 8_388_608;

/// Number of low bits of `event_seq` reserved for the per-tx event index.
pub const EVENT_BITS: u32 = 16;

/// Maximum number of events per tx representable in a packed event_seq.
pub const MAX_EVENTS_PER_TX: u32 = 1 << EVENT_BITS;

/// Maximum tx_seq representable in a packed event_seq (48 bits at
/// EVENT_BITS = 16). Anything larger would shift bits off the top.
pub const MAX_TX_SEQ: u64 = u64::MAX >> EVENT_BITS;

pub mod col {
    pub const BITMAP: &str = "b";
}

/// Pack `(tx_seq, event_idx)` into a single globally-ordered u64 event_seq.
///
/// `event_idx` must be `< MAX_EVENTS_PER_TX` and `tx_seq` must be
/// `<= MAX_TX_SEQ`. Caller is responsible for respecting the protocol-level
/// limit on events-per-tx.
#[inline]
pub fn encode_event_seq(tx_seq: u64, event_idx: u32) -> u64 {
    debug_assert!(event_idx < MAX_EVENTS_PER_TX);
    debug_assert!(
        tx_seq <= MAX_TX_SEQ,
        "tx_seq {} exceeds {} bits and would lose data when shifted by EVENT_BITS={}",
        tx_seq,
        64 - EVENT_BITS,
        EVENT_BITS,
    );
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
    super::encode_bitmap_row_key_into(out, version, BUCKET_ID_WIDTH, dimension_key, bucket_id);
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
