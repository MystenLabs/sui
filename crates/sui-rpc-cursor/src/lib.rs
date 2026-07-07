// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use bytes::Bytes;
use prost::Message as _;

mod proto;

use proto::sui::rpc::cursor::v1 as grpc;

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
        grpc::CursorToken::from(self).encode_to_vec().into()
    }

    pub fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        Self::try_from(grpc::CursorToken::decode(bytes)?)
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
    fn to_proto(self) -> grpc::QueryType {
        match self {
            QueryType::Checkpoints => grpc::QueryType::Checkpoints,
            QueryType::Transactions => grpc::QueryType::Transactions,
            QueryType::Events => grpc::QueryType::Events,
        }
    }

    fn from_proto(value: grpc::QueryType) -> Option<Self> {
        match value {
            grpc::QueryType::Checkpoints => Some(QueryType::Checkpoints),
            grpc::QueryType::Transactions => Some(QueryType::Transactions),
            grpc::QueryType::Events => Some(QueryType::Events),
            grpc::QueryType::Unspecified => None,
        }
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

impl From<&CursorToken> for grpc::CursorToken {
    fn from(cursor: &CursorToken) -> Self {
        Self {
            query_type: cursor.query_type.to_proto() as i32,
            kind: cursor.kind.to_proto() as i32,
            checkpoint: Some(cursor.checkpoint),
            position: Some(cursor.position),
        }
    }
}

impl TryFrom<grpc::CursorToken> for CursorToken {
    type Error = anyhow::Error;

    fn try_from(proto: grpc::CursorToken) -> anyhow::Result<Self> {
        let query_type = grpc::QueryType::try_from(proto.query_type)
            .ok()
            .and_then(QueryType::from_proto)
            .with_context(|| format!("unknown cursor query_type: {}", proto.query_type))?;
        let kind = grpc::CursorKind::try_from(proto.kind)
            .ok()
            .and_then(CursorKind::from_proto)
            .with_context(|| format!("unknown cursor kind: {}", proto.kind))?;
        let checkpoint = proto.checkpoint.context("cursor missing checkpoint")?;
        let position = proto.position.context("cursor missing position")?;
        Ok(Self {
            query_type,
            kind,
            checkpoint,
            position,
        })
    }
}
