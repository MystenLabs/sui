// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Packed event-sequence encoding used by bitmap index storage.
//!
//! The public cursor/query layers use `(tx_seq, event_index)`. Bitmap storage
//! packs that coordinate into `event_seq = (tx_seq << EVENT_BITS) | event_idx`.

use std::ops::{Bound, Range};

/// Number of low bits of `event_seq` reserved for the per-tx event index.
pub const EVENT_BITS: u32 = 16;

/// Maximum number of events per tx representable in a packed event_seq.
pub const MAX_EVENTS_PER_TX: u32 = 1 << EVENT_BITS;

/// Maximum tx_seq representable in a packed event_seq.
pub const MAX_TX_SEQ: u64 = u64::MAX >> EVENT_BITS;
/// Pack `(tx_seq, event_idx)` into a globally ordered event_seq.
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

/// Unpack a packed event_seq back into `(tx_seq, event_idx)`.
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

/// Convert semantic event-coordinate bounds into the packed half-open range
/// scanned by bitmap indexes.
pub fn packed_range(lo: Bound<(u64, u32)>, hi: Bound<(u64, u32)>) -> Range<u64> {
    let start = match lo {
        Bound::Included((tx_seq, event_idx)) => saturating_lo(tx_seq, event_idx),
        Bound::Excluded((tx_seq, event_idx)) => saturating_successor(tx_seq, event_idx),
        Bound::Unbounded => 0,
    };

    let end = match hi {
        Bound::Included((tx_seq, event_idx)) => saturating_successor(tx_seq, event_idx),
        Bound::Excluded((tx_seq, event_idx)) => saturating_lo(tx_seq, event_idx),
        Bound::Unbounded => u64::MAX,
    };

    start..end
}

#[inline]
fn saturating_lo(tx_seq: u64, event_idx: u32) -> u64 {
    if tx_seq > MAX_TX_SEQ {
        return u64::MAX;
    }

    if event_idx >= MAX_EVENTS_PER_TX {
        return tx_seq
            .checked_add(1)
            .filter(|next_tx| *next_tx <= MAX_TX_SEQ)
            .map(event_seq_lo)
            .unwrap_or(u64::MAX);
    }

    encode_event_seq(tx_seq, event_idx)
}

#[inline]
fn saturating_successor(tx_seq: u64, event_idx: u32) -> u64 {
    if event_idx.saturating_add(1) < MAX_EVENTS_PER_TX {
        saturating_lo(tx_seq, event_idx + 1)
    } else {
        saturating_lo(tx_seq.saturating_add(1), 0)
    }
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
        let a = encode_event_seq(100, 0);
        let b = encode_event_seq(100, 5);
        assert!(a < b);

        let c = encode_event_seq(101, 0);
        assert!(b < c);
    }

    #[test]
    fn test_packed_range_whole_tx_span() {
        assert_eq!(
            packed_range(Bound::Included((10, 0)), Bound::Excluded((13, 0))),
            event_seq_lo(10)..event_seq_lo(13),
        );
    }

    #[test]
    fn test_packed_range_excluded_start_advances() {
        assert_eq!(
            packed_range(Bound::Excluded((10, 1)), Bound::Excluded((11, 0))).start,
            encode_event_seq(10, 2),
        );
    }

    #[test]
    fn test_packed_range_forged_extremes_saturate() {
        let oversized_event = packed_range(
            Bound::Included((10, u32::MAX)),
            Bound::Excluded((10, u32::MAX)),
        );
        assert!(oversized_event.is_empty());
        assert_eq!(oversized_event.start, event_seq_lo(11));

        let oversized_tx = packed_range(
            Bound::Included((u64::MAX, 0)),
            Bound::Excluded((u64::MAX, 0)),
        );
        assert!(oversized_tx.is_empty());
        assert_eq!(oversized_tx.start, u64::MAX);
        assert_eq!(oversized_tx.end, u64::MAX);
    }
}
