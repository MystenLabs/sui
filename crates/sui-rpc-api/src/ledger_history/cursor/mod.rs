// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use bytes::Bytes;
use prost::Message as _;
use sui_inverted_index::ScanDirection;
use sui_rpc::proto::sui::rpc::v2::LedgerPosition;

mod proto;

use proto::sui::rpc::cursor::v1 as grpc;

/// Pagination cursor for ledger-history endpoints.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorToken {
    pub kind: CursorKind,
    pub position: Position,
}

/// A boundary in public ledger order (`sui.rpc.v2.LedgerPosition`): just before the item keyed
/// `(checkpoint, transaction_index, event_index)`. An index past the last transaction of a
/// checkpoint (or past the last event of a transaction) names the start of the next one, so
/// different points can name the same boundary; they still order correctly against every item
/// key. Transaction cursors carry `event_index` 0.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct LedgerPoint {
    pub checkpoint: u64,
    pub transaction_index: u64,
    pub event_index: u32,
}

impl LedgerPoint {
    /// The start of the ledger.
    pub const GENESIS: Self = Self {
        checkpoint: 0,
        transaction_index: 0,
        event_index: 0,
    };

    /// The boundary before `checkpoint`'s first item.
    pub fn checkpoint_start(checkpoint: u64) -> Self {
        Self {
            checkpoint,
            ..Self::GENESIS
        }
    }

    /// The boundary before the transaction at `transaction_index` of `checkpoint`, or a
    /// transaction's key.
    pub fn transaction(checkpoint: u64, transaction_index: u64) -> Self {
        Self {
            checkpoint,
            transaction_index,
            event_index: 0,
        }
    }

    /// The boundary before an event, or an event's key.
    pub fn event(checkpoint: u64, transaction_index: u64, event_index: u32) -> Self {
        Self {
            checkpoint,
            transaction_index,
            event_index,
        }
    }

    /// The wire form of this point for a watermark whose cursor is `position`: only the
    /// coordinates of that lane's keys are set.
    pub fn to_ledger_position(self, position: &Position) -> LedgerPosition {
        let proto = LedgerPosition::default().with_checkpoint(self.checkpoint);
        match position {
            Position::Checkpoints { .. } => proto,
            Position::Transactions { .. } => proto.with_transaction_index(self.transaction_index),
            Position::Events { .. } => proto
                .with_transaction_index(self.transaction_index)
                .with_event_index(self.event_index),
        }
    }
}

/// Endpoint-specific cursor position. Transaction and event positions carry `public_position`,
/// the boundary immediately before the item at their coordinate in public ledger coordinates,
/// independent of the cursor's kind: an item cursor's item key, or the boundary a boundary cursor
/// names. It only labels the cursor: scans are bounded by `tx_seq` and `event_index` alone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Position {
    Checkpoints {
        checkpoint: u64,
    },
    Transactions {
        tx_seq: u64,
        public_position: LedgerPoint,
    },
    Events {
        tx_seq: u64,
        event_index: u32,
        public_position: LedgerPoint,
    },
}

impl Position {
    /// This coordinate in public ledger coordinates: a checkpoint position's checkpoint start, or
    /// the public position a transaction or event position carries.
    pub fn public_position(&self) -> LedgerPoint {
        match *self {
            Position::Checkpoints { checkpoint } => LedgerPoint::checkpoint_start(checkpoint),
            Position::Transactions {
                public_position, ..
            }
            | Position::Events {
                public_position, ..
            } => public_position,
        }
    }

    /// The resume point of a frame carrying the item at this position, whose public position is
    /// the item's key: ascending, the boundary right after the item (the key with its lane's
    /// finest index incremented, never carried into the next transaction or checkpoint);
    /// descending, the key.
    pub fn item_resume(&self, direction: ScanDirection) -> LedgerPoint {
        let key = self.public_position();
        if !direction.is_ascending() {
            return key;
        }
        // The finest index of a real item never reaches its numeric maximum; saturating keeps the
        // impossible edge, which a client's cursor can still name, from wrapping.
        match self {
            Position::Checkpoints { .. } => {
                LedgerPoint::checkpoint_start(key.checkpoint.saturating_add(1))
            }
            Position::Transactions { .. } => {
                LedgerPoint::transaction(key.checkpoint, key.transaction_index.saturating_add(1))
            }
            Position::Events { .. } => LedgerPoint::event(
                key.checkpoint,
                key.transaction_index,
                key.event_index.saturating_add(1),
            ),
        }
    }
}

/// Whether a cursor position is a matched row that was returned to the client (`Item`) or a scan
/// frontier the server reached (`Boundary`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CursorKind {
    Item,
    Boundary,
}

impl CursorToken {
    pub fn item(position: Position) -> Self {
        Self {
            kind: CursorKind::Item,
            position,
        }
    }

    pub fn boundary(position: Position) -> Self {
        Self {
            kind: CursorKind::Boundary,
            position,
        }
    }

    pub fn encode(&self) -> Bytes {
        grpc::CursorToken::from(self).encode_to_vec().into()
    }

    pub fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        Self::try_from(grpc::CursorToken::decode(bytes)?)
    }
}

impl CursorKind {
    fn to_proto(self) -> grpc::CursorKind {
        match self {
            CursorKind::Item => grpc::CursorKind::Item,
            CursorKind::Boundary => grpc::CursorKind::Boundary,
        }
    }

    fn from_proto(value: grpc::CursorKind) -> Option<Self> {
        match value {
            grpc::CursorKind::Item => Some(CursorKind::Item),
            grpc::CursorKind::Boundary => Some(CursorKind::Boundary),
            grpc::CursorKind::Unspecified => None,
        }
    }
}

impl From<LedgerPoint> for grpc::PublicPosition {
    fn from(point: LedgerPoint) -> Self {
        Self {
            checkpoint: point.checkpoint,
            transaction_index: point.transaction_index,
            event_index: point.event_index,
        }
    }
}

impl From<grpc::PublicPosition> for LedgerPoint {
    fn from(position: grpc::PublicPosition) -> Self {
        Self {
            checkpoint: position.checkpoint,
            transaction_index: position.transaction_index,
            event_index: position.event_index,
        }
    }
}

impl From<&CursorToken> for grpc::CursorToken {
    fn from(cursor: &CursorToken) -> Self {
        let (position, public_position) = match cursor.position {
            Position::Checkpoints { checkpoint } => (
                grpc::cursor_token::Position::Checkpoints(grpc::CheckpointsPosition {
                    checkpoint: Some(checkpoint),
                }),
                None,
            ),
            Position::Transactions {
                tx_seq,
                public_position,
            } => (
                grpc::cursor_token::Position::Transactions(grpc::TransactionsPosition {
                    tx_seq: Some(tx_seq),
                }),
                // A transaction cursor's position lies between or at transactions.
                Some(LedgerPoint::transaction(
                    public_position.checkpoint,
                    public_position.transaction_index,
                )),
            ),
            Position::Events {
                tx_seq,
                event_index,
                public_position,
            } => (
                grpc::cursor_token::Position::Events(grpc::EventsPosition {
                    tx_seq: Some(tx_seq),
                    event_index: Some(event_index),
                }),
                Some(public_position),
            ),
        };
        Self {
            kind: cursor.kind.to_proto() as i32,
            public_position: public_position.map(Into::into),
            position: Some(position),
        }
    }
}

/// A transaction or event cursor without `public_position` is invalid. A checkpoint cursor's
/// `public_position` is ignored: its checkpoint is already its public position. A transaction
/// cursor's position is a place between or at transactions, so its `public_position.event_index`
/// is ignored and decodes as 0.
impl TryFrom<grpc::CursorToken> for CursorToken {
    type Error = anyhow::Error;

    fn try_from(proto: grpc::CursorToken) -> anyhow::Result<Self> {
        let kind = grpc::CursorKind::try_from(proto.kind)
            .ok()
            .and_then(CursorKind::from_proto)
            .with_context(|| format!("unknown cursor kind: {}", proto.kind))?;
        let public_position = || -> anyhow::Result<LedgerPoint> {
            proto
                .public_position
                .map(Into::into)
                .context("cursor missing public_position")
        };
        let position = match proto.position.context("cursor missing position")? {
            grpc::cursor_token::Position::Checkpoints(position) => Position::Checkpoints {
                checkpoint: position.checkpoint.context("cursor missing checkpoint")?,
            },
            grpc::cursor_token::Position::Transactions(position) => {
                let tx_seq = position.tx_seq.context("cursor missing tx_seq")?;
                let point = public_position()?;
                Position::Transactions {
                    tx_seq,
                    public_position: LedgerPoint::transaction(
                        point.checkpoint,
                        point.transaction_index,
                    ),
                }
            }
            grpc::cursor_token::Position::Events(position) => Position::Events {
                tx_seq: position.tx_seq.context("cursor missing tx_seq")?,
                event_index: position.event_index.context("cursor missing event_index")?,
                public_position: public_position()?,
            },
        };
        Ok(Self { kind, position })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PUBLIC: LedgerPoint = LedgerPoint {
        checkpoint: 11,
        transaction_index: 2,
        event_index: 13,
    };

    fn token_bytes(
        public_position: Option<grpc::PublicPosition>,
        position: grpc::cursor_token::Position,
    ) -> Vec<u8> {
        grpc::CursorToken {
            kind: grpc::CursorKind::Item as i32,
            public_position,
            position: Some(position),
        }
        .encode_to_vec()
    }

    #[test]
    fn roundtrips_all_position_variants() {
        for token in [
            CursorToken::item(Position::Checkpoints { checkpoint: 7 }),
            CursorToken::boundary(Position::Transactions {
                tx_seq: 10,
                public_position: LedgerPoint::transaction(9, 4),
            }),
            CursorToken::item(Position::Events {
                tx_seq: 12,
                event_index: 13,
                public_position: PUBLIC,
            }),
            // A position at genesis encodes as an empty message, still present.
            CursorToken::boundary(Position::Events {
                tx_seq: 0,
                event_index: 0,
                public_position: LedgerPoint::GENESIS,
            }),
        ] {
            let bytes = token.encode();
            let decoded = CursorToken::decode(&bytes).unwrap();
            assert_eq!(decoded, token);
            assert_eq!(decoded.encode(), bytes, "re-encoding keeps the bytes");
        }
    }

    /// A cursor whose transaction or event position carries a checkpoint label in tag 1 decodes by
    /// its scan position and public position, and re-encodes without the label.
    #[test]
    fn decodes_cursors_carrying_a_checkpoint_label() {
        let cases: [(&[u8], CursorToken, &[u8]); 2] = [
            (
                // kind = ITEM; transactions { 1: 42, tx_seq: 7 };
                // public_position { checkpoint: 42, transaction_index: 3 }.
                &[
                    0x28, 0x01, 0x3a, 0x04, 0x08, 0x2a, 0x10, 0x07, 0x4a, 0x04, 0x08, 0x2a, 0x10,
                    0x03,
                ],
                CursorToken::item(Position::Transactions {
                    tx_seq: 7,
                    public_position: LedgerPoint::transaction(42, 3),
                }),
                &[
                    0x28, 0x01, 0x3a, 0x02, 0x10, 0x07, 0x4a, 0x04, 0x08, 0x2a, 0x10, 0x03,
                ],
            ),
            (
                // kind = ITEM; events { 1: 42, tx_seq: 7, event_index: 1 };
                // public_position { checkpoint: 42, transaction_index: 3, event_index: 1 }.
                &[
                    0x28, 0x01, 0x42, 0x06, 0x08, 0x2a, 0x10, 0x07, 0x18, 0x01, 0x4a, 0x06, 0x08,
                    0x2a, 0x10, 0x03, 0x18, 0x01,
                ],
                CursorToken::item(Position::Events {
                    tx_seq: 7,
                    event_index: 1,
                    public_position: LedgerPoint::event(42, 3, 1),
                }),
                &[
                    0x28, 0x01, 0x42, 0x04, 0x10, 0x07, 0x18, 0x01, 0x4a, 0x06, 0x08, 0x2a, 0x10,
                    0x03, 0x18, 0x01,
                ],
            ),
        ];
        for (labelled, expected, unlabelled) in cases {
            let token = CursorToken::decode(labelled).unwrap();
            assert_eq!(token, expected);
            assert_eq!(token.encode().as_ref(), unlabelled);
        }
    }

    /// A transaction cursor carrying a public `event_index` decodes and encodes without it,
    /// whatever its kind. An event cursor keeps it.
    #[test]
    fn transaction_cursors_drop_a_public_event_index() {
        for (kind, token) in [
            (0x01, CursorToken::item as fn(Position) -> CursorToken),
            (0x02, CursorToken::boundary),
        ] {
            // kind; transactions { tx_seq: 42 };
            // public_position { checkpoint: 7, transaction_index: 3, event_index: 9 }.
            let forged: &[u8] = &[
                0x28, kind, 0x3a, 0x02, 0x10, 0x2a, 0x4a, 0x06, 0x08, 0x07, 0x10, 0x03, 0x18, 0x09,
            ];
            // The same cursor with public_position { checkpoint: 7, transaction_index: 3 }.
            let projected: &[u8] = &[
                0x28, kind, 0x3a, 0x02, 0x10, 0x2a, 0x4a, 0x04, 0x08, 0x07, 0x10, 0x03,
            ];
            let decoded = CursorToken::decode(forged).unwrap();
            assert_eq!(
                decoded,
                token(Position::Transactions {
                    tx_seq: 42,
                    public_position: LedgerPoint::transaction(7, 3),
                })
            );
            assert_eq!(decoded.encode().as_ref(), projected);
            let minted = token(Position::Transactions {
                tx_seq: 42,
                public_position: LedgerPoint::event(7, 3, 9),
            });
            assert_eq!(minted.encode().as_ref(), projected);
        }

        let event = CursorToken::item(Position::Events {
            tx_seq: 42,
            event_index: 9,
            public_position: LedgerPoint::event(7, 3, 9),
        });
        assert_eq!(CursorToken::decode(&event.encode()).unwrap(), event);
    }

    #[test]
    fn rejects_transaction_and_event_cursors_without_a_public_position() {
        let transactions = token_bytes(
            None,
            grpc::cursor_token::Position::Transactions(grpc::TransactionsPosition {
                tx_seq: Some(10),
            }),
        );
        assert!(CursorToken::decode(&transactions).is_err());

        let events = token_bytes(
            None,
            grpc::cursor_token::Position::Events(grpc::EventsPosition {
                tx_seq: Some(10),
                event_index: Some(3),
            }),
        );
        assert!(CursorToken::decode(&events).is_err());
    }

    /// A checkpoint cursor's checkpoint is its public position; one it carries is ignored.
    #[test]
    fn checkpoint_cursors_ignore_a_public_position() {
        let bytes = token_bytes(
            Some(PUBLIC.into()),
            grpc::cursor_token::Position::Checkpoints(grpc::CheckpointsPosition {
                checkpoint: Some(5),
            }),
        );
        let token = CursorToken::decode(&bytes).unwrap();
        assert_eq!(
            token,
            CursorToken::item(Position::Checkpoints { checkpoint: 5 })
        );
        assert_eq!(
            token.position.public_position(),
            LedgerPoint::checkpoint_start(5)
        );
    }

    #[test]
    fn rejects_empty_token() {
        assert!(CursorToken::decode(&[]).is_err());
    }

    /// Tokens in the schema that used tags 1-4 decode as a token with no kind or position.
    #[test]
    fn rejects_tokens_in_the_reserved_schema() {
        // query_type = TRANSACTIONS, kind = ITEM, checkpoint = 42, position = 7.
        let bytes = [0x08, 0x02, 0x10, 0x01, 0x18, 0x2a, 0x20, 0x07];
        assert!(CursorToken::decode(&bytes).is_err());
    }

    #[test]
    fn rejects_unspecified_kind() {
        let bytes = grpc::CursorToken {
            kind: grpc::CursorKind::Unspecified as i32,
            public_position: Some(PUBLIC.into()),
            position: Some(grpc::cursor_token::Position::Transactions(
                grpc::TransactionsPosition { tx_seq: Some(2) },
            )),
        }
        .encode_to_vec();

        assert!(CursorToken::decode(&bytes).is_err());
    }

    #[test]
    fn rejects_missing_position() {
        let bytes = grpc::CursorToken {
            kind: grpc::CursorKind::Item as i32,
            public_position: Some(grpc::PublicPosition::default()),
            position: None,
        }
        .encode_to_vec();

        assert!(CursorToken::decode(&bytes).is_err());
    }

    #[test]
    fn rejects_missing_inner_fields() {
        let events = token_bytes(
            Some(PUBLIC.into()),
            grpc::cursor_token::Position::Events(grpc::EventsPosition {
                tx_seq: Some(2),
                event_index: None,
            }),
        );
        assert!(CursorToken::decode(&events).is_err());

        let checkpoints = token_bytes(
            None,
            grpc::cursor_token::Position::Checkpoints(grpc::CheckpointsPosition {
                checkpoint: None,
            }),
        );
        assert!(CursorToken::decode(&checkpoints).is_err());
    }

    #[test]
    fn item_resume_is_above_the_key_ascending_and_the_key_descending() {
        for (item, key, above) in [
            (
                Position::Checkpoints { checkpoint: 4 },
                LedgerPoint::checkpoint_start(4),
                LedgerPoint::checkpoint_start(5),
            ),
            (
                Position::Transactions {
                    tx_seq: 9,
                    public_position: LedgerPoint::transaction(4, 2),
                },
                LedgerPoint::transaction(4, 2),
                LedgerPoint::transaction(4, 3),
            ),
            (
                Position::Events {
                    tx_seq: 9,
                    event_index: 1,
                    public_position: LedgerPoint::event(4, 2, 1),
                },
                LedgerPoint::event(4, 2, 1),
                LedgerPoint::event(4, 2, 2),
            ),
        ] {
            assert_eq!(item.item_resume(ScanDirection::Ascending), above);
            assert_eq!(item.item_resume(ScanDirection::Descending), key);
        }
    }

    /// The lane comes from the cursor position; its own coordinates don't matter.
    #[test]
    fn to_ledger_position_sets_only_the_lane_coordinates() {
        let point = LedgerPoint::event(4, 2, 1);
        let checkpoint = point.to_ledger_position(&Position::Checkpoints { checkpoint: 0 });
        assert_eq!(
            (
                checkpoint.checkpoint,
                checkpoint.transaction_index,
                checkpoint.event_index
            ),
            (Some(4), None, None)
        );
        let transaction = point.to_ledger_position(&Position::Transactions {
            tx_seq: 0,
            public_position: LedgerPoint::GENESIS,
        });
        assert_eq!(
            (
                transaction.checkpoint,
                transaction.transaction_index,
                transaction.event_index
            ),
            (Some(4), Some(2), None)
        );
        let event = point.to_ledger_position(&Position::Events {
            tx_seq: 0,
            event_index: 0,
            public_position: LedgerPoint::GENESIS,
        });
        assert_eq!(
            (event.checkpoint, event.transaction_index, event.event_index),
            (Some(4), Some(2), Some(1))
        );
    }
}
