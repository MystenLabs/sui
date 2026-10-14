// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{ExecutionEffects, workloads::ExpectedFailureType};
use async_trait::async_trait;
use std::{fmt::Display, num::NonZeroUsize};
use sui_types::crypto::AccountKeyPair;
use sui_types::digests::TransactionDigest;
use sui_types::transaction::{Transaction, TransactionDataAPI as _, TransactionExpiration};

/// Results from executing a batch of transactions.
pub struct BatchExecutionResults {
    /// Results for each transaction in the bundle.
    pub results: Vec<BatchedTransactionResult>,
}

/// Result for a single transaction within a batch.
#[derive(Debug)]
pub struct BatchedTransactionResult {
    /// The transaction digest associated with this result.
    pub digest: TransactionDigest,
    /// The status/outcome of the transaction.
    pub status: BatchedTransactionStatus,
}

/// Status of a single transaction within a batch.
#[derive(Debug)]
pub enum BatchedTransactionStatus {
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
    /// We didn't get a specific error message, so the failure could be
    /// retriable or permanent.
    UnknownRejection,
}

impl BatchedTransactionResult {
    /// Returns true if this result represents a successful transaction.
    pub fn is_success(&self) -> bool {
        matches!(self.status, BatchedTransactionStatus::Success { .. })
    }

    /// Returns true if this result represents a retriable failure.
    pub fn is_retriable(&self) -> bool {
        matches!(
            self.status,
            BatchedTransactionStatus::RetriableFailure { .. }
        )
    }

    /// Returns the error message if this is a failure, None if success.
    pub fn error(&self) -> Option<&str> {
        match &self.status {
            BatchedTransactionStatus::Success { .. } => None,
            BatchedTransactionStatus::PermanentFailure { error }
            | BatchedTransactionStatus::RetriableFailure { error } => Some(error),
            BatchedTransactionStatus::UnknownRejection => Some("unknown rejection"),
        }
    }

    /// Returns the effects if this is a success, None if failure.
    pub fn effects(&self) -> Option<&ExecutionEffects> {
        match &self.status {
            BatchedTransactionStatus::Success { effects } => Some(effects),
            _ => None,
        }
    }

    pub fn description(&self) -> String {
        match &self.status {
            BatchedTransactionStatus::Success { effects } => {
                format!("{}: success: {:?}", self.digest, effects.status())
            }
            BatchedTransactionStatus::PermanentFailure { error } => {
                format!("{}: permanent failure: {}", self.digest, error)
            }
            BatchedTransactionStatus::RetriableFailure { error } => {
                format!("{}: retriable failure: {}", self.digest, error)
            }
            BatchedTransactionStatus::UnknownRejection => {
                format!("{}: unknown rejection", self.digest)
            }
        }
    }
}

/// A Payload is a transaction wrapper of a particular type (transfer object, shared counter, etc).
/// Calling `make_transaction()` on a payload produces the transaction it is wrapping. Once that
/// transaction is returned with effects (by quorum driver), a new payload can be generated with that
/// effect by invoking `make_new_payload(effects)`
#[async_trait]
pub trait Payload: Send + Sync + std::fmt::Debug + Display {
    fn make_new_payload(&mut self, effects: &ExecutionEffects);
    fn make_transaction(&mut self) -> Transaction;

    fn make_transaction_with_expiration(
        &mut self,
        expiration: TransactionExpiration,
    ) -> Transaction {
        assert!(
            matches!(expiration, TransactionExpiration::None),
            "this workload does not support restricted proposer traffic"
        );
        self.make_transaction()
    }
    fn get_failure_type(&self) -> Option<ExpectedFailureType> {
        None // Default implementation returns None
    }

    /// Returns true if this payload builds batches of transactions.
    /// When true, the bench driver will call `make_transaction_batch()`.
    /// The batch will be split into a random number of soft bundles,
    /// each of which will be executed by `proxy.execute_soft_bundle()`.
    fn is_batched(&self) -> bool {
        false // Default: not a batch
    }

    /// Returns the maximum number of soft bundles that can be created for a batch of transactions.
    /// If set to 1, all transactions will always be executed as a single bundle.
    fn max_soft_bundles(&self) -> NonZeroUsize {
        NonZeroUsize::MAX
    }

    /// Maximum size of any individual soft bundle.
    fn max_soft_bundle_size(&self) -> NonZeroUsize {
        // TODO: we could get this from the protocol config but a) its unlikely to change
        // b) it would be very hard to do that
        NonZeroUsize::new(5).unwrap()
    }

    /// Returns true if every transaction in the batch should be submitted independently through
    /// the normal transaction path instead of being packed into soft bundles.
    fn use_direct_batch_submission(&self) -> bool {
        false
    }

    /// Creates a batch of transactions for concurrent execution.
    /// Only called when `is_batched()` returns true.
    async fn make_transaction_batch(&mut self) -> Vec<Transaction> {
        vec![self.make_transaction()] // Default: single transaction
    }

    /// Handles the results of a batch of concurrent transactions.
    /// Called after the all transactions in the batch have been executed,
    /// allowing the payload to update its internal state based on which
    /// transactions succeeded or failed.
    fn handle_batch_results(&mut self, _results: &BatchExecutionResults) {
        // Default: do nothing
    }
}

pub fn resign_transaction_with_expiration(
    transaction: Transaction,
    expiration: TransactionExpiration,
    signer: &AccountKeyPair,
) -> Transaction {
    let mut transaction_data = transaction.into_data().into_inner().intent_message.value;
    *transaction_data.expiration_mut() = expiration;
    Transaction::from_data_and_signer(transaction_data, vec![signer])
}

#[cfg(test)]
mod tests {
    use super::resign_transaction_with_expiration;
    use sui_types::base_types::random_object_ref;
    use sui_types::crypto::get_key_pair;
    use sui_types::digests::ChainIdentifier;
    use sui_types::signature::VerifyParams;
    use sui_types::transaction::{
        AllowedProposers, Transaction, TransactionData, TransactionDataAPI as _,
        TransactionExpiration,
    };

    #[test]
    fn restricted_transaction_is_resigned() {
        let (sender, keypair) = get_key_pair();
        let transaction = Transaction::from_data_and_signer(
            TransactionData::new_transfer_sui(
                sender,
                sender,
                Some(1),
                random_object_ref(),
                1_000_000,
                1_000,
            ),
            vec![&keypair],
        );
        let expiration = TransactionExpiration::Validity {
            min_epoch: Some(7),
            max_epoch: Some(8),
            min_timestamp: None,
            max_timestamp: None,
            chain: ChainIdentifier::default(),
            nonce: 9,
            allowed_proposers: Some(AllowedProposers {
                epoch: 7,
                proposers: vec![0, 1, 2].try_into().unwrap(),
            }),
        };

        let transaction =
            resign_transaction_with_expiration(transaction, expiration.clone(), &keypair);

        assert_eq!(
            transaction.data().transaction_data().expiration(),
            &expiration
        );
        transaction
            .verify_signature_for_testing(7, &VerifyParams::default())
            .unwrap();
    }
}
