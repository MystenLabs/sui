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
    Checkpoints {
        checkpoint: u64,
    },
    Transactions {
        checkpoint: u64,
        tx_seq: u64,
    },
    Events {
        checkpoint: u64,
        tx_seq: u64,
        event_index: u32,
    },
}

impl Position {
    pub fn checkpoint(&self) -> u64 {
        match *self {
            Position::Checkpoints { checkpoint }
            | Position::Transactions { checkpoint, .. }
            | Position::Events { checkpoint, .. } => checkpoint,
        }
    }

    /// The same position with its `checkpoint` coordinate replaced; the
    /// scalar coordinates are preserved.
    pub fn with_checkpoint(self, checkpoint: u64) -> Self {
        match self {
            Position::Checkpoints { .. } => Position::Checkpoints { checkpoint },
            Position::Transactions { tx_seq, .. } => Position::Transactions { checkpoint, tx_seq },
            Position::Events {
                tx_seq,
                event_index,
                ..
            } => Position::Events {
                checkpoint,
                tx_seq,
                event_index,
            },
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
    /// Nothing mints this format.
    fn decode_legacy(bytes: &[u8]) -> anyhow::Result<Self> {
        let legacy = LegacyCursorToken::decode(bytes)?;
        let kind = match legacy.kind {
            1 => CursorKind::Item,
            2 => CursorKind::Boundary,
            k => anyhow::bail!("unknown legacy cursor kind: {k}"),
        };
        let checkpoint = legacy
            .checkpoint
            .context("legacy cursor missing checkpoint")?;
        let position = legacy.position.context("legacy cursor missing position")?;
        let position = match legacy.query_type {
            // Legacy checkpoint cursors minted `checkpoint == position`; use
            // `position`, the coordinate old scans resumed from.
            1 => Position::Checkpoints {
                checkpoint: position,
            },
            2 => Position::Transactions {
                checkpoint,
                tx_seq: position,
            },
            // Legacy event cursors packed the coordinate into one u64:
            // `(tx_seq << 16) | event_index`. The constant is the FROZEN
            // historical wire format, deliberately not shared with the live
            // storage encoding, which is free to diverge.
            3 => Position::Events {
                checkpoint,
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
            Position::Transactions { checkpoint, tx_seq } => {
                grpc::cursor_token::Position::Transactions(grpc::TransactionsPosition {
                    checkpoint: Some(checkpoint),
                    tx_seq: Some(tx_seq),
                })
            }
            Position::Events {
                checkpoint,
                tx_seq,
                event_index,
            } => grpc::cursor_token::Position::Events(grpc::EventsPosition {
                checkpoint: Some(checkpoint),
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
                checkpoint: position.checkpoint.context("cursor missing checkpoint")?,
                tx_seq: position.tx_seq.context("cursor missing tx_seq")?,
            }),
            grpc::cursor_token::Position::Events(position) => Ok(Position::Events {
                checkpoint: position.checkpoint.context("cursor missing checkpoint")?,
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
            CursorToken::boundary(Position::Transactions {
                checkpoint: 9,
                tx_seq: 10,
            }),
            CursorToken::item(Position::Events {
                checkpoint: 11,
                tx_seq: 12,
                event_index: 13,
            }),
        ] {
            assert_eq!(CursorToken::decode(&token.encode()).unwrap(), token);
        }
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
                grpc::TransactionsPosition {
                    checkpoint: Some(1),
                    tx_seq: Some(2),
                },
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
                checkpoint: Some(1),
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
            CursorToken::item(Position::Transactions {
                checkpoint: 42,
                tx_seq: 7,
            })
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
                checkpoint: 9,
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
                checkpoint: 6,
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
