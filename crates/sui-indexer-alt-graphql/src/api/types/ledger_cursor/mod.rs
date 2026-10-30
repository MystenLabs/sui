// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! GraphQL's own cursor format for transactions, events, and checkpoints.
//!
//! A cursor the ledger service minted is carried verbatim and never interpreted. Next to it, GraphQL
//! records where it places that cursor in ledger order, from data it saw itself: an item's payload,
//! a page's coverage claim, the request that produced the page. Cursors GraphQL mints from data it
//! holds (a live checkpoint, a preloaded checkpoint) carry only that place. A change to the ledger
//! service's cursor format therefore never changes how GraphQL reads its cursors.
//!
//! A cursor in this format is [`FORMAT_MARKER`] followed by the BCS encoding of an envelope naming
//! the cursor's type.

use std::cmp::Ordering;
use std::fmt;

use anyhow::Context as _;
use bytes::Bytes;
use serde::Deserialize;
use serde::Serialize;

pub(crate) mod scan;

/// Leading byte of every cursor in GraphQL's own format. It is not a valid protobuf field key (field
/// number 0) and cannot start a JSON document, so GraphQL releases that predate this format reject
/// these cursors instead of decoding them into a position.
pub(crate) const FORMAT_MARKER: u8 = 0x00;

/// A transaction's place in ledger order: its checkpoint, then its index within the checkpoint.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransactionKey {
    pub checkpoint: u64,
    pub transaction_index: u64,
}

/// An event's place in ledger order: its transaction's place, then its index among the
/// transaction's events.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventKey {
    pub checkpoint: u64,
    pub transaction_index: u64,
    pub event_index: u32,
}

/// A key that orders items in ledger order.
pub trait LedgerKey: Copy + Ord + fmt::Debug + Send + Sync + 'static {
    /// The checkpoint holding the item.
    fn checkpoint(&self) -> u64;

    /// The smallest key a checkpoint's items can have.
    fn checkpoint_start(checkpoint: u64) -> Self;

    /// The key right after this one, whether or not an item has it.
    fn successor(&self) -> Self;
}

/// A place in ledger order that a cursor names.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Point<K> {
    /// At the item with this key: a page after it starts with the next item, and a page before it
    /// ends with the previous one.
    At(K),

    /// In the gap just before this key, whether or not an item has it: a page after it starts with
    /// the item at the key, and a page before it ends with the item before the key.
    Before(K),
}

/// A cursor in GraphQL's own format, over items ordered by `K`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum OwnedCursor<K> {
    /// A place GraphQL minted from data it holds.
    Point(Point<K>),

    /// A cursor the ledger service minted, carried verbatim.
    Ledger {
        bytes: Bytes,

        /// Where GraphQL places the ledger cursor. Exact for an item (read from its payload). For a
        /// page boundary, a place with the same matches on either side as the boundary, for the
        /// query that produced it. `None` when nothing GraphQL saw bounds the boundary.
        point: Option<Point<K>>,
    },
}

/// What follows [`FORMAT_MARKER`]. BCS encodes by position, so the layout of every type inside it
/// is frozen: variants may only be appended, here or in a nested enum, and no field may be added,
/// removed, or reordered, or cursors clients already hold would decode into different places
/// (`pinned_encodings` holds the bytes of each shape). [`EnvelopeRef`] must list the same variants
/// in the same order.
#[derive(Deserialize)]
pub(crate) enum Envelope {
    Transaction(OwnedCursor<TransactionKey>),
    Event(OwnedCursor<EventKey>),
    Checkpoint(u64),
}

/// [`Envelope`] by reference, for encoding without copying the cursor.
#[derive(Serialize)]
pub(crate) enum EnvelopeRef<'c> {
    Transaction(&'c OwnedCursor<TransactionKey>),
    Event(&'c OwnedCursor<EventKey>),
    Checkpoint(u64),
}

impl<K: LedgerKey> Point<K> {
    /// The gap before a checkpoint's first item.
    pub fn checkpoint_start(checkpoint: u64) -> Self {
        Self::Before(K::checkpoint_start(checkpoint))
    }

    pub fn key(&self) -> K {
        match self {
            Self::At(key) | Self::Before(key) => *key,
        }
    }

    /// Whether a page after this point includes the item at `key`.
    pub fn includes_after(&self, key: &K) -> bool {
        match self {
            Self::At(at) => key > at,
            Self::Before(before) => key >= before,
        }
    }

    /// Whether a page before this point includes the item at `key`.
    pub fn includes_before(&self, key: &K) -> bool {
        *key < self.key()
    }

    /// The gap where a page after this point starts.
    pub fn gap_after(self) -> Self {
        match self {
            Self::At(key) => Self::Before(key.successor()),
            Self::Before(_) => self,
        }
    }

    /// The gap where a page before this point ends.
    pub fn gap_before(self) -> Self {
        Self::Before(self.key())
    }

    /// The latest checkpoint holding an item at or before this point, or `None` if every item comes
    /// after it.
    pub fn last_checkpoint(&self) -> Option<u64> {
        match self {
            Self::At(key) => Some(key.checkpoint()),
            Self::Before(key) if *key == K::checkpoint_start(key.checkpoint()) => {
                key.checkpoint().checked_sub(1)
            }
            Self::Before(key) => Some(key.checkpoint()),
        }
    }

    /// Orders the gap before a key ahead of the item at it.
    fn rank(&self) -> (K, u8) {
        match self {
            Self::Before(key) => (*key, 0),
            Self::At(key) => (*key, 1),
        }
    }
}

impl<K: Copy> OwnedCursor<K> {
    /// Where GraphQL places this cursor, if it can.
    pub fn point(&self) -> Option<Point<K>> {
        match self {
            Self::Point(point) => Some(*point),
            Self::Ledger { point, .. } => *point,
        }
    }

    /// The ledger service's cursor, if this cursor came from a ledger List stream.
    pub fn ledger_bytes(&self) -> Option<&Bytes> {
        match self {
            Self::Point(_) => None,
            Self::Ledger { bytes, .. } => Some(bytes),
        }
    }
}

impl Envelope {
    /// Decode a cursor in GraphQL's own format.
    pub(crate) fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        let payload = bytes
            .strip_prefix(&[FORMAT_MARKER])
            .context("Invalid cursor")?;
        bcs::from_bytes(payload).context("Invalid cursor")
    }
}

impl EnvelopeRef<'_> {
    pub(crate) fn encode(&self) -> Bytes {
        let mut bytes = vec![FORMAT_MARKER];
        bytes.extend(bcs::to_bytes(self).expect("cursor envelopes always serialize"));
        bytes.into()
    }
}

impl LedgerKey for TransactionKey {
    fn checkpoint(&self) -> u64 {
        self.checkpoint
    }

    fn checkpoint_start(checkpoint: u64) -> Self {
        Self {
            checkpoint,
            transaction_index: 0,
        }
    }

    fn successor(&self) -> Self {
        Self {
            checkpoint: self.checkpoint,
            transaction_index: self.transaction_index.saturating_add(1),
        }
    }
}

impl LedgerKey for EventKey {
    fn checkpoint(&self) -> u64 {
        self.checkpoint
    }

    fn checkpoint_start(checkpoint: u64) -> Self {
        Self {
            checkpoint,
            transaction_index: 0,
            event_index: 0,
        }
    }

    fn successor(&self) -> Self {
        Self {
            event_index: self.event_index.saturating_add(1),
            ..*self
        }
    }
}

impl<K: LedgerKey> Ord for Point<K> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank().cmp(&other.rank())
    }
}

impl<K: LedgerKey> PartialOrd for Point<K> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx(checkpoint: u64, transaction_index: u64) -> TransactionKey {
        TransactionKey {
            checkpoint,
            transaction_index,
        }
    }

    /// The gap before a key sorts between the item before it and the item at it.
    #[test]
    fn gaps_sort_between_items() {
        use Point::At;
        use Point::Before;

        let points = [
            At(tx(4, 9)),
            Before(tx(5, 0)),
            At(tx(5, 0)),
            Before(tx(5, 1)),
            At(tx(5, 1)),
            Before(tx(6, 0)),
        ];
        assert!(points.windows(2).all(|pair| pair[0] < pair[1]));
    }

    /// An item cursor excludes its item from pages on both sides; a gap cursor puts the item at its
    /// key on the `after` side.
    #[test]
    fn items_and_gaps_split_pages() {
        let at = Point::At(tx(5, 1));
        assert!(!at.includes_after(&tx(5, 1)));
        assert!(at.includes_after(&tx(5, 2)));
        assert!(!at.includes_before(&tx(5, 1)));
        assert!(at.includes_before(&tx(5, 0)));

        let gap = Point::Before(tx(5, 1));
        assert!(gap.includes_after(&tx(5, 1)));
        assert!(!gap.includes_before(&tx(5, 1)));
        assert!(gap.includes_before(&tx(5, 0)));

        // The gap after an item and the gap before it bound the same pages as the item does.
        for key in [tx(5, 0), tx(5, 1), tx(5, 2)] {
            assert_eq!(at.gap_after().includes_after(&key), at.includes_after(&key));
            assert_eq!(
                at.gap_before().includes_before(&key),
                at.includes_before(&key)
            );
        }
    }

    /// The last checkpoint a point has reached: an item's own checkpoint, or the checkpoint before a
    /// gap at a checkpoint's start.
    #[test]
    fn last_checkpoint_reached() {
        assert_eq!(Point::At(tx(5, 0)).last_checkpoint(), Some(5));
        assert_eq!(Point::Before(tx(5, 3)).last_checkpoint(), Some(5));
        assert_eq!(
            Point::<TransactionKey>::checkpoint_start(5).last_checkpoint(),
            Some(4)
        );
        assert_eq!(
            Point::<TransactionKey>::checkpoint_start(0).last_checkpoint(),
            None
        );
    }

    /// The exact bytes of one cursor of each shape. A reordered field or variant still round-trips,
    /// but decodes cursors clients already hold into different places; these pins catch that.
    #[test]
    fn pinned_encodings() {
        // `tx(7, 2)`: checkpoint, then transaction index, as little-endian u64s.
        const TX_7_2: [u8; 16] = [7, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0];

        let decode_transaction = |bytes: &[u8]| match Envelope::decode(bytes).unwrap() {
            Envelope::Transaction(cursor) => cursor,
            _ => panic!("expected a transaction cursor"),
        };

        // Marker, then the `Transaction`, `Point`, and `At`/`Before` variant indices.
        let transactions = [
            (
                OwnedCursor::Point(Point::At(tx(7, 2))),
                [&[0x00, 0x00, 0x00, 0x00][..], &TX_7_2].concat(),
            ),
            (
                OwnedCursor::Point(Point::Before(tx(7, 2))),
                [&[0x00, 0x00, 0x00, 0x01][..], &TX_7_2].concat(),
            ),
            // `Ledger`, the length-prefixed ledger bytes, then `Some(At(..))`.
            (
                OwnedCursor::Ledger {
                    bytes: Bytes::from_static(&[0xaa, 0xbb]),
                    point: Some(Point::At(tx(7, 2))),
                },
                [
                    &[0x00, 0x00, 0x01, 0x02, 0xaa, 0xbb, 0x01, 0x00][..],
                    &TX_7_2,
                ]
                .concat(),
            ),
            (
                OwnedCursor::Ledger {
                    bytes: Bytes::from_static(&[0xaa]),
                    point: None,
                },
                vec![0x00, 0x00, 0x01, 0x01, 0xaa, 0x00],
            ),
        ];
        for (cursor, expected) in transactions {
            assert_eq!(EnvelopeRef::Transaction(&cursor).encode()[..], expected[..]);
            assert_eq!(decode_transaction(&expected), cursor);
        }

        // Marker, `Event`, `Point`, `At`, the transaction's place, then the event index as a u32.
        let event = OwnedCursor::Point(Point::At(EventKey {
            checkpoint: 7,
            transaction_index: 2,
            event_index: 1,
        }));
        let expected = [&[0x00, 0x01, 0x00, 0x00][..], &TX_7_2, &[1, 0, 0, 0]].concat();
        assert_eq!(EnvelopeRef::Event(&event).encode()[..], expected[..]);
        let Envelope::Event(decoded) = Envelope::decode(&expected).unwrap() else {
            panic!("expected an event cursor");
        };
        assert_eq!(decoded, event);

        // Marker, `Checkpoint`, then the sequence number.
        let expected = [0x00, 0x02, 9, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(EnvelopeRef::Checkpoint(9).encode()[..], expected[..]);
        let Envelope::Checkpoint(decoded) = Envelope::decode(&expected).unwrap() else {
            panic!("expected a checkpoint cursor");
        };
        assert_eq!(decoded, 9);
    }

    /// Bytes without the marker, and marked bytes that do not parse, are an error.
    #[test]
    fn decode_requires_the_marker() {
        assert!(Envelope::decode(&[0x28, 0x01]).is_err());
        assert!(Envelope::decode(&[]).is_err());
        assert!(Envelope::decode(&[FORMAT_MARKER]).is_err());
        assert!(Envelope::decode(&[FORMAT_MARKER, 0x7f]).is_err());
    }
}
