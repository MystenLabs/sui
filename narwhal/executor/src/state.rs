// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use serde::{Deserialize, Serialize};
use types::SequenceNumber;

/// The state of the subscriber keeping track of the transactions that have already been
/// executed. It ensures we do not process twice the same transaction despite crash-recovery.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionIndices {
    /// The index of the latest consensus message we processed (used for crash-recovery).
    pub next_certificate_index: SequenceNumber,
    /// The index of the last batch we executed (used for crash-recovery).
    pub next_batch_index: SequenceNumber,
    /// The index of the last transaction we executed (used for crash-recovery).
    pub next_transaction_index: SequenceNumber,
}

impl Ord for ExecutionIndices {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (
            self.next_certificate_index,
            self.next_batch_index,
            self.next_transaction_index,
        )
            .cmp(&(
                other.next_certificate_index,
                other.next_batch_index,
                other.next_transaction_index,
            ))
    }
}

impl PartialOrd for ExecutionIndices {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
