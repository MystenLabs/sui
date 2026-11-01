// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! GraphQL's own cursor format for transactions, events, and checkpoints.
//!
//! A cursor the ledger service minted is carried verbatim and never interpreted. Next to it, GraphQL
//! records the cursor's exact place in ledger order, as the ledger service reported it: an item's
//! key, read from its payload, or the `cursor_position` of a frame without an item. Cursors GraphQL
//! mints from data it holds (a live checkpoint, a preloaded checkpoint) carry only that place. A
//! change to the ledger service's cursor format therefore never changes how GraphQL reads its
//! cursors.
//!
//! A cursor in this format is [`FORMAT_MARKER`] followed by the BCS encoding of an envelope naming
//! the cursor's type.

use std::fmt;

use anyhow::Context as _;
use bytes::Bytes;
use serde::Deserialize;
use serde::Serialize;
use sui_rpc::proto::sui::rpc::v2;

use crate::api::scalars::cursor::ByteCursor;
use crate::api::scalars::cursor::OpaqueCursor;

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

/// A key that orders items in ledger order. A key also names a position: the boundary just before
/// the item with that key, whether or not an item has it.
pub trait LedgerKey: Copy + Ord + fmt::Debug + Send + Sync + 'static {
    /// The checkpoint holding the item.
    fn checkpoint(&self) -> u64;

    /// The smallest key a checkpoint's items can have.
    fn checkpoint_start(checkpoint: u64) -> Self;

    /// The key right after this one, whether or not an item has it.
    fn successor(&self) -> Self;

    /// The key naming a position the ledger service reported, with unset fields read as 0.
    fn from_position(position: &v2::LedgerPosition) -> Self;

    /// The key's transaction index and event index within its checkpoint, 0 where it has none.
    fn transaction_and_event_index(&self) -> (u64, u32);

    /// Decode a cursor over this key from GraphQL's own format, rejecting cursors of other types.
    fn decode_owned(bytes: &[u8]) -> anyhow::Result<OwnedCursor<Self>>;

    /// Encode a cursor over this key in GraphQL's own format.
    fn encode_owned(cursor: &OwnedCursor<Self>) -> Bytes;
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

        /// Where the ledger cursor sits: at its item's key for an item, or before the frame's
        /// `cursor_position` for a frame without an item. Encoded as an `Option` that is always
        /// set; a cursor without it does not decode.
        #[serde(with = "required_point")]
        point: Point<K>,
    },
}

/// A decoded transaction or event cursor, over items ordered by `K`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LedgerToken<K>(OwnedCursor<K>);

/// A transaction or event cursor as the GraphQL schema carries it.
pub type LedgerCursor<K> = OpaqueCursor<LedgerToken<K>>;

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
}

impl<K: Copy> OwnedCursor<K> {
    /// Where this cursor sits in ledger order.
    pub fn point(&self) -> Point<K> {
        match self {
            Self::Point(point) | Self::Ledger { point, .. } => *point,
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

impl<K: LedgerKey> LedgerToken<K> {
    /// The cursor of the item at `key`, minted from data GraphQL holds.
    pub(crate) fn at(key: K) -> LedgerCursor<K> {
        Self::from_point(Point::At(key))
    }

    /// The cursor at the start of `checkpoint`: a page after it starts with the checkpoint's first
    /// item.
    pub(crate) fn checkpoint_start(checkpoint: u64) -> LedgerCursor<K> {
        Self::from_point(Point::checkpoint_start(checkpoint))
    }

    /// A cursor at `point`, with no ledger bytes.
    pub(crate) fn from_point(point: Point<K>) -> LedgerCursor<K> {
        OpaqueCursor::new(Self(OwnedCursor::Point(point)))
    }

    /// A cursor for ledger bytes that sit at `point`.
    pub(crate) fn from_ledger(bytes: Bytes, point: Point<K>) -> LedgerCursor<K> {
        OpaqueCursor::new(Self(OwnedCursor::Ledger { bytes, point }))
    }

    /// Where this cursor sits in ledger order.
    pub fn point(&self) -> Point<K> {
        self.0.point()
    }

    /// The ledger service's cursor, if this cursor came from a ledger List stream.
    pub(crate) fn ledger_bytes(&self) -> Option<&Bytes> {
        self.0.ledger_bytes()
    }
}

impl<K: LedgerKey> ByteCursor for LedgerToken<K> {
    fn decode_cursor(bytes: &[u8]) -> anyhow::Result<Self> {
        Ok(Self(K::decode_owned(bytes)?))
    }

    fn encode_cursor(&self) -> Bytes {
        K::encode_owned(&self.0)
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

    /// `ListTransactions` positions carry no event index.
    fn from_position(position: &v2::LedgerPosition) -> Self {
        Self {
            checkpoint: position.checkpoint.unwrap_or_default(),
            transaction_index: position.transaction_index.unwrap_or_default(),
        }
    }

    fn transaction_and_event_index(&self) -> (u64, u32) {
        (self.transaction_index, 0)
    }

    fn decode_owned(bytes: &[u8]) -> anyhow::Result<OwnedCursor<Self>> {
        let Envelope::Transaction(cursor) = Envelope::decode(bytes)? else {
            anyhow::bail!("Not a transaction cursor");
        };
        Ok(cursor)
    }

    fn encode_owned(cursor: &OwnedCursor<Self>) -> Bytes {
        EnvelopeRef::Transaction(cursor).encode()
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

    fn from_position(position: &v2::LedgerPosition) -> Self {
        Self {
            checkpoint: position.checkpoint.unwrap_or_default(),
            transaction_index: position.transaction_index.unwrap_or_default(),
            event_index: position.event_index.unwrap_or_default(),
        }
    }

    fn transaction_and_event_index(&self) -> (u64, u32) {
        (self.transaction_index, self.event_index)
    }

    fn decode_owned(bytes: &[u8]) -> anyhow::Result<OwnedCursor<Self>> {
        let Envelope::Event(cursor) = Envelope::decode(bytes)? else {
            anyhow::bail!("Not an event cursor");
        };
        Ok(cursor)
    }

    fn encode_owned(cursor: &OwnedCursor<Self>) -> Bytes {
        EnvelopeRef::Event(cursor).encode()
    }
}

/// Encodes [`OwnedCursor::Ledger`]'s point as a `Some` and rejects a `None`, so a ledger cursor
/// always has an exact place.
mod required_point {
    use serde::Deserialize;
    use serde::Deserializer;
    use serde::Serialize;
    use serde::Serializer;
    use serde::de::Error as _;

    use super::Point;

    pub(super) fn serialize<K: Serialize, S: Serializer>(
        point: &Point<K>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Some(point).serialize(serializer)
    }

    pub(super) fn deserialize<'de, K: Deserialize<'de>, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Point<K>, D::Error> {
        Option::<Point<K>>::deserialize(deserializer)?.ok_or_else(|| {
            D::Error::custom(
                "ledger cursor has no position in ledger order; restart pagination without a cursor",
            )
        })
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

    /// A ledger position reads as a key with unset fields as 0; a transaction key reads no event
    /// index.
    #[test]
    fn keys_from_positions() {
        let position = v2::LedgerPosition::default()
            .with_checkpoint(7)
            .with_transaction_index(2)
            .with_event_index(1);
        assert_eq!(TransactionKey::from_position(&position), tx(7, 2));
        assert_eq!(
            EventKey::from_position(&position),
            EventKey {
                checkpoint: 7,
                transaction_index: 2,
                event_index: 1,
            }
        );

        let checkpoint_only = v2::LedgerPosition::default().with_checkpoint(7);
        assert_eq!(
            TransactionKey::from_position(&checkpoint_only),
            TransactionKey::checkpoint_start(7)
        );
        assert_eq!(
            EventKey::from_position(&checkpoint_only),
            EventKey::checkpoint_start(7)
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
                    point: Point::At(tx(7, 2)),
                },
                [
                    &[0x00, 0x00, 0x01, 0x02, 0xaa, 0xbb, 0x01, 0x00][..],
                    &TX_7_2,
                ]
                .concat(),
            ),
            // `Ledger`, the length-prefixed ledger bytes, then `Some(Before(..))`.
            (
                OwnedCursor::Ledger {
                    bytes: Bytes::from_static(&[0xaa]),
                    point: Point::Before(tx(7, 2)),
                },
                [&[0x00, 0x00, 0x01, 0x01, 0xaa, 0x01, 0x01][..], &TX_7_2].concat(),
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

    /// A ledger cursor whose point is `None` has no place in ledger order, so it is an invalid
    /// cursor, for transactions and events alike.
    #[test]
    fn ledger_cursor_without_a_point_is_rejected() {
        // Marker, `Transaction`/`Event`, `Ledger`, the length-prefixed ledger bytes, then `None`.
        for envelope in [0x00, 0x01] {
            let unplaced = [0x00, envelope, 0x01, 0x01, 0xaa, 0x00];
            let error = Envelope::decode(&unplaced).err().expect("rejected");
            assert!(
                format!("{error:#}").contains("ledger cursor has no position"),
                "{error:#}"
            );
        }
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
