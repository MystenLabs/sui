// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::kv_checkpoints;
use diesel::prelude::*;
use sui_field_count::FieldCount;

#[derive(Insertable, Debug, Clone, FieldCount)]
#[diesel(table_name = kv_checkpoints)]
pub struct StoredCheckpoint {
    pub sequence_number: i64,
    /// BCS serialized CertifiedCheckpointSummary
    pub certified_checkpoint: Vec<u8>,
    /// BCS serialized CheckpointContents
    pub checkpoint_contents: Vec<u8>,
}
