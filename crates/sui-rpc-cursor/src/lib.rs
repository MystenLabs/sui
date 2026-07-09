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
}
