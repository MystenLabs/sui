// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use bytes::Bytes;
use prost::Message as _;

mod proto;

use proto::sui::rpc::cursor::v1 as grpc;

/// Pagination cursor for ledger-history endpoints.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorToken {
    pub kind: CursorKind,
    pub position: Position,
}

/// Endpoint-specific cursor position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Position {
    Checkpoints { checkpoint: u64 },
    Transactions { tx_seq: u64 },
    Events { tx_seq: u64, event_index: u32 },
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
        // Current format first. Legacy bytes also decode "successfully" here —
        // their tags 1-4 are reserved, so prost skips them as unknown fields —
        // but the resulting all-defaults message fails validation, sending us
        // to the fallback. Bytes that fail both surface the current-format
        // error.
        let token = grpc::CursorToken::decode(bytes)?;
        match Self::try_from(token) {
            Ok(token) => Ok(token),
            Err(err) => Self::decode_legacy(bytes).map_err(|_| err),
        }
    }

    /// Decode-only compatibility with the pre-explicit-coordinate schema
    /// (tags 1-4, now reserved): checkpoint, transaction, and event cursors
    /// minted before the format change keep resuming. The event arm hard-codes
    /// the frozen historical bitpacking rather than importing the storage crate.
    /// The legacy `checkpoint` field (tag 3) is ignored: every position is
    /// recovered from the legacy `position` field. Nothing mints this format.
    fn decode_legacy(bytes: &[u8]) -> anyhow::Result<Self> {
        let legacy = LegacyCursorToken::decode(bytes)?;
        let kind = match legacy.kind {
            1 => CursorKind::Item,
            2 => CursorKind::Boundary,
            k => anyhow::bail!("unknown legacy cursor kind: {k}"),
        };
        let position = legacy.position.context("legacy cursor missing position")?;
        let position = match legacy.query_type {
            // Legacy checkpoint cursors minted `checkpoint == position`; use
            // `position`, the coordinate old scans resumed from.
            1 => Position::Checkpoints {
                checkpoint: position,
            },
            2 => Position::Transactions { tx_seq: position },
            // Legacy event cursors packed the coordinate into one u64:
            // `(tx_seq << 16) | event_index`. The constant is the FROZEN
            // historical wire format, deliberately not shared with the live
            // storage encoding, which is free to diverge.
            3 => Position::Events {
                tx_seq: position >> 16,
                event_index: (position & 0xFFFF) as u32,
            },
            q => anyhow::bail!("unknown legacy cursor query_type: {q}"),
        };
        Ok(Self { kind, position })
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

impl From<Position> for grpc::cursor_token::Position {
    fn from(position: Position) -> Self {
        match position {
            Position::Checkpoints { checkpoint } => {
                grpc::cursor_token::Position::Checkpoints(grpc::CheckpointsPosition {
                    checkpoint: Some(checkpoint),
                })
            }
            Position::Transactions { tx_seq } => {
                grpc::cursor_token::Position::Transactions(grpc::TransactionsPosition {
                    tx_seq: Some(tx_seq),
                })
            }
            Position::Events {
                tx_seq,
                event_index,
            } => grpc::cursor_token::Position::Events(grpc::EventsPosition {
                tx_seq: Some(tx_seq),
                event_index: Some(event_index),
            }),
        }
    }
}

impl TryFrom<grpc::cursor_token::Position> for Position {
    type Error = anyhow::Error;

    fn try_from(position: grpc::cursor_token::Position) -> anyhow::Result<Self> {
        match position {
            grpc::cursor_token::Position::Checkpoints(position) => Ok(Position::Checkpoints {
                checkpoint: position.checkpoint.context("cursor missing checkpoint")?,
            }),
            grpc::cursor_token::Position::Transactions(position) => Ok(Position::Transactions {
                tx_seq: position.tx_seq.context("cursor missing tx_seq")?,
            }),
            grpc::cursor_token::Position::Events(position) => Ok(Position::Events {
                tx_seq: position.tx_seq.context("cursor missing tx_seq")?,
                event_index: position.event_index.context("cursor missing event_index")?,
            }),
        }
    }
}

impl From<&CursorToken> for grpc::CursorToken {
    fn from(cursor: &CursorToken) -> Self {
        Self {
            kind: cursor.kind.to_proto() as i32,
            position: Some(cursor.position.into()),
        }
    }
}

impl TryFrom<grpc::CursorToken> for CursorToken {
    type Error = anyhow::Error;

    fn try_from(proto: grpc::CursorToken) -> anyhow::Result<Self> {
        let kind = grpc::CursorKind::try_from(proto.kind)
            .ok()
            .and_then(CursorKind::from_proto)
            .with_context(|| format!("unknown cursor kind: {}", proto.kind))?;
        let position = proto
            .position
            .context("cursor missing position")?
            .try_into()?;
        Ok(Self { kind, position })
    }
}

/// Wire mirror of the previous `CursorToken` schema (tags 1-4, reserved in the
/// current message; disjoint from the current fields 5-8, which is what makes
/// the decode fallback unambiguous in both directions). Private and
/// decode-only: deliberately absent from `cursor.proto`, which stays the spec
/// for third-party implementations.
#[derive(Clone, PartialEq, prost::Message)]
struct LegacyCursorToken {
    /// Legacy QueryType: 1 = checkpoints, 2 = transactions, 3 = events.
    #[prost(int32, tag = "1")]
    query_type: i32,
    /// Legacy CursorKind: 1 = item, 2 = boundary.
    #[prost(int32, tag = "2")]
    kind: i32,
    /// Ignored on decode: positions come from `position` alone.
    #[prost(uint64, optional, tag = "3")]
    checkpoint: Option<u64>,
    #[prost(uint64, optional, tag = "4")]
    position: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_all_position_variants() {
        for token in [
            CursorToken::item(Position::Checkpoints { checkpoint: 7 }),
            CursorToken::boundary(Position::Transactions { tx_seq: 10 }),
            CursorToken::item(Position::Events {
                tx_seq: 12,
                event_index: 13,
            }),
        ] {
            assert_eq!(CursorToken::decode(&token.encode()).unwrap(), token);
        }
    }

    /// Cursors minted while `TransactionsPosition` and `EventsPosition` carried a checkpoint in
    /// tag 1 decode to the same position: the reserved tag is skipped.
    #[test]
    fn decodes_cursors_carrying_reserved_checkpoint_tag() {
        // kind (tag 5) = ITEM;
        // transactions (tag 7) = { checkpoint (tag 1) = 9, tx_seq (tag 2) = 10 }.
        let transactions = [0x28, 0x01, 0x3a, 0x04, 0x08, 0x09, 0x10, 0x0a];
        assert_eq!(
            CursorToken::decode(&transactions).unwrap(),
            CursorToken::item(Position::Transactions { tx_seq: 10 })
        );

        // kind (tag 5) = BOUNDARY;
        // events (tag 8) = { checkpoint (tag 1) = 11, tx_seq (tag 2) = 12,
        //                    event_index (tag 3) = 13 }.
        let events = [0x28, 0x02, 0x42, 0x06, 0x08, 0x0b, 0x10, 0x0c, 0x18, 0x0d];
        assert_eq!(
            CursorToken::decode(&events).unwrap(),
            CursorToken::boundary(Position::Events {
                tx_seq: 12,
                event_index: 13,
            })
        );
    }

    #[test]
    fn rejects_empty_token() {
        assert!(CursorToken::decode(&[]).is_err());
    }

    #[test]
    fn rejects_unspecified_kind() {
        let bytes = grpc::CursorToken {
            kind: grpc::CursorKind::Unspecified as i32,
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
            position: None,
        }
        .encode_to_vec();

        assert!(CursorToken::decode(&bytes).is_err());
    }

    #[test]
    fn rejects_missing_inner_fields() {
        let bytes = grpc::CursorToken {
            kind: grpc::CursorKind::Item as i32,
            position: Some(grpc::cursor_token::Position::Events(grpc::EventsPosition {
                tx_seq: Some(2),
                event_index: None,
            })),
        }
        .encode_to_vec();

        assert!(CursorToken::decode(&bytes).is_err());
    }

    #[test]
    fn decodes_legacy_transactions_cursor() {
        // query_type=TRANSACTIONS(2), kind=ITEM(1), checkpoint=42, position=7,
        // exactly as the previous schema serialized them.
        let bytes = [0x08, 0x02, 0x10, 0x01, 0x18, 0x2a, 0x20, 0x07];
        assert_eq!(
            CursorToken::decode(&bytes).unwrap(),
            CursorToken::item(Position::Transactions { tx_seq: 7 })
        );
    }

    #[test]
    fn decodes_legacy_checkpoints_cursor() {
        // query_type=CHECKPOINTS(1), kind=BOUNDARY(2); legacy checkpoint
        // cursors carried the cp_seq in both fields.
        let bytes = [0x08, 0x01, 0x10, 0x02, 0x18, 0x05, 0x20, 0x05];
        assert_eq!(
            CursorToken::decode(&bytes).unwrap(),
            CursorToken::boundary(Position::Checkpoints { checkpoint: 5 })
        );
    }

    #[test]
    fn decodes_legacy_events_cursor() {
        let bytes = LegacyCursorToken {
            query_type: 3,
            kind: 1,
            checkpoint: Some(9),
            position: Some((42u64 << 16) | 7),
        }
        .encode_to_vec();
        assert_eq!(
            CursorToken::decode(&bytes).unwrap(),
            CursorToken::item(Position::Events {
                tx_seq: 42,
                event_index: 7,
            })
        );
    }

    #[test]
    fn decodes_legacy_events_boundary_fencepost() {
        let bytes = LegacyCursorToken {
            query_type: 3,
            kind: 2,
            checkpoint: Some(6),
            position: Some((5u64 << 16) | 0xFFFF),
        }
        .encode_to_vec();
        assert_eq!(
            CursorToken::decode(&bytes).unwrap(),
            CursorToken::boundary(Position::Events {
                tx_seq: 5,
                event_index: 65535,
            })
        );
    }

    #[test]
    fn rejects_legacy_token_with_missing_fields() {
        // Discriminant present but coordinates absent — must not become a
        // "position 0" cursor.
        let bytes = LegacyCursorToken {
            query_type: 2,
            kind: 1,
            checkpoint: None,
            position: None,
        }
        .encode_to_vec();
        assert!(CursorToken::decode(&bytes).is_err());
    }
}
