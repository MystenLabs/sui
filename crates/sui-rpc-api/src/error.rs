// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tonic::Code;

pub type Result<T, E = RpcError> = std::result::Result<T, E>;

/// An error encountered while serving an RPC request.
///
/// General error type used by top-level RPC service methods. The main purpose of this error type
/// is to provide a convenient type for converting between internal errors and a response that
/// needs to be sent to a calling client.
#[derive(Debug)]
pub struct RpcError {
    code: Code,
    message: Option<String>,
}

impl RpcError {
    pub fn new<T: Into<String>>(code: Code, message: T) -> Self {
        Self {
            code,
            message: Some(message.into()),
        }
    }

    pub fn not_found() -> Self {
        Self {
            code: Code::NotFound,
            message: None,
        }
    }
}

impl From<RpcError> for tonic::Status {
    fn from(value: RpcError) -> Self {
        use prost::Message;

        let status = crate::proto::google::rpc::Status {
            code: value.code.into(),
            message: value.message.unwrap_or_default(),
            details: Default::default(),
        };

        let code = value.code;
        let details = status.encode_to_vec().into();
        let message = status.message;

        tonic::Status::with_details(code, message, details)
    }
}

impl From<sui_types::storage::error::Error> for RpcError {
    fn from(value: sui_types::storage::error::Error) -> Self {
        Self {
            code: Code::Internal,
            message: Some(value.to_string()),
        }
    }
}

impl From<anyhow::Error> for RpcError {
    fn from(value: anyhow::Error) -> Self {
        Self {
            code: Code::Internal,
            message: Some(value.to_string()),
        }
    }
}

impl From<sui_types::sui_sdk_types_conversions::SdkTypeConversionError> for RpcError {
    fn from(value: sui_types::sui_sdk_types_conversions::SdkTypeConversionError) -> Self {
        Self {
            code: Code::Internal,
            message: Some(value.to_string()),
        }
    }
}

impl From<bcs::Error> for RpcError {
    fn from(value: bcs::Error) -> Self {
        Self {
            code: Code::Internal,
            message: Some(value.to_string()),
        }
    }
}

impl From<sui_types::quorum_driver_types::QuorumDriverError> for RpcError {
    fn from(error: sui_types::quorum_driver_types::QuorumDriverError) -> Self {
        use itertools::Itertools;
        use sui_types::error::SuiError;
        use sui_types::quorum_driver_types::QuorumDriverError::*;

        match error {
            InvalidUserSignature(err) => {
                let message = {
                    let err = match err {
                        SuiError::UserInputError { error } => error.to_string(),
                        _ => err.to_string(),
                    };
                    format!("Invalid user signature: {err}")
                };

                RpcError::new(Code::InvalidArgument, message)
            }
            QuorumDriverInternalError(err) => RpcError::new(Code::Internal, err.to_string()),
            ObjectsDoubleUsed { conflicting_txes } => {
                let new_map = conflicting_txes
                    .into_iter()
                    .map(|(digest, (pairs, _))| {
                        (
                            digest,
                            pairs.into_iter().map(|(_, obj_ref)| obj_ref).collect(),
                        )
                    })
                    .collect::<std::collections::BTreeMap<_, Vec<_>>>();

                let message = format!(
                        "Failed to sign transaction by a quorum of validators because of locked objects. Conflicting Transactions:\n{new_map:#?}",  
                    );

                RpcError::new(Code::FailedPrecondition, message)
            }
            TimeoutBeforeFinality | FailedWithTransientErrorAfterMaximumAttempts { .. } => {
                // TODO add a Retry-After header
                RpcError::new(
                    Code::Unavailable,
                    "timed-out before finality could be reached",
                )
            }
            NonRecoverableTransactionError { errors } => {
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

                let error_list = new_errors.join(", ");
                let error_msg = format!("Transaction execution failed due to issues with transaction inputs, please review the errors and try again: {}.", error_list);

                RpcError::new(Code::InvalidArgument, error_msg)
            }
            TxAlreadyFinalizedWithDifferentUserSignatures => RpcError::new(
                Code::Aborted,
                "The transaction is already finalized but with different user signatures",
            ),
            SystemOverload { .. } | SystemOverloadRetryAfter { .. } => {
                // TODO add a Retry-After header
                RpcError::new(Code::Unavailable, "system is overloaded")
            }
        }
    }
}
