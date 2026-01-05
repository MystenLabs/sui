// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{ExecutionEffects, workloads::ExpectedFailureType};
use std::fmt::Display;
use sui_types::transaction::Transaction;

/// Results from executing a soft bundle of transactions.
pub struct SoftBundleExecutionResults {
    /// Results for each transaction in the bundle.
    /// Each result contains the transaction index, whether it succeeded, and optional effects.
    pub results: Vec<SoftBundleTransactionResult>,
}

/// Result for a single transaction within a soft bundle.
pub struct SoftBundleTransactionResult {
    /// Whether this transaction was successfully executed.
    pub success: bool,
    /// The execution effects, if the transaction was executed (not rejected).
    pub effects: Option<ExecutionEffects>,
    /// Error message if the transaction was rejected.
    pub error: Option<String>,
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
}
