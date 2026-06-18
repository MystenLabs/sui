// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bytes::Bytes;
use serde::Deserialize;
use serde::Serialize;

/// Pagination cursor for the bitmap-backed ledger-history endpoints.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CursorToken {
    pub query_type: QueryType,
    pub kind: CursorKind,
    pub checkpoint: u64,
    pub position: u64,
}

/// The ledger-history query the cursor was minted for.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum QueryType {
    Checkpoints,
    Transactions,
    Events,
}

/// Whether a cursor position is a matched row that was returned to the client (`Item`) or a scan
/// frontier the server reached (`Boundary`).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
        bcs::to_bytes(self)
            .expect("CursorToken should serialize")
            .into()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, bcs::Error> {
        bcs::from_bytes(bytes)
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
