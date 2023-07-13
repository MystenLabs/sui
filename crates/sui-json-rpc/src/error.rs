// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::error::FastCryptoError;
use hyper::header::InvalidHeaderValue;
use jsonrpsee::core::Error as RpcError;
use jsonrpsee::types::error::{CallError, INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE};
use jsonrpsee::types::ErrorObject;
use std::collections::BTreeMap;
use sui_types::base_types::ObjectRef;
use sui_types::digests::TransactionDigest;
use sui_types::error::{SuiError, SuiObjectResponseError, UserInputError};
use sui_types::quorum_driver_types::{QuorumDriverError, NON_RECOVERABLE_ERROR_MSG};
use thiserror::Error;
use tokio::task::JoinError;

use crate::authority_state::StateReadError;

pub const TRANSIENT_ERROR_CODE: i32 = -32001;

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
}

impl From<Error> for RpcError {
    fn from(e: Error) -> Self {
        e.to_rpc_error()
    }
}

impl From<SuiError> for Error {
    fn from(e: SuiError) -> Self {
        match e {
            SuiError::UserInputError { error } => Self::UserInputError(error),
            SuiError::SuiObjectResponseError { error } => Self::SuiObjectResponseError(error),
            other => Self::SuiError(other),
        }
    }
}

impl Error {
    /// `InvalidParams`/`INVALID_PARAMS_CODE` for client errors.
    pub fn to_rpc_error(self) -> RpcError {
        match self {
            Error::UserInputError(_) => RpcError::Call(CallError::InvalidParams(self.into())),
            Error::SuiObjectResponseError(err) => match err {
                SuiObjectResponseError::NotExists { .. }
                | SuiObjectResponseError::DynamicFieldNotFound { .. }
                | SuiObjectResponseError::Deleted { .. }
                | SuiObjectResponseError::DisplayError { .. } => {
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
            Error::QuorumDriverError(err) => match err {
                QuorumDriverError::NonRecoverableTransactionError { errors } => {
                    // Note: we probably want a more precise error than `INVALID_PARAMS_CODE`
                    // but to keep the error code consistent we still use `INVALID_PARAMS_CODE`
                    let error_object = ErrorObject::owned(
                        jsonrpsee::types::error::INVALID_PARAMS_CODE,
                        NON_RECOVERABLE_ERROR_MSG,
                        Some(errors),
                    );
                    RpcError::Call(CallError::Custom(error_object))
                }
                _ => RpcError::Call(CallError::Failed(err.into())),
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
            _ => RpcError::Call(CallError::Failed(self.into())),
        }
    }
}

pub fn match_quorum_driver_error(err: QuorumDriverError) -> RpcError {
    match err {
        QuorumDriverError::InvalidUserSignature(err) => {
            let inner_error_str = match err {
                // Use inner UserInputError's Display instead of SuiError::UserInputError, which renders UserInputError in debug format
                SuiError::UserInputError { error } => error.to_string(),
                _ => err.to_string(),
            };

            let error_message = format!("Invalid user signature: {inner_error_str}");

            let error_object = ErrorObject::owned(INVALID_PARAMS_CODE, error_message, None::<()>);
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
            retried_tx,
            retried_tx_success,
        } => {
            let error_message = format!(
                "Failed to sign transaction by a quorum of validators because of locked objects. Retried a conflicting transaction {:?}, success: {:?}",
                retried_tx,
                retried_tx_success
            );

            let mut new_map: BTreeMap<TransactionDigest, Vec<ObjectRef>> = BTreeMap::new();

            for (digest, (pairs, _)) in conflicting_txes {
                let mut new_vec = Vec::new();

                for (_authority, obj_ref) in pairs {
                    new_vec.push(obj_ref);
                }

                new_map.insert(digest, new_vec);
            }

            let error_object =
                ErrorObject::owned(INVALID_PARAMS_CODE, error_message, Some(new_map));
            RpcError::Call(CallError::Custom(error_object))
        }
        QuorumDriverError::NonRecoverableTransactionError { errors } => {
            let mut new_errors: Vec<String> = errors
                .into_iter()
                .filter(|(err, _, _)| !err.is_retryable().0) // consider retryable errors as transient errors
                .map(|(err, _, _)| {
                    match err {
                        // Use inner UserInputError's Display instead of SuiError::UserInputError, which renders UserInputError in debug format
                        SuiError::UserInputError { error } => error.to_string(),
                        _ => err.to_string(),
                    }
                })
                .collect();

            if new_errors.is_empty() {
                new_errors.push(
                    "Transient errors occurred during execution. Please try again.".to_string(),
                );
            }

            let error_object = ErrorObject::owned(
                INVALID_PARAMS_CODE,
                NON_RECOVERABLE_ERROR_MSG,
                Some(new_errors),
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
        QuorumDriverError::SystemOverload { .. } => {
            let error_object = ErrorObject::owned(INTERNAL_ERROR_CODE, err.to_string(), None::<()>);
            RpcError::Call(CallError::Custom(error_object))
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
    use sui_types::base_types::{random_object_ref, AuthorityName};
    use sui_types::committee::StakeUnit;
    use sui_types::crypto::AuthorityPublicKey;
    use sui_types::crypto::AuthorityPublicKeyBytes;

    mod match_quorum_driver_error_tests {
        use super::*;

        #[test]
        fn test_invalid_user_signature() {
            let quorum_driver_error =
                QuorumDriverError::InvalidUserSignature(SuiError::InvalidSignature {
                    error: "Test inner invalid signature".to_string(),
                });

            let rpc_error = match_quorum_driver_error(quorum_driver_error);

            let error_object: ErrorObjectOwned = rpc_error.into();
            let expected_code = expect!["-32602"];
            expected_code.assert_eq(&error_object.code().to_string());
            let expected_message = expect![
                "Invalid user signature: Signature is not valid: Test inner invalid signature"
            ];
            expected_message.assert_eq(error_object.message());
        }

        #[test]
        fn test_timeout_before_finality() {
            let quorum_driver_error = QuorumDriverError::TimeoutBeforeFinality;

            let rpc_error = match_quorum_driver_error(quorum_driver_error);

            let error_object: ErrorObjectOwned = rpc_error.into();
            let expected_code = expect!["-32001"];
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

            let rpc_error = match_quorum_driver_error(quorum_driver_error);

            let error_object: ErrorObjectOwned = rpc_error.into();
            let expected_code = expect!["-32001"];
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
            let tx_digest = TransactionDigest::default();
            let object_ref = random_object_ref();
            let stake_unit: StakeUnit = 10;
            let authority_name = AuthorityPublicKeyBytes([0; AuthorityPublicKey::LENGTH]);
            conflicting_txes.insert(tx_digest, (vec![(authority_name, object_ref)], stake_unit));

            let quorum_driver_error = QuorumDriverError::ObjectsDoubleUsed {
                conflicting_txes,
                retried_tx: Some(TransactionDigest::default()),
                retried_tx_success: Some(true),
            };

            let rpc_error = match_quorum_driver_error(quorum_driver_error);

            let error_object: ErrorObjectOwned = rpc_error.into();
            let expected_code = expect!["-32602"];
            expected_code.assert_eq(&error_object.code().to_string());
            let expected_message = expect!["Failed to sign transaction by a quorum of validators because of locked objects. Retried a conflicting transaction Some(TransactionDigest(11111111111111111111111111111111)), success: Some(true)"];
            expected_message.assert_eq(error_object.message());
            let expected_data = expect![[
                r#"{"11111111111111111111111111111111":[["0x7da78de297e53bf42590250788d7e5e0e3ff355330ea1393d4d7dce86682ec7b",0,"11111111111111111111111111111111"]]}"#
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
                                provided_obj_ref: random_object_ref(),
                                current_version: 10.into(),
                            },
                        },
                        0,
                        vec![],
                    ),
                ],
            };

            let rpc_error = match_quorum_driver_error(quorum_driver_error);

            let error_object: ErrorObjectOwned = rpc_error.into();
            let expected_code = expect!["-32602"];
            expected_code.assert_eq(&error_object.code().to_string());
            let expected_message =
                expect!["Transaction has non recoverable errors from at least 1/3 of validators"];
            expected_message.assert_eq(error_object.message());
            let expected_data = expect![[
                r#"["Balance of gas object 10 is lower than the needed amount: 100.","Object (0xcf9722a6c9421d8c5b1df13455da03d81e0372bd8ac982a3c0092112235fef14, SequenceNumber(0), o#11111111111111111111111111111111) is not available for consumption, its current version: SequenceNumber(10)."]"#
            ]];
            let actual_data = error_object.data().unwrap().to_string();
            expected_data.assert_eq(&actual_data);
        }

        #[test]
        fn test_non_recoverable_transaction_error_with_transient_errors() {
            let quorum_driver_error = QuorumDriverError::NonRecoverableTransactionError {
                errors: vec![
                    (
                        SuiError::UserInputError {
                            error: UserInputError::ObjectNotFound {
                                object_id: random_object_ref().0,
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

            let rpc_error = match_quorum_driver_error(quorum_driver_error);

            let error_object: ErrorObjectOwned = rpc_error.into();
            let expected_code = expect!["-32602"];
            expected_code.assert_eq(&error_object.code().to_string());
            let expected_message =
                expect!["Transaction has non recoverable errors from at least 1/3 of validators"];
            expected_message.assert_eq(error_object.message());
            let expected_data =
                expect![[r#"["Transient errors occurred during execution. Please try again."]"#]];
            let actual_data = error_object.data().unwrap().to_string();
            expected_data.assert_eq(&actual_data);
        }

        #[test]
        fn test_quorum_driver_internal_error() {
            let quorum_driver_error =
                QuorumDriverError::QuorumDriverInternalError(SuiError::UnexpectedMessage);

            let rpc_error = match_quorum_driver_error(quorum_driver_error);

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
                errors: vec![(SuiError::UnexpectedMessage, 0, vec![])],
            };

            let rpc_error = match_quorum_driver_error(quorum_driver_error);

            let error_object: ErrorObjectOwned = rpc_error.into();
            let expected_code = expect!["-32603"];
            expected_code.assert_eq(&error_object.code().to_string());
            let expected_message = expect!["Transaction is not processed because 10 of validators by stake are overloaded with certificates pending execution."];
            expected_message.assert_eq(error_object.message());
        }
    }
}
