// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use serde::{Deserialize, Serialize};
use types::SequenceNumber;

#[cfg(test)]
#[path = "tests/state_tests.rs"]
pub mod state_tests;

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

impl ExecutionIndices {
    /// Compute the next expected indices.
    pub fn next(&mut self, total_batches: usize, total_transactions: usize) {
        let total_batches = total_batches as SequenceNumber;
        let total_transactions = total_transactions as SequenceNumber;

        if self.next_transaction_index + 1 == total_transactions {
            if self.next_batch_index + 1 == total_batches {
                self.next_certificate_index += 1;
            }
            self.next_batch_index = (self.next_batch_index + 1) % total_batches;
        }
        self.next_transaction_index = (self.next_transaction_index + 1) % total_transactions;
    }

    /// Update the state to skip a batch.
    pub fn skip_batch(&mut self, total_batches: usize) {
        let total_batches = total_batches as SequenceNumber;

        if self.next_batch_index + 1 == total_batches {
            self.next_certificate_index += 1;
        }
        self.next_batch_index = (self.next_batch_index + 1) % total_batches;
        self.next_transaction_index = 0;
    }

    /// Update the state to skip a certificate.
    pub fn skip_certificate(&mut self) {
        self.next_transaction_index = 0;
        self.next_batch_index = 0;
        self.next_certificate_index += 1;
    }

    /// Check whether the input index is the next expected batch index.
    pub fn check_next_batch_index(&self, batch_index: SequenceNumber) -> bool {
        batch_index == self.next_batch_index
    }

    /// Check whether the input index is the next expected transaction index.
    pub fn check_next_transaction_index(&self, transaction_index: SequenceNumber) -> bool {
        transaction_index == self.next_transaction_index
    }
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
