// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{ExecutionEffects, workloads::ExpectedFailureType};
use std::fmt::Display;
use sui_types::transaction::Transaction;

/// Results from executing a soft bundle of transactions.
pub struct SoftBundleExecutionResults {
    /// Results for each transaction in the bundle.
    pub results: Vec<SoftBundleTransactionResult>,
}

/// Result for a single transaction within a soft bundle.
#[derive(Debug)]
pub enum SoftBundleTransactionResult {
    /// Transaction executed successfully.
    Success {
        /// The execution effects from the successful transaction.
        effects: Box<ExecutionEffects>,
    },
    /// Transaction failed with a non-retriable error (e.g., ObjectLockConflict).
    PermanentFailure {
        /// Error message describing the failure.
        error: String,
    },
    /// Transaction failed with a retriable error (e.g., epoch change, expired).
    RetriableFailure {
        /// Error message describing the failure.
        error: String,
    },
}

impl SoftBundleTransactionResult {
    /// Returns true if this result represents a successful transaction.
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }

    /// Returns true if this result represents a retriable failure.
    pub fn is_retriable(&self) -> bool {
        matches!(self, Self::RetriableFailure { .. })
    }

    /// Returns the error message if this is a failure, None if success.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Success { .. } => None,
            Self::PermanentFailure { error } | Self::RetriableFailure { error } => Some(error),
        }
    }

    /// Returns the effects if this is a success, None if failure.
    pub fn effects(&self) -> Option<&ExecutionEffects> {
        match self {
            Self::Success { effects } => Some(effects),
            _ => None,
        }
    }
}

/// Result for a single transaction in a concurrent batch.
pub enum ConcurrentTransactionResult {
    /// Transaction executed successfully with effects.
    Success { effects: Box<ExecutionEffects> },
    /// Transaction failed with an error message.
    Failure { error: String },
}

/// A Payload is a transaction wrapper of a particular type (transfer object, shared counter, etc).
/// Calling `make_transaction()` on a payload produces the transaction it is wrapping. Once that
/// transaction is returned with effects (by quorum driver), a new payload can be generated with that
/// effect by invoking `make_new_payload(effects)`
pub trait Payload: Send + Sync + std::fmt::Debug + Display {
    fn make_new_payload(&mut self, effects: &ExecutionEffects);
    fn make_transaction(&mut self) -> Transaction;
    fn get_failure_type(&self) -> Option<ExpectedFailureType> {
        None // Default implementation returns None
    }

    /// Returns true if this payload should be executed as a soft bundle.
    /// When true, the bench driver will call `make_soft_bundle_transactions()` and
    /// execute all transactions together via `proxy.execute_soft_bundle()`.
    fn is_soft_bundle(&self) -> bool {
        false // Default: not a soft bundle
    }

    /// Creates all transactions for a soft bundle execution.
    /// Only called when `is_soft_bundle()` returns true.
    /// Returns a vector of transactions that should be submitted together as a soft bundle.
    fn make_soft_bundle_transactions(&mut self) -> Vec<Transaction> {
        vec![self.make_transaction()] // Default: single transaction
    }

    /// Handles the results of soft bundle execution.
    /// Called after the soft bundle has been executed, allowing the payload to update
    /// its internal state based on which transactions succeeded or failed.
    fn handle_soft_bundle_results(&mut self, _results: &SoftBundleExecutionResults) {
        // Default: do nothing
    }

    /// Returns true if this payload generates multiple concurrent transactions.
    /// When true, the bench driver will call `make_concurrent_transactions()` and
    /// submit each transaction separately but concurrently (not as a bundle).
    fn is_concurrent_batch(&self) -> bool {
        false // Default: not a concurrent batch
    }

    /// Creates transactions to be submitted concurrently (but separately).
    /// Only called when `is_concurrent_batch()` returns true.
    /// Each transaction is submitted independently and may succeed or fail.
    fn make_concurrent_transactions(&mut self) -> Vec<Transaction> {
        vec![self.make_transaction()] // Default: single transaction
    }

    /// Handles results from concurrent transaction execution.
    /// Called after all concurrent transactions have completed.
    fn handle_concurrent_results(&mut self, _results: &[ConcurrentTransactionResult]) {
        // Default: do nothing
    }
}
