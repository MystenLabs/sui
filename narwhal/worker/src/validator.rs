// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use types::Batch;

/// Defines the validation procedure for receiving either a new single transaction (from a client)
/// of a batch of transactions (from another validator). Invalid transactions will not receive
/// further processing.
pub trait TxValidator: Clone + Send + Sync + 'static {
    /// Determines if a transaction valid for the worker to consider putting in a batch
    fn validate(&self, _t: &[u8]) -> bool {
        true
    }
    /// Determines if this batch can be voted on
    fn validate_batch(&self, _b: &Batch) -> bool {
        true
    }
}

/// Simple validator that accepts all transactions and batches.
#[derive(Debug, Clone, Default)]
pub struct TrivialTxValidator;
impl TxValidator for TrivialTxValidator {}
