// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{ExecutionEffects, workloads::ExpectedFailureType};
use async_trait::async_trait;
use std::{fmt::Display, num::NonZeroUsize};
use sui_types::crypto::AccountKeyPair;
use sui_types::digests::{ChainIdentifier, TransactionDigest};
use sui_types::transaction::{
    AllowedProposers, Transaction, TransactionDataAPI, TransactionExpiration,
};

#[derive(Debug, Clone)]
pub enum TransactionValidity {
    Unrestricted,
    Restricted {
        chain: ChainIdentifier,
        allowed_proposers: AllowedProposers,
    },
}

impl TransactionValidity {
    pub fn apply(self, transaction: Transaction, signer: &AccountKeyPair) -> Transaction {
        let Self::Restricted {
            chain,
            allowed_proposers,
        } = self
        else {
            return transaction;
        };

        let mut data = transaction.into_data().into_inner().intent_message.value;
        let expiration = match data.expiration().clone() {
            TransactionExpiration::ValidDuring {
                min_epoch,
                max_epoch,
                min_timestamp,
                max_timestamp,
                chain,
                nonce,
            }
            | TransactionExpiration::Validity {
                min_epoch,
                max_epoch,
                min_timestamp,
                max_timestamp,
                chain,
                nonce,
                ..
            } => TransactionExpiration::Validity {
                min_epoch,
                max_epoch,
                min_timestamp,
                max_timestamp,
                chain,
                nonce,
                allowed_proposers: Some(allowed_proposers),
            },
            TransactionExpiration::None | TransactionExpiration::Epoch(_) => {
                let epoch = allowed_proposers.epoch;
                TransactionExpiration::Validity {
                    min_epoch: Some(epoch),
                    max_epoch: Some(epoch),
                    min_timestamp: None,
                    max_timestamp: None,
                    chain,
                    nonce: rand::random(),
                    allowed_proposers: Some(allowed_proposers),
                }
            }
        };
        *data.expiration_mut() = expiration;
        Transaction::from_data_and_signer(data, vec![signer])
    }

    pub fn is_restricted(&self) -> bool {
        matches!(self, Self::Restricted { .. })
    }
}

pub trait TransactionValidityGenerator: Send {
    fn next_validity(&mut self) -> TransactionValidity;
}

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
    fn make_transaction(&mut self, validity: TransactionValidity) -> Transaction;
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

    /// Creates a batch of transactions for concurrent execution.
    /// Only called when `is_batched()` returns true.
    async fn make_transaction_batch(
        &mut self,
        validity_generator: &mut (dyn TransactionValidityGenerator + Send),
    ) -> Vec<Transaction> {
        vec![self.make_transaction(validity_generator.next_validity())]
    }

    /// Handles the results of a batch of concurrent transactions.
    /// Called after the all transactions in the batch have been executed,
    /// allowing the payload to update its internal state based on which
    /// transactions succeeded or failed.
    fn handle_batch_results(&mut self, _results: &BatchExecutionResults) {
        // Default: do nothing
    }
}

#[cfg(test)]
mod tests {
    use nonempty::NonEmpty;
    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::{
        base_types::{SuiAddress, random_object_ref},
        crypto::{AccountKeyPair, get_key_pair},
        transaction::{AllowedProposers, TransactionDataAPI, TransactionExpiration},
    };

    use super::*;

    fn test_transaction(expiration: TransactionExpiration) -> (Transaction, AccountKeyPair) {
        let (sender, keypair) = get_key_pair();
        let mut data = TestTransactionBuilder::new(sender, random_object_ref(), 1)
            .transfer_sui(Some(1), SuiAddress::random_for_testing_only())
            .build();
        *data.expiration_mut() = expiration;
        (
            Transaction::from_data_and_signer(data, vec![&keypair]),
            keypair,
        )
    }

    #[test]
    fn unrestricted_validity_leaves_transaction_unchanged() {
        let (transaction, keypair) = test_transaction(TransactionExpiration::None);
        let digest = *transaction.digest();

        let transaction = TransactionValidity::Unrestricted.apply(transaction, &keypair);

        assert_eq!(*transaction.digest(), digest);
        assert_eq!(
            transaction.data().transaction_data().expiration(),
            &TransactionExpiration::None
        );
    }

    #[test]
    fn restricted_validity_sets_allowed_proposers_and_resigns() {
        let epoch = 7;
        let chain = ChainIdentifier::default();
        let allowed_proposers = AllowedProposers {
            epoch,
            proposers: NonEmpty::from_vec(vec![1, 3]).unwrap(),
        };
        let (transaction, keypair) = test_transaction(TransactionExpiration::None);

        let transaction = TransactionValidity::Restricted {
            chain,
            allowed_proposers: allowed_proposers.clone(),
        }
        .apply(transaction, &keypair);

        match transaction.data().transaction_data().expiration() {
            TransactionExpiration::Validity {
                min_epoch,
                max_epoch,
                chain: actual_chain,
                allowed_proposers: actual_allowed_proposers,
                ..
            } => {
                assert_eq!(*min_epoch, Some(epoch));
                assert_eq!(*max_epoch, Some(epoch));
                assert_eq!(*actual_chain, chain);
                assert_eq!(actual_allowed_proposers, &Some(allowed_proposers));
            }
            other => panic!("expected restricted validity, got {other:?}"),
        }
        transaction
            .verify_signature_for_testing(epoch, &Default::default())
            .unwrap();
    }

    #[test]
    fn restricted_validity_preserves_existing_validity_window() {
        let epoch = 11;
        let chain = ChainIdentifier::default();
        let expiration = TransactionExpiration::ValidDuring {
            min_epoch: Some(epoch),
            max_epoch: Some(epoch),
            min_timestamp: Some(100),
            max_timestamp: Some(200),
            chain,
            nonce: 42,
        };
        let (transaction, keypair) = test_transaction(expiration);
        let allowed_proposers = AllowedProposers {
            epoch,
            proposers: NonEmpty::from_vec(vec![0, 2]).unwrap(),
        };

        let transaction = TransactionValidity::Restricted {
            chain,
            allowed_proposers: allowed_proposers.clone(),
        }
        .apply(transaction, &keypair);

        assert_eq!(
            transaction.data().transaction_data().expiration(),
            &TransactionExpiration::Validity {
                min_epoch: Some(epoch),
                max_epoch: Some(epoch),
                min_timestamp: Some(100),
                max_timestamp: Some(200),
                chain,
                nonce: 42,
                allowed_proposers: Some(allowed_proposers),
            }
        );
        transaction
            .verify_signature_for_testing(epoch, &Default::default())
            .unwrap();
    }
}
