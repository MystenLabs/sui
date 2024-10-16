// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_state::StateReadError;
use crate::name_service::NameServiceError;
use fastcrypto::error::FastCryptoError;
use hyper::header::InvalidHeaderValue;
use itertools::Itertools;
use jsonrpsee::core::Error as RpcError;
use jsonrpsee::types::error::{CallError, INTERNAL_ERROR_CODE};
use jsonrpsee::types::ErrorObject;
use std::collections::BTreeMap;
use sui_json_rpc_api::{TRANSACTION_EXECUTION_CLIENT_ERROR_CODE, TRANSIENT_ERROR_CODE};
use sui_types::committee::{QUORUM_THRESHOLD, TOTAL_VOTING_POWER};
use sui_types::error::{SuiError, SuiObjectResponseError, UserInputError};
use sui_types::quorum_driver_types::QuorumDriverError;
use thiserror::Error;
use tokio::task::JoinError;

pub type RpcInterimResult<T = ()> = Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    SuiError(SuiError),

    #[error(transparent)]
    InternalError(#[from] anyhow::Error),

    #[error("Deserialization error: {0}")]
    BcsError(#[from] bcs::Error),
    #[error("Unexpected error: {0}")]
    UnexpectedError(String),

    #[error(transparent)]
    RPCServerError(#[from] jsonrpsee::core::Error),

    #[error(transparent)]
    InvalidHeaderValue(#[from] InvalidHeaderValue),

    #[error(transparent)]
    UserInputError(#[from] UserInputError),

    #[error(transparent)]
    EncodingError(#[from] eyre::Report),

    #[error(transparent)]
    TokioJoinError(#[from] JoinError),

    #[error(transparent)]
    QuorumDriverError(#[from] QuorumDriverError),

    #[error(transparent)]
    FastCryptoError(#[from] FastCryptoError),

    #[error(transparent)]
    SuiObjectResponseError(#[from] SuiObjectResponseError),

    #[error(transparent)]
    SuiRpcInputError(#[from] SuiRpcInputError),

    // TODO(wlmyng): convert StateReadError::Internal message to generic internal error message.
    #[error(transparent)]
    StateReadError(#[from] StateReadError),

    #[error("Unsupported Feature: {0}")]
    UnsupportedFeature(String),

    #[error("transparent")]
    NameServiceError(#[from] NameServiceError),
}

impl From<SuiError> for Error {
    fn from(e: SuiError) -> Self {
        match e {
            SuiError::UserInputError { error } => Self::UserInputError(error),
            SuiError::SuiObjectResponseError { error } => Self::SuiObjectResponseError(error),
            SuiError::UnsupportedFeatureError { error } => Self::UnsupportedFeature(error),
            SuiError::IndexStoreNotAvailable => Self::UnsupportedFeature(
                "Required indexes are not available on this node".to_string(),
            ),
            other => Self::SuiError(other),
        }
    }
}

impl From<Error> for RpcError {
    /// `InvalidParams`/`INVALID_PARAMS_CODE` for client errors.
    fn from(e: Error) -> RpcError {
        match e {
            Error::UserInputError(_) => RpcError::Call(CallError::InvalidParams(e.into())),
            Error::UnsupportedFeature(_) => RpcError::Call(CallError::InvalidParams(e.into())),
            Error::SuiObjectResponseError(err) => match err {
                SuiObjectResponseError::NotExists { .. }
                | SuiObjectResponseError::DynamicFieldNotFound { .. }
                | SuiObjectResponseError::Deleted { .. }
                | SuiObjectResponseError::DisplayError { .. } => {
                    RpcError::Call(CallError::InvalidParams(err.into()))
                }
                _ => RpcError::Call(CallError::Failed(err.into())),
            },
            Error::NameServiceError(err) => match err {
                NameServiceError::ExceedsMaxLength { .. }
                | NameServiceError::InvalidHyphens { .. }
                | NameServiceError::InvalidLength { .. }
                | NameServiceError::InvalidUnderscore { .. }
                | NameServiceError::LabelsEmpty { .. }
                | NameServiceError::InvalidSeparator { .. } => {
                    RpcError::Call(CallError::InvalidParams(err.into()))
                }
                _ => RpcError::Call(CallError::Failed(err.into())),
            },
            Error::SuiRpcInputError(err) => RpcError::Call(CallError::InvalidParams(err.into())),
            Error::SuiError(sui_error) => match sui_error {
                SuiError::TransactionNotFound { .. }
                | SuiError::TransactionsNotFound { .. }
                | SuiError::TransactionEventsNotFound { .. } => {
                    RpcError::Call(CallError::InvalidParams(sui_error.into()))
                }
                _ => RpcError::Call(CallError::Failed(sui_error.into())),
            },
            Error::StateReadError(err) => match err {
                StateReadError::Client(_) => RpcError::Call(CallError::InvalidParams(err.into())),
                _ => {
                    let error_object = ErrorObject::owned(
                        jsonrpsee::types::error::INTERNAL_ERROR_CODE,
                        err.to_string(),
                        None::<()>,
                    );
                    RpcError::Call(CallError::Custom(error_object))
                }
            },
            Error::QuorumDriverError(err) => {
                match err {
                    QuorumDriverError::InvalidUserSignature(err) => {
                        let error_object = ErrorObject::owned(
                            TRANSACTION_EXECUTION_CLIENT_ERROR_CODE,
                            format!("Invalid user signature: {err}"),
                            None::<()>,
                        );
                        RpcError::Call(CallError::Custom(error_object))
                    }
                    QuorumDriverError::TxAlreadyFinalizedWithDifferentUserSignatures => {
                        let error_object = ErrorObject::owned(
                            TRANSACTION_EXECUTION_CLIENT_ERROR_CODE,
                            "The transaction is already finalized but with different user signatures",
                            None::<()>,
                        );
                        RpcError::Call(CallError::Custom(error_object))
                    }
                    QuorumDriverError::TimeoutBeforeFinality
                    | QuorumDriverError::FailedWithTransientErrorAfterMaximumAttempts { .. } => {
                        let error_object =
                            ErrorObject::owned(TRANSIENT_ERROR_CODE, err.to_string(), None::<()>);
                        RpcError::Call(CallError::Custom(error_object))
                    }
                    QuorumDriverError::ObjectsDoubleUsed {
                        conflicting_txes,
                        retried_tx_status,
                    } => {
                        let weights: Vec<u64> =
                            conflicting_txes.values().map(|(_, stake)| *stake).collect();
                        let remaining: u64 = TOTAL_VOTING_POWER - weights.iter().sum::<u64>();

                        // better version of above
                        let reason = if weights.iter().all(|w| remaining + w < QUORUM_THRESHOLD) {
                            "equivocated until the next epoch"
                        } else {
                            "reserved for another transaction"
                        };

                        let retried_info = match retried_tx_status {
                            Some((digest, success)) => {
                                format!(
                                    "Retried transaction {} ({}) because it was able to gather the necessary votes.",
                                    digest, if success { "succeeded" } else { "failed" }
                                )
                            }
                            None => "".to_string(),
                        };

                        let error_message = format!(
                            "Failed to sign transaction by a quorum of validators because one or more of its objects is {}. {} Other transactions locking these objects:\n{}",
                            reason,
                            retried_info,
                            conflicting_txes
                                .iter()
                                .sorted_by(|(_, (_, a)), (_, (_, b))| b.cmp(a))
                                .map(|(digest, (_, stake))| format!(
                                    "- {} (stake {}.{})",
                                    digest,
                                    stake / 100,
                                    stake % 100,
                                ))
                                .join("\n"),
                        );

                        let new_map = conflicting_txes
                            .into_iter()
                            .map(|(digest, (pairs, _))| {
                                (
                                    digest,
                                    pairs.into_iter().map(|(_, obj_ref)| obj_ref).collect(),
                                )
                            })
                            .collect::<BTreeMap<_, Vec<_>>>();

                        let error_object = ErrorObject::owned(
                            TRANSACTION_EXECUTION_CLIENT_ERROR_CODE,
                            error_message,
                            Some(new_map),
                        );
                        RpcError::Call(CallError::Custom(error_object))
                    }
                    QuorumDriverError::NonRecoverableTransactionError { errors } => {
                        let new_errors: Vec<String> = errors
                            .into_iter()
                            // sort by total stake, descending, so users see the most prominent one first
                            .sorted_by(|(_, a, _), (_, b, _)| b.cmp(a))
                            .filter_map(|(err, _, _)| {
                                match &err {
                                    // Special handling of UserInputError:
                                    // ObjectNotFound and DependentPackageNotFound are considered
                                    // retryable errors but they have different treatment
                                    // in AuthorityAggregator.
                                    // The optimal fix would be to examine if the total stake
                                    // of ObjectNotFound/DependentPackageNotFound exceeds the
                                    // quorum threshold, but it takes a Committee here.
                                    // So, we take an easier route and consider them non-retryable
                                    // at all. Combining this with the sorting above, clients will
                                    // see the dominant error first.
                                    SuiError::UserInputError { error } => Some(error.to_string()),
                                    _ => {
                                        if err.is_retryable().0 {
                                            None
                                        } else {
                                            Some(err.to_string())
                                        }
                                    }
                                }
                            })
                            .collect();

                        assert!(
                            !new_errors.is_empty(),
                            "NonRecoverableTransactionError should have at least one non-retryable error"
                        );

                        let mut error_list = vec![];

                        for err in new_errors.iter() {
                            error_list.push(format!("- {}", err));
                        }

                        let error_msg = format!("Transaction validator signing failed due to issues with transaction inputs, please review the errors and try again:\n{}", error_list.join("\n"));

                        let error_object = ErrorObject::owned(
                            TRANSACTION_EXECUTION_CLIENT_ERROR_CODE,
                            error_msg,
                            None::<()>,
                        );
                        RpcError::Call(CallError::Custom(error_object))
                    }
                    QuorumDriverError::QuorumDriverInternalError(_) => {
                        let error_object = ErrorObject::owned(
                            INTERNAL_ERROR_CODE,
                            "Internal error occurred while executing transaction.",
                            None::<()>,
                        );
                        RpcError::Call(CallError::Custom(error_object))
                    }
                    QuorumDriverError::SystemOverload { .. }
                    | QuorumDriverError::SystemOverloadRetryAfter { .. } => {
                        let error_object =
                            ErrorObject::owned(TRANSIENT_ERROR_CODE, err.to_string(), None::<()>);
                        RpcError::Call(CallError::Custom(error_object))
                    }
                }
            }
            _ => RpcError::Call(CallError::Failed(e.into())),
        }
    }
}

#[derive(Debug, Error)]
pub enum SuiRpcInputError {
    #[error("Input contains duplicates")]
    ContainsDuplicates,

    #[error("Input exceeds limit of {0}")]
    SizeLimitExceeded(String),

    #[error("{0}")]
    GenericNotFound(String),

    #[error("{0}")]
    GenericInvalid(String),

    #[error("request_type` must set to `None` or `WaitForLocalExecution` if effects is required in the response")]
    InvalidExecuteTransactionRequestType,

    #[error("Unsupported protocol version requested. Min supported: {0}, max supported: {1}")]
    ProtocolVersionUnsupported(u64, u64),

    #[error("{0}")]
    CannotParseSuiStructTag(String),

    #[error(transparent)]
    Base64(#[from] eyre::Report),

    #[error("Deserialization error: {0}")]
    Bcs(#[from] bcs::Error),

    #[error(transparent)]
    FastCryptoError(#[from] FastCryptoError),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error(transparent)]
    UserInputError(#[from] UserInputError),
}

impl From<SuiRpcInputError> for RpcError {
    fn from(e: SuiRpcInputError) -> Self {
        RpcError::Call(CallError::InvalidParams(e.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;
    use jsonrpsee::types::ErrorObjectOwned;
    use sui_types::base_types::AuthorityName;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::ObjectRef;
    use sui_types::base_types::SequenceNumber;
    use sui_types::committee::StakeUnit;
    use sui_types::crypto::AuthorityPublicKey;
    use sui_types::crypto::AuthorityPublicKeyBytes;
    use sui_types::digests::ObjectDigest;
    use sui_types::digests::TransactionDigest;

    fn test_object_ref() -> ObjectRef {
        (
            ObjectID::ZERO,
            SequenceNumber::from_u64(0),
            ObjectDigest::new([0; 32]),
        )
    }

    mod match_quorum_driver_error_tests {
        use super::*;

        #[test]
        fn test_invalid_user_signature() {
            let quorum_driver_error =
                QuorumDriverError::InvalidUserSignature(SuiError::InvalidSignature {
                    error: "Test inner invalid signature".to_string(),
                });

            let rpc_error: RpcError = Error::QuorumDriverError(quorum_driver_error).into();

            let error_object: ErrorObjectOwned = rpc_error.into();
            let expected_code = expect!["-32002"];
            expected_code.assert_eq(&error_object.code().to_string());
            let expected_message = expect![
                "Invalid user signature: Signature is not valid: Test inner invalid signature"
            ];
            expected_message.assert_eq(error_object.message());
        }

        #[test]
        fn test_timeout_before_finality() {
            let quorum_driver_error = QuorumDriverError::TimeoutBeforeFinality;

            let rpc_error: RpcError = Error::QuorumDriverError(quorum_driver_error).into();

            let error_object: ErrorObjectOwned = rpc_error.into();
            let expected_code = expect!["-32050"];
            expected_code.assert_eq(&error_object.code().to_string());
            let expected_message = expect!["Transaction timed out before reaching finality"];
            expected_message.assert_eq(error_object.message());
        }

        #[test]
        fn test_failed_with_transient_error_after_maximum_attempts() {
            let quorum_driver_error =
                QuorumDriverError::FailedWithTransientErrorAfterMaximumAttempts {
                    total_attempts: 10,
                };

            let rpc_error: RpcError = Error::QuorumDriverError(quorum_driver_error).into();

            let error_object: ErrorObjectOwned = rpc_error.into();
            let expected_code = expect!["-32050"];
            expected_code.assert_eq(&error_object.code().to_string());
            let expected_message = expect![
                "Transaction failed to reach finality with transient error after 10 attempts."
            ];
            expected_message.assert_eq(error_object.message());
        }

        #[test]
        fn test_objects_double_used() {
            use sui_types::crypto::VerifyingKey;
            let mut conflicting_txes: BTreeMap<
                TransactionDigest,
                (Vec<(AuthorityName, ObjectRef)>, StakeUnit),
            > = BTreeMap::new();
            let tx_digest = TransactionDigest::from([1; 32]);
            let object_ref = test_object_ref();

            // 4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi has enough stake to escape equivocation
            let stake_unit: StakeUnit = 8000;
            let authority_name = AuthorityPublicKeyBytes([0; AuthorityPublicKey::LENGTH]);
            conflicting_txes.insert(tx_digest, (vec![(authority_name, object_ref)], stake_unit));

            // 8qbHbw2BbbTHBW1sbeqakYXVKRQM8Ne7pLK7m6CVfeR stake below quorum threshold
            let tx_digest = TransactionDigest::from([2; 32]);
            let stake_unit: StakeUnit = 500;
            let authority_name = AuthorityPublicKeyBytes([1; AuthorityPublicKey::LENGTH]);
            conflicting_txes.insert(tx_digest, (vec![(authority_name, object_ref)], stake_unit));

            let quorum_driver_error = QuorumDriverError::ObjectsDoubleUsed {
                conflicting_txes,
                retried_tx_status: Some((TransactionDigest::from([1; 32]), true)),
            };

            let rpc_error: RpcError = Error::QuorumDriverError(quorum_driver_error).into();

            let error_object: ErrorObjectOwned = rpc_error.into();
            let expected_code = expect!["-32002"];
            expected_code.assert_eq(&error_object.code().to_string());
            let expected_message = expect!["Failed to sign transaction by a quorum of validators because one or more of its objects is reserved for another transaction. Retried transaction 4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi (succeeded) because it was able to gather the necessary votes. Other transactions locking these objects:\n- 4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi (stake 80.0)\n- 8qbHbw2BbbTHBW1sbeqakYXVKRQM8Ne7pLK7m6CVfeR (stake 5.0)"];
            expected_message.assert_eq(error_object.message());
            let expected_data = expect![[
                r#"{"4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi":[["0x0000000000000000000000000000000000000000000000000000000000000000",0,"11111111111111111111111111111111"]],"8qbHbw2BbbTHBW1sbeqakYXVKRQM8Ne7pLK7m6CVfeR":[["0x0000000000000000000000000000000000000000000000000000000000000000",0,"11111111111111111111111111111111"]]}"#
            ]];
            let actual_data = error_object.data().unwrap().to_string();
            expected_data.assert_eq(&actual_data);
        }

        #[test]
        fn test_objects_double_used_equivocated() {
            use sui_types::crypto::VerifyingKey;
            let mut conflicting_txes: BTreeMap<
                TransactionDigest,
                (Vec<(AuthorityName, ObjectRef)>, StakeUnit),
            > = BTreeMap::new();
            let tx_digest = TransactionDigest::from([1; 32]);
            let object_ref = test_object_ref();

            // 4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi has lower stake at 10
            let stake_unit: StakeUnit = 4000;
            let authority_name = AuthorityPublicKeyBytes([0; AuthorityPublicKey::LENGTH]);
            conflicting_txes.insert(tx_digest, (vec![(authority_name, object_ref)], stake_unit));

            // 8qbHbw2BbbTHBW1sbeqakYXVKRQM8Ne7pLK7m6CVfeR is a higher stake and should be first in the list
            let tx_digest = TransactionDigest::from([2; 32]);
            let stake_unit: StakeUnit = 5000;
            let authority_name = AuthorityPublicKeyBytes([1; AuthorityPublicKey::LENGTH]);
            conflicting_txes.insert(tx_digest, (vec![(authority_name, object_ref)], stake_unit));

            let quorum_driver_error = QuorumDriverError::ObjectsDoubleUsed {
                conflicting_txes,
                // below threshold
                retried_tx_status: None,
            };

            let rpc_error: RpcError = Error::QuorumDriverError(quorum_driver_error).into();

            let error_object: ErrorObjectOwned = rpc_error.into();
            let expected_code = expect!["-32002"];
            expected_code.assert_eq(&error_object.code().to_string());
            let expected_message = expect!["Failed to sign transaction by a quorum of validators because one or more of its objects is equivocated until the next epoch.  Other transactions locking these objects:\n- 8qbHbw2BbbTHBW1sbeqakYXVKRQM8Ne7pLK7m6CVfeR (stake 50.0)\n- 4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi (stake 40.0)"];
            expected_message.assert_eq(error_object.message());
            let expected_data = expect![[
                r#"{"4vJ9JU1bJJE96FWSJKvHsmmFADCg4gpZQff4P3bkLKi":[["0x0000000000000000000000000000000000000000000000000000000000000000",0,"11111111111111111111111111111111"]],"8qbHbw2BbbTHBW1sbeqakYXVKRQM8Ne7pLK7m6CVfeR":[["0x0000000000000000000000000000000000000000000000000000000000000000",0,"11111111111111111111111111111111"]]}"#
            ]];
            let actual_data = error_object.data().unwrap().to_string();
            expected_data.assert_eq(&actual_data);
        }

        #[test]
        fn test_non_recoverable_transaction_error() {
            let quorum_driver_error = QuorumDriverError::NonRecoverableTransactionError {
                errors: vec![
                    (
                        SuiError::UserInputError {
                            error: UserInputError::GasBalanceTooLow {
                                gas_balance: 10,
                                needed_gas_amount: 100,
                            },
                        },
                        0,
                        vec![],
                    ),
                    (
                        SuiError::UserInputError {
                            error: UserInputError::ObjectVersionUnavailableForConsumption {
                                provided_obj_ref: test_object_ref(),
                                current_version: 10.into(),
                            },
                        },
                        0,
                        vec![],
                    ),
                ],
            };

            let rpc_error: RpcError = Error::QuorumDriverError(quorum_driver_error).into();

            let error_object: ErrorObjectOwned = rpc_error.into();
            let expected_code = expect!["-32002"];
            expected_code.assert_eq(&error_object.code().to_string());
            let expected_message =
                expect!["Transaction validator signing failed due to issues with transaction inputs, please review the errors and try again:\n- Balance of gas object 10 is lower than the needed amount: 100\n- Object ID 0x0000000000000000000000000000000000000000000000000000000000000000 Version 0x0 Digest 11111111111111111111111111111111 is not available for consumption, current version: 0xa"];
            expected_message.assert_eq(error_object.message());
        }

        #[test]
        fn test_non_recoverable_transaction_error_with_transient_errors() {
            let quorum_driver_error = QuorumDriverError::NonRecoverableTransactionError {
                errors: vec![
                    (
                        SuiError::UserInputError {
                            error: UserInputError::ObjectNotFound {
                                object_id: test_object_ref().0,
                                version: None,
                            },
                        },
                        0,
                        vec![],
                    ),
                    (
                        SuiError::RpcError("Hello".to_string(), "Testing".to_string()),
                        0,
                        vec![],
                    ),
                ],
            };

            let rpc_error: RpcError = Error::QuorumDriverError(quorum_driver_error).into();

            let error_object: ErrorObjectOwned = rpc_error.into();
            let expected_code = expect!["-32002"];
            expected_code.assert_eq(&error_object.code().to_string());
            let expected_message =
                expect!["Transaction validator signing failed due to issues with transaction inputs, please review the errors and try again:\n- Could not find the referenced object 0x0000000000000000000000000000000000000000000000000000000000000000 at version None"];
            expected_message.assert_eq(error_object.message());
        }

        #[test]
        fn test_quorum_driver_internal_error() {
            let quorum_driver_error = QuorumDriverError::QuorumDriverInternalError(
                SuiError::UnexpectedMessage("test".to_string()),
            );

            let rpc_error: RpcError = Error::QuorumDriverError(quorum_driver_error).into();

            let error_object: ErrorObjectOwned = rpc_error.into();
            let expected_code = expect!["-32603"];
            expected_code.assert_eq(&error_object.code().to_string());
            let expected_message = expect!["Internal error occurred while executing transaction."];
            expected_message.assert_eq(error_object.message());
        }

        #[test]
        fn test_system_overload() {
            let quorum_driver_error = QuorumDriverError::SystemOverload {
                overloaded_stake: 10,
                errors: vec![(SuiError::UnexpectedMessage("test".to_string()), 0, vec![])],
            };

            let rpc_error: RpcError = Error::QuorumDriverError(quorum_driver_error).into();

            let error_object: ErrorObjectOwned = rpc_error.into();
            let expected_code = expect!["-32050"];
            expected_code.assert_eq(&error_object.code().to_string());
            let expected_message = expect!["Transaction is not processed because 10 of validators by stake are overloaded with certificates pending execution."];
            expected_message.assert_eq(error_object.message());
        }
    }
}
