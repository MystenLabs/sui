// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::crypto::AuthorityStrongQuorumSignInfo;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSummary;

/// A checkpoint received from gRPC with pre-deserialized data for subscriber consumption.
pub(crate) struct ProcessedCheckpoint {
    pub(crate) sequence_number: u64,
    pub(crate) summary: CheckpointSummary,
    pub(crate) contents: CheckpointContents,
    pub(crate) signature: AuthorityStrongQuorumSignInfo,
}
