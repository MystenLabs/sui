// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use bytes::Bytes;
use prost::Message as _;

mod proto;

use proto::sui::rpc::cursor::v1::CursorToken as ProtoCursorToken;

/// Pagination cursor for the bitmap-backed ledger-history endpoints.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorToken {
    pub query_type: QueryType,
    pub kind: CursorKind,
    pub checkpoint: u64,
    pub position: u64,
}

/// The ledger-history query the cursor was minted for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryType {
    Checkpoints,
    Transactions,
    Events,
}

/// Whether a cursor position is a matched row that was returned to the client (`Item`) or a scan
/// frontier the server reached (`Boundary`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CursorKind {
    Item,
    Boundary,
}

impl CursorToken {
    pub fn item(query_type: QueryType, checkpoint: u64, position: u64) -> Self {
        Self {
            query_type,
            kind: CursorKind::Item,
            checkpoint,
            position,
        }
    }

    pub fn boundary(query_type: QueryType, checkpoint: u64, position: u64) -> Self {
        Self {
            query_type,
            kind: CursorKind::Boundary,
            checkpoint,
            position,
        }
    }

    pub fn encode(&self) -> Bytes {
        ProtoCursorToken::from(self).encode_to_vec().into()
    }

    pub fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        Self::try_from(ProtoCursorToken::decode(bytes)?)
    }

    pub fn after_position_start(&self) -> Option<u64> {
        match self.kind {
            CursorKind::Item => self.position.checked_add(1),
            CursorKind::Boundary => Some(self.position),
        }
    }

    pub fn before_checkpoint_end(&self) -> Option<u64> {
        match self.kind {
            CursorKind::Item => self.checkpoint.checked_add(1),
            CursorKind::Boundary => Some(self.checkpoint),
        }
    }
}

impl QueryType {
    fn to_proto(self) -> u32 {
        match self {
            QueryType::Checkpoints => 1,
            QueryType::Transactions => 2,
            QueryType::Events => 3,
        }
    }

    fn from_proto(value: u32) -> Option<Self> {
        Some(match value {
            1 => QueryType::Checkpoints,
            2 => QueryType::Transactions,
            3 => QueryType::Events,
            _ => return None,
        })
    }
}

impl CursorKind {
    fn to_proto(self) -> u32 {
        match self {
            CursorKind::Item => 1,
            CursorKind::Boundary => 2,
        }
    }

    fn from_proto(value: u32) -> Option<Self> {
        Some(match value {
            1 => CursorKind::Item,
            2 => CursorKind::Boundary,
            _ => return None,
        })
    }
}

impl From<&CursorToken> for ProtoCursorToken {
    fn from(cursor: &CursorToken) -> Self {
        Self {
            query_type: cursor.query_type.to_proto(),
            kind: cursor.kind.to_proto(),
            checkpoint: cursor.checkpoint,
            position: cursor.position,
        }
    }
}

impl TryFrom<ProtoCursorToken> for CursorToken {
    type Error = anyhow::Error;

    fn try_from(proto: ProtoCursorToken) -> anyhow::Result<Self> {
        Ok(Self {
            query_type: QueryType::from_proto(proto.query_type)
                .with_context(|| format!("unknown cursor query_type: {}", proto.query_type))?,
            kind: CursorKind::from_proto(proto.kind)
                .with_context(|| format!("unknown cursor kind: {}", proto.kind))?,
            checkpoint: proto.checkpoint,
            position: proto.position,
        })
    }
}
