// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

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
    /// Index of `transactions` by digest for O(1) lookup that scales across many subscribers.
    /// Built once at construction.
    by_digest: HashMap<TransactionDigest, usize>,
}

/// A transaction from a streamed checkpoint with pre-deserialized contents.
pub(crate) struct ProcessedTransaction {
    pub(crate) tx_sequence_number: u64,
    pub(crate) digest: TransactionDigest,
    pub(crate) contents: TransactionContents,
}

impl ProcessedCheckpoint {
    /// Construct a checkpoint with the digest index pre-built from `transactions`.
    pub(crate) fn new(
        sequence_number: u64,
        summary: CheckpointSummary,
        contents: CheckpointContents,
        signature: AuthorityStrongQuorumSignInfo,
        transactions: Vec<ProcessedTransaction>,
        execution_objects: ExecutionObjectMap,
    ) -> Self {
        let by_digest = transactions
            .iter()
            .enumerate()
            .map(|(i, t)| (t.digest, i))
            .collect();
        Self {
            sequence_number,
            summary,
            contents,
            signature,
            transactions,
            execution_objects,
            by_digest,
        }
    }

    /// Lookup a transaction by digest within this checkpoint.
    pub(crate) fn transaction_by_digest(
        &self,
        digest: TransactionDigest,
    ) -> Option<&ProcessedTransaction> {
        let i = *self.by_digest.get(&digest)?;
        self.transactions.get(i)
    }
}
