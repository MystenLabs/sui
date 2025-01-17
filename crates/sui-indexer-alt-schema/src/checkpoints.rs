// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use diesel::prelude::*;
use sui_field_count::FieldCount;
use sui_protocol_config::{Chain, ProtocolVersion};
use sui_types::digests::{ChainIdentifier, CheckpointDigest};

use crate::schema::{kv_checkpoints, kv_genesis};

#[derive(Insertable, Debug, Clone, FieldCount, Queryable)]
#[diesel(table_name = kv_checkpoints)]
pub struct StoredCheckpoint {
    pub sequence_number: i64,
    /// BCS serialized CheckpointContents
    pub checkpoint_contents: Vec<u8>,
    /// BCS serialized CheckpointSummary
    pub checkpoint_summary: Vec<u8>,
    /// BCS serialized AuthorityQuorumSignInfo
    pub validator_signatures: Vec<u8>,
}

#[derive(Insertable, Selectable, Queryable, Debug, Clone)]
#[diesel(table_name = kv_genesis)]
pub struct StoredGenesis {
    pub genesis_digest: Vec<u8>,
    pub initial_protocol_version: i64,
}

impl StoredGenesis {
    /// Try and identify the chain that this indexer is indexing based on its genesis checkpoint
    /// digest.
    pub fn chain(&self) -> Result<Chain> {
        let bytes: [u8; 32] = self
            .genesis_digest
            .clone()
            .try_into()
            .map_err(|_| anyhow!("Bad genesis digest"))?;

        let digest = CheckpointDigest::new(bytes);
        let identifier = ChainIdentifier::from(digest);

        Ok(identifier.chain())
    }

    /// The protocol version that the chain was started at.
    pub fn initial_protocol_version(&self) -> ProtocolVersion {
        ProtocolVersion::new(self.initial_protocol_version as u64)
    }
}
