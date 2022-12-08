// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use serde::{Deserialize, Serialize};
use types::{Round, SequenceNumber};

/// The state of the subscriber keeping track of the transactions that have already been
/// executed. It ensures we do not process twice the same transaction despite crash-recovery.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq, Copy)]
pub struct ExecutionIndices {
    /// The round number of the last committed leader.
    pub last_committed_round: Round,
    /// The index of the last sub-DAG that was executed (either fully or partially).
    pub sub_dag_index: SequenceNumber,
    /// The index of the last transaction was executed (used for crash-recovery).
    pub transaction_index: SequenceNumber,
}

impl ExecutionIndices {
    pub fn end_for_commit(commit_round: u64) -> Self {
        ExecutionIndices {
            last_committed_round: commit_round,
            sub_dag_index: SequenceNumber::MAX,
            transaction_index: SequenceNumber::MAX,
        }
    }
}

impl Ord for ExecutionIndices {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (
            self.last_committed_round,
            self.sub_dag_index,
            self.transaction_index,
        )
            .cmp(&(
                other.last_committed_round,
                other.sub_dag_index,
                other.transaction_index,
            ))
    }
}

impl PartialOrd for ExecutionIndices {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
