// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_indexer_alt_reader::kv_loader::TransactionContents;
use sui_types::crypto::AuthorityStrongQuorumSignInfo;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSummary;

use crate::scope::ExecutionObjectMap;

/// A checkpoint received from gRPC with pre-deserialized data for subscriber consumption.
pub(crate) struct ProcessedCheckpoint {
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
    /// Wrapped in `Arc` so that subscribers (and resolvers within them) share a single deep
    /// copy of the per-tx contents instead of each cloning the whole `TransactionContents`.
    pub(crate) contents: Arc<TransactionContents>,
}

#[cfg(test)]
impl ProcessedCheckpoint {
    /// A checkpoint for tests where only `sequence_number` and `network_total_transactions` carry
    /// meaning; the remaining fields are empty or default.
    pub(crate) fn for_test(sequence_number: u64, network_total_transactions: u64) -> Self {
        use std::collections::BTreeMap;

        use sui_types::crypto::AggregateAuthoritySignature;
        use sui_types::gas::GasCostSummary;

        let contents = CheckpointContents::new_with_digests_only_for_tests(vec![]);
        let summary = CheckpointSummary {
            epoch: 0,
            sequence_number,
            network_total_transactions,
            content_digest: *contents.digest(),
            previous_digest: None,
            epoch_rolling_gas_cost_summary: GasCostSummary::default(),
            timestamp_ms: 0,
            checkpoint_commitments: vec![],
            end_of_epoch_data: None,
            version_specific_data: vec![],
        };
        let signature = AuthorityStrongQuorumSignInfo {
            epoch: 0,
            signature: AggregateAuthoritySignature::default(),
            signers_map: Default::default(),
        };
        Self {
            summary,
            contents,
            signature,
            transactions: vec![],
            execution_objects: Arc::new(BTreeMap::new()),
        }
    }
}
