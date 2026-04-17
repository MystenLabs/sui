// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_reader::kv_loader::TransactionContents;
use sui_types::crypto::AuthorityStrongQuorumSignInfo;
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSummary;

use crate::scope::ExecutionObjectMap;

/// A checkpoint received from gRPC with pre-deserialized data for subscriber consumption.
pub(crate) struct ProcessedCheckpoint {
    pub(crate) sequence_number: u64,
    pub(crate) summary: CheckpointSummary,
    pub(crate) contents: CheckpointContents,
    pub(crate) signature: AuthorityStrongQuorumSignInfo,
    pub(crate) transactions: Vec<ProcessedTransaction>,
}

/// A transaction from a streamed checkpoint with pre-deserialized contents.
pub(crate) struct ProcessedTransaction {
    pub(crate) tx_sequence_number: u64,
    pub(crate) digest: TransactionDigest,
    pub(crate) contents: TransactionContents,
    /// Pre-built execution objects for this transaction, extracted from checkpoint-level objects.
    pub(crate) execution_objects: ExecutionObjectMap,
}
