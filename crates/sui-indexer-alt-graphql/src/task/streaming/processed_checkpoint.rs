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
    /// Index of `transactions` by global `tx_sequence_number` for O(1) lookup that scales
    /// across many subscribers. Built once at construction.
    by_tx_sequence_number: HashMap<u64, usize>,
}

/// A transaction from a streamed checkpoint with pre-deserialized contents.
pub(crate) struct ProcessedTransaction {
    pub(crate) tx_sequence_number: u64,
    pub(crate) digest: TransactionDigest,
    pub(crate) contents: TransactionContents,
    /// Pre-built execution objects for this transaction, extracted from checkpoint-level objects.
    pub(crate) execution_objects: ExecutionObjectMap,
}

impl ProcessedCheckpoint {
    pub(crate) fn new(
        sequence_number: u64,
        summary: CheckpointSummary,
        contents: CheckpointContents,
        signature: AuthorityStrongQuorumSignInfo,
        transactions: Vec<ProcessedTransaction>,
    ) -> Self {
        let by_tx_sequence_number = transactions
            .iter()
            .enumerate()
            .map(|(i, t)| (t.tx_sequence_number, i))
            .collect();
        Self {
            sequence_number,
            summary,
            contents,
            signature,
            transactions,
            by_tx_sequence_number,
        }
    }

    /// Lookup a transaction by its global `tx_sequence_number` within this checkpoint.
    pub(crate) fn transaction(&self, tx_sequence_number: u64) -> Option<&ProcessedTransaction> {
        let i = *self.by_tx_sequence_number.get(&tx_sequence_number)?;
        self.transactions.get(i)
    }
}
