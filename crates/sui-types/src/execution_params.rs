// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use mysten_common::assert_reachable;
use nonempty::NonEmpty;
use once_cell::sync::Lazy;

use crate::{
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
    execution_status::{CongestedObjects, ExecutionErrorKind, ExecutionFailure, ExecutionStatus},
    transaction::CheckedInputObjects,
};

/// Execution inputs computed before running a transaction: whether to fail it early (and with
/// which errors), plus context for gas charging. An execution input only - never serialized into
/// `TransactionEffects`, so adding fields here does not change effects or their digests.
#[derive(Debug, Clone)]
pub struct ExecutionOrEarlyError {
    early_errors: Option<NonEmpty<ExecutionErrorKind>>,
    /// Accumulator (settlement) root version assigned to this transaction. Gates the mainnet
    /// address-balance gas-smash short-circuit. Populated only for mainnet committed execution;
    /// `None` elsewhere, leaving that gate inert.
    accumulator_version: Option<SequenceNumber>,
}

impl ExecutionOrEarlyError {
    /// Execute the transaction normally (no predetermined early error).
    pub fn ok(accumulator_version: Option<SequenceNumber>) -> Self {
        Self {
            early_errors: None,
            accumulator_version,
        }
    }

    /// Skip execution and fail the transaction with `errors`.
    pub fn failed(
        errors: NonEmpty<ExecutionErrorKind>,
        accumulator_version: Option<SequenceNumber>,
    ) -> Self {
        Self {
            early_errors: Some(errors),
            accumulator_version,
        }
    }

    pub fn is_ok(&self) -> bool {
        self.early_errors.is_none()
    }

    /// The predetermined early errors, if any.
    pub fn early_errors(&self) -> Option<&NonEmpty<ExecutionErrorKind>> {
        self.early_errors.as_ref()
    }

    /// Consume self, returning the predetermined early errors, if any.
    pub fn into_early_errors(self) -> Option<NonEmpty<ExecutionErrorKind>> {
        self.early_errors
    }

    pub fn accumulator_version(&self) -> Option<SequenceNumber> {
        self.accumulator_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FundsWithdrawStatus {
    /// Either we don't know yet whether the funds withdrawals are sufficient or not,
    /// or we know for sure that the funds withdrawals are sufficient.
    /// The reason we don't need to distinguish between unknown and sufficient funds is that
    /// in either case we would have to go ahead and execute the transaction anyway.
    MaybeSufficient,
    // TODO(address-funds): Add information on the address and type?
    /// We know for sure that the funds withdrawals in this transaction do not all have enough funds.
    /// This takes account of both address and object funds withdrawals.
    Insufficient,
}

/// Reconstruct only the congestion bit lost when a gas allowance retains its normal version.
/// Other objects and the cancellation reason are always decoded from their version sentinels.
/// Callers must first establish that this is an enabled allowance payment.
pub fn congested_gas_allowance_from_effects(
    allowance_id: ObjectID,
    status: &ExecutionStatus,
    shared_versions: impl IntoIterator<Item = (ObjectID, SequenceNumber)>,
) -> Option<ObjectID> {
    let mut allowance_is_loaded = false;
    let mut has_cancelled_read = false;
    let mut has_congestion = false;
    let mut has_randomness_cancellation = false;
    for (id, version) in shared_versions {
        if id == allowance_id {
            allowance_is_loaded = !version.is_cancelled();
        }
        has_cancelled_read |= version == SequenceNumber::CANCELLED_READ;
        has_congestion |= version == SequenceNumber::CONGESTED;
        has_randomness_cancellation |= version == SequenceNumber::RANDOMNESS_UNAVAILABLE;
    }
    if !allowance_is_loaded || has_randomness_cancellation {
        return None;
    }
    let explicitly_congested = matches!(
        status,
        ExecutionStatus::Failure(ExecutionFailure {
            error: ExecutionErrorKind::ExecutionCancelledDueToSharedObjectCongestion {
                congested_objects: CongestedObjects(objects),
            },
            ..
        }) if objects.contains(&allowance_id)
    );
    // If allowance availability failed first, effects need not contain a cancellation error.
    // All other inputs being CANCELLED_READ still identifies the sole missing cause.
    (explicitly_congested || (has_cancelled_read && !has_congestion)).then_some(allowance_id)
}

/// Determine if a transaction is predetermined to fail execution.
/// Returns all matching error kinds, or `None` if there is no early failure.
/// When we pass this to the execution engine, we will not execute the transaction
/// if it is predetermined to fail execution.
pub fn get_early_execution_error(
    transaction_digest: &TransactionDigest,
    input_objects: &CheckedInputObjects,
    config_certificate_deny_set: &HashSet<TransactionDigest>,
    congested_gas_allowance: Option<ObjectID>,
    funds_withdraw_status: &FundsWithdrawStatus,
) -> Option<NonEmpty<ExecutionErrorKind>> {
    let mut errors = vec![];
    if is_certificate_denied(transaction_digest, config_certificate_deny_set) {
        errors.push(ExecutionErrorKind::CertificateDenied);
    }

    if input_objects
        .inner()
        .contains_consensus_stream_ended_objects()
    {
        errors.push(ExecutionErrorKind::InputObjectDeleted);
    }

    if let Some((cancelled_objects, reason)) = input_objects
        .inner()
        .get_cancelled_objects(congested_gas_allowance)
    {
        match reason {
            SequenceNumber::CONGESTED => {
                errors.push(
                    ExecutionErrorKind::ExecutionCancelledDueToSharedObjectCongestion {
                        congested_objects: CongestedObjects(cancelled_objects),
                    },
                );
            }
            SequenceNumber::RANDOMNESS_UNAVAILABLE => {
                errors.push(ExecutionErrorKind::ExecutionCancelledDueToRandomnessUnavailable);
            }
            _ => panic!("invalid cancellation reason SequenceNumber: {reason}"),
        }
    }

    if matches!(funds_withdraw_status, FundsWithdrawStatus::Insufficient) {
        assert_reachable!("insufficient funds for withdraw");
        errors.push(ExecutionErrorKind::InsufficientFundsForWithdraw);
    }

    NonEmpty::from_vec(errors)
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
            None,
            &FundsWithdrawStatus::Insufficient,
        );
        assert_eq!(
            result.unwrap().into_iter().collect::<Vec<_>>(),
            vec![ExecutionErrorKind::InsufficientFundsForWithdraw],
        );

        // Test with sufficient balance
        let result = get_early_execution_error(
            &tx_digest,
            &input_objects,
            &deny_set,
            None,
            &FundsWithdrawStatus::MaybeSufficient,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_early_execution_error_collects_all() {
        let tx_digest = crate::digests::TransactionDigest::random();
        let input_objects = create_test_input_objects();

        // Certificate denial + insufficient balance collected together.
        let mut deny_set = HashSet::new();
        deny_set.insert(tx_digest);
        let result = get_early_execution_error(
            &tx_digest,
            &input_objects,
            &deny_set,
            None,
            &FundsWithdrawStatus::Insufficient,
        );
        assert_eq!(
            result.unwrap().into_iter().collect::<Vec<_>>(),
            vec![
                ExecutionErrorKind::CertificateDenied,
                ExecutionErrorKind::InsufficientFundsForWithdraw,
            ],
        );

        // Deleted input objects + insufficient balance.
        let input_objects = InputObjects::new(vec![ObjectReadResult {
            input_object_kind: InputObjectKind::SharedMoveObject {
                id: ObjectID::random(),
                initial_shared_version: SequenceNumber::MIN,
                mutability: SharedObjectMutability::Immutable,
            },
            object: ObjectReadResultKind::ObjectConsensusStreamEnded(
                SequenceNumber::MIN, // doesn't matter
                tx_digest,
            ),
        }]);
        deny_set.clear();
        let result = get_early_execution_error(
            &tx_digest,
            &CheckedInputObjects::new_for_replay(input_objects),
            &deny_set,
            None,
            &FundsWithdrawStatus::Insufficient,
        );
        assert_eq!(
            result.unwrap().into_iter().collect::<Vec<_>>(),
            vec![
                ExecutionErrorKind::InputObjectDeleted,
                ExecutionErrorKind::InsufficientFundsForWithdraw,
            ],
        );

        // Cancelled (congestion) + insufficient balance.
        let input_objects = InputObjects::new(vec![ObjectReadResult {
            input_object_kind: InputObjectKind::SharedMoveObject {
                id: ObjectID::random(),
                initial_shared_version: SequenceNumber::MIN,
                mutability: SharedObjectMutability::Immutable,
            },
            object: ObjectReadResultKind::CancelledTransactionSharedObject(
                SequenceNumber::CONGESTED,
            ),
        }]);
        let marker_result = get_early_execution_error(
            &tx_digest,
            &CheckedInputObjects::new_for_replay(input_objects),
            &deny_set,
            None,
            &FundsWithdrawStatus::Insufficient,
        );

        let result: Vec<_> = marker_result.unwrap().into_iter().collect();
        assert_eq!(result.len(), 2);
        assert!(matches!(
            result[0],
            ExecutionErrorKind::ExecutionCancelledDueToSharedObjectCongestion { .. }
        ));
        assert_eq!(result[1], ExecutionErrorKind::InsufficientFundsForWithdraw);
    }

    #[test]
    fn replay_cancellation_uses_sentinels_not_expected_error() {
        let allowance = crate::object::Object::shared_for_testing();
        let allowance_id = allowance.id();
        let congested_id = ObjectID::random();
        let unrelated_id = ObjectID::random();
        let inputs = |reason| {
            CheckedInputObjects::new_for_replay(InputObjects::new(vec![
                ObjectReadResult {
                    input_object_kind: InputObjectKind::SharedMoveObject {
                        id: allowance_id,
                        initial_shared_version: allowance.owner.start_version().unwrap(),
                        mutability: SharedObjectMutability::Mutable,
                    },
                    object: ObjectReadResultKind::Object(allowance.clone()),
                },
                ObjectReadResult {
                    input_object_kind: InputObjectKind::SharedMoveObject {
                        id: congested_id,
                        initial_shared_version: SequenceNumber::MIN,
                        mutability: SharedObjectMutability::Mutable,
                    },
                    object: ObjectReadResultKind::CancelledTransactionSharedObject(reason),
                },
                ObjectReadResult {
                    input_object_kind: InputObjectKind::SharedMoveObject {
                        id: unrelated_id,
                        initial_shared_version: SequenceNumber::MIN,
                        mutability: SharedObjectMutability::Mutable,
                    },
                    object: ObjectReadResultKind::CancelledTransactionSharedObject(
                        SequenceNumber::CANCELLED_READ,
                    ),
                },
            ]))
        };
        let versions = |reason| {
            [
                (allowance_id, allowance.version()),
                (congested_id, reason),
                (unrelated_id, SequenceNumber::CANCELLED_READ),
            ]
        };
        let stale_status = ExecutionStatus::Failure(ExecutionFailure::new(
            ExecutionErrorKind::ExecutionCancelledDueToSharedObjectCongestion {
                congested_objects: CongestedObjects(vec![allowance_id, unrelated_id]),
            },
            None,
        ));
        let digest = TransactionDigest::random();
        let deny_set = HashSet::new();
        let reconstruct = |status: &ExecutionStatus, reason| {
            get_early_execution_error(
                &digest,
                &inputs(reason),
                &deny_set,
                congested_gas_allowance_from_effects(allowance_id, status, versions(reason)),
                &FundsWithdrawStatus::MaybeSufficient,
            )
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>()
        };

        // Only the allowance's missing congestion bit comes from effects. The other IDs
        // must come from the actual sentinels, even when expected effects name the wrong object.
        assert_eq!(
            reconstruct(&stale_status, SequenceNumber::CONGESTED),
            vec![
                ExecutionErrorKind::ExecutionCancelledDueToSharedObjectCongestion {
                    congested_objects: CongestedObjects(vec![allowance_id, congested_id]),
                }
            ],
        );
        // An expected congestion error cannot override a randomness-unavailable sentinel.
        assert_eq!(
            reconstruct(&stale_status, SequenceNumber::RANDOMNESS_UNAVAILABLE),
            vec![ExecutionErrorKind::ExecutionCancelledDueToRandomnessUnavailable],
        );
        // Unavailable escrow can supersede the cancellation status. CANCELLED_READ inputs
        // still identify the retained allowance as the sole cause, without trusting that status.
        assert_eq!(
            reconstruct(
                &ExecutionStatus::Failure(ExecutionFailure::new(
                    ExecutionErrorKind::InsufficientGas,
                    None,
                )),
                SequenceNumber::CANCELLED_READ,
            ),
            vec![
                ExecutionErrorKind::ExecutionCancelledDueToSharedObjectCongestion {
                    congested_objects: CongestedObjects(vec![allowance_id]),
                }
            ],
        );

        let mut certificate_denied = HashSet::new();
        certificate_denied.insert(digest);
        assert_eq!(
            get_early_execution_error(
                &digest,
                &inputs(SequenceNumber::CANCELLED_READ),
                &certificate_denied,
                congested_gas_allowance_from_effects(
                    allowance_id,
                    &ExecutionStatus::Failure(ExecutionFailure::new(
                        ExecutionErrorKind::CertificateDenied,
                        None,
                    )),
                    versions(SequenceNumber::CANCELLED_READ),
                ),
                &FundsWithdrawStatus::MaybeSufficient,
            )
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>(),
            vec![
                ExecutionErrorKind::CertificateDenied,
                ExecutionErrorKind::ExecutionCancelledDueToSharedObjectCongestion {
                    congested_objects: CongestedObjects(vec![allowance_id]),
                },
            ],
            "effects replay restores the retained allowance cancellation beneath a certificate denial"
        );

        assert_eq!(
            get_early_execution_error(
                &digest,
                &inputs(SequenceNumber::CANCELLED_READ),
                &deny_set,
                congested_gas_allowance_from_effects(
                    allowance_id,
                    &ExecutionStatus::Failure(ExecutionFailure::new(
                        ExecutionErrorKind::InsufficientFundsForWithdraw,
                        None,
                    )),
                    versions(SequenceNumber::CANCELLED_READ),
                ),
                &FundsWithdrawStatus::Insufficient,
            )
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>(),
            vec![
                ExecutionErrorKind::ExecutionCancelledDueToSharedObjectCongestion {
                    congested_objects: CongestedObjects(vec![allowance_id]),
                },
                ExecutionErrorKind::InsufficientFundsForWithdraw,
            ],
            "effects replay retains the cancellation input to the IFFW short-circuit"
        );
    }
}
