// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

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
    /// Checkpoint-wide execution objects (inputs and outputs across all transactions in the
    /// checkpoint, including tombstones for deleted/wrapped objects). Object visibility in a
    /// streamed scope is end-of-checkpoint, matching what the indexed Query API exposes.
    pub(crate) execution_objects: ExecutionObjectMap,
}

/// A transaction from a streamed checkpoint with pre-deserialized contents.
pub(crate) struct ProcessedTransaction {
    pub(crate) tx_sequence_number: u64,
    pub(crate) digest: TransactionDigest,
    /// Wrapped in `Arc` so that subscribers (and resolvers within them) share a single deep
    /// copy of the per-tx contents instead of each cloning the whole `TransactionContents`.
    pub(crate) contents: Arc<TransactionContents>,
}
