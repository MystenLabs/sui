// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use once_cell::sync::Lazy;

use crate::{
    base_types::SequenceNumber, digests::TransactionDigest, error::ExecutionErrorKind,
    execution_status::CongestedObjects, transaction::CheckedInputObjects,
};

pub type ExecutionOrEarlyError = Result<(), ExecutionErrorKind>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BalanceWithdrawStatus {
    NoWithdraw,
    SufficientBalance,
    // TODO(address-balances): Add information on the address and type?
    InsufficientBalance,
}

/// Determine if a transaction is predetermined to fail execution.
/// If so, return the error kind, otherwise return `None`.
/// When we pass this to the execution engine, we will not execute the transaction
/// if it is predetermined to fail execution.
pub fn get_early_execution_error(
    transaction_digest: &TransactionDigest,
    input_objects: &CheckedInputObjects,
    config_certificate_deny_set: &HashSet<TransactionDigest>,
    balance_withdraw_status: &BalanceWithdrawStatus,
) -> Option<ExecutionErrorKind> {
    if is_certificate_denied(transaction_digest, config_certificate_deny_set) {
        return Some(ExecutionErrorKind::CertificateDenied);
    }

    if input_objects
        .inner()
        .contains_consensus_stream_ended_objects()
    {
        return Some(ExecutionErrorKind::InputObjectDeleted);
    }

    let cancelled_objects = input_objects.inner().get_cancelled_objects();
    if let Some((cancelled_objects, reason)) = cancelled_objects {
        match reason {
            SequenceNumber::CONGESTED => {
                return Some(
                    ExecutionErrorKind::ExecutionCancelledDueToSharedObjectCongestion {
                        congested_objects: CongestedObjects(cancelled_objects),
                    },
                );
            }
            SequenceNumber::RANDOMNESS_UNAVAILABLE => {
                return Some(ExecutionErrorKind::ExecutionCancelledDueToRandomnessUnavailable);
            }
            _ => panic!("invalid cancellation reason SequenceNumber: {reason}"),
        }
    }

    if matches!(
        balance_withdraw_status,
        BalanceWithdrawStatus::InsufficientBalance
    ) {
        return Some(ExecutionErrorKind::InsufficientBalanceForWithdraw);
    }

    None
}

/// If a transaction digest shows up in this list, when executing such transaction,
/// we will always return `ExecutionError::CertificateDenied` without executing it (but still do
/// gas smashing). Because this list is not gated by protocol version, there are a few important
/// criteria for adding a digest to this list:
/// 1. The certificate must be causing all validators to either panic or hang forever deterministically.
/// 2. If we ever ship a fix to make it no longer panic or hang when executing such transaction, we
///    must make sure the transaction is already in this list. Otherwise nodes running the newer
///    version without these transactions in the list will generate forked result.
///
/// Below is a scenario of when we need to use this list:
/// 1. We detect that a specific transaction is causing all validators to either panic or hang forever deterministically.
/// 2. We push a CertificateDenyConfig to deny such transaction to all validators asap.
/// 3. To make sure that all fullnodes are able to sync to the latest version, we need to add the
///    transaction digest to this list as well asap, and ship this binary to all fullnodes, so that
///    they can sync past this transaction.
/// 4. We then can start fixing the issue, and ship the fix to all nodes.
/// 5. Unfortunately, we can't remove the transaction digest from this list, because if we do so,
///    any future node that sync from genesis will fork on this transaction. We may be able to
///    remove it once we have stable snapshots and the binary has a minimum supported protocol
///    version past the epoch.
fn get_denied_certificates() -> &'static HashSet<TransactionDigest> {
    static DENIED_CERTIFICATES: Lazy<HashSet<TransactionDigest>> = Lazy::new(|| HashSet::from([]));
    Lazy::force(&DENIED_CERTIFICATES)
}

// This is needed to initialize static variables in the simtest environment.
#[cfg(msim)]
pub fn get_denied_certificates_for_sim_test() -> &'static HashSet<TransactionDigest> {
    get_denied_certificates()
}

fn is_certificate_denied(
    transaction_digest: &TransactionDigest,
    certificate_deny_set: &HashSet<TransactionDigest>,
) -> bool {
    certificate_deny_set.contains(transaction_digest)
        || get_denied_certificates().contains(transaction_digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        base_types::ObjectID,
        transaction::{
            CheckedInputObjects, InputObjectKind, InputObjects, ObjectReadResult,
            ObjectReadResultKind, SharedObjectMutability,
        },
    };

    fn create_test_input_objects() -> CheckedInputObjects {
        let input_objects = InputObjects::new(vec![]);
        CheckedInputObjects::new_for_replay(input_objects)
    }

    #[test]
    fn test_early_execution_error_insufficient_balance() {
        let tx_digest = crate::digests::TransactionDigest::random();
        let input_objects = create_test_input_objects();
        let deny_set = HashSet::new();

        // Test with insufficient balance
        let result = get_early_execution_error(
            &tx_digest,
            &input_objects,
            &deny_set,
            &BalanceWithdrawStatus::InsufficientBalance,
        );
        assert_eq!(
            result,
            Some(ExecutionErrorKind::InsufficientBalanceForWithdraw)
        );

        // Test with sufficient balance
        let result = get_early_execution_error(
            &tx_digest,
            &input_objects,
            &deny_set,
            &BalanceWithdrawStatus::SufficientBalance,
        );
        assert_eq!(result, None);
    }

    #[test]
    fn test_early_execution_error_precedence() {
        let tx_digest = crate::digests::TransactionDigest::random();
        let input_objects = create_test_input_objects();

        // Test that certificate denial takes precedence over insufficient balance
        let mut deny_set = HashSet::new();
        deny_set.insert(tx_digest);
        let result = get_early_execution_error(
            &tx_digest,
            &input_objects,
            &deny_set,
            &BalanceWithdrawStatus::InsufficientBalance,
        );
        assert_eq!(result, Some(ExecutionErrorKind::CertificateDenied));

        // Test that deleted input objects take precedence over insufficient balance
        let input_objects = InputObjects::new(vec![
            // canceled object
            ObjectReadResult {
                input_object_kind: InputObjectKind::SharedMoveObject {
                    id: ObjectID::random(),
                    initial_shared_version: SequenceNumber::MIN,
                    mutability: SharedObjectMutability::Immutable,
                },
                object: ObjectReadResultKind::ObjectConsensusStreamEnded(
                    SequenceNumber::MIN, // doesn't matter
                    tx_digest,
                ),
            },
        ]);
        deny_set.clear();
        let result = get_early_execution_error(
            &tx_digest,
            &CheckedInputObjects::new_for_replay(input_objects),
            &deny_set,
            &BalanceWithdrawStatus::InsufficientBalance,
        );
        assert_eq!(result, Some(ExecutionErrorKind::InputObjectDeleted));

        // Test that canceled takes precedence over insufficient balance
        let input_objects = InputObjects::new(vec![
            // canceled object
            ObjectReadResult {
                input_object_kind: InputObjectKind::SharedMoveObject {
                    id: ObjectID::random(),
                    initial_shared_version: SequenceNumber::MIN,
                    mutability: SharedObjectMutability::Immutable,
                },
                object: ObjectReadResultKind::CancelledTransactionSharedObject(
                    SequenceNumber::CONGESTED,
                ),
            },
        ]);
        let result = get_early_execution_error(
            &tx_digest,
            &CheckedInputObjects::new_for_replay(input_objects),
            &deny_set,
            &BalanceWithdrawStatus::InsufficientBalance,
        );
        assert!(matches!(
            result,
            Some(ExecutionErrorKind::ExecutionCancelledDueToSharedObjectCongestion { .. })
        ));
    }
}
