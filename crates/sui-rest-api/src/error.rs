// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::http::StatusCode;

pub type Result<T, E = RestError> = std::result::Result<T, E>;

#[derive(Debug)]
pub struct RestError {
    status: StatusCode,
    message: Option<String>,
}

impl RestError {
    pub fn new<T: Into<String>>(status: StatusCode, message: T) -> Self {
        Self {
            status,
            message: Some(message.into()),
        }
    }
}

// Tell axum how to convert `AppError` into a response.
impl axum::response::IntoResponse for RestError {
    fn into_response(self) -> axum::response::Response {
        match self.message {
            Some(message) => (self.status, message).into_response(),
            None => self.status.into_response(),
        }
    }
}

impl From<sui_types::storage::error::Error> for RestError {
    fn from(value: sui_types::storage::error::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: Some(value.to_string()),
        }
    }
}

impl From<anyhow::Error> for RestError {
    fn from(value: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: Some(value.to_string()),
        }
    }
}

impl From<sui_types::sui_sdk_types_conversions::SdkTypeConversionError> for RestError {
    fn from(value: sui_types::sui_sdk_types_conversions::SdkTypeConversionError) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: Some(value.to_string()),
        }
    }
}

impl From<bcs::Error> for RestError {
    fn from(value: bcs::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: Some(value.to_string()),
        }
    }
}

impl From<sui_types::quorum_driver_types::QuorumDriverError> for RestError {
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

                RestError::new(StatusCode::BAD_REQUEST, message)
            }
            QuorumDriverInternalError(err) => {
                RestError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
            }
            ObjectsDoubleUsed {
                conflicting_txes,
                retried_tx_status,
            } => {
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
                        "Failed to sign transaction by a quorum of validators because of locked objects. Retried a conflicting transaction {:?}, success: {:?}. Conflicting Transactions:\n{:#?}",
                        retried_tx_status.map(|(tx, _)| tx),
                        retried_tx_status.map(|(_, success)| success),
                        new_map,
                    );

                RestError::new(StatusCode::CONFLICT, message)
            }
            TimeoutBeforeFinality | FailedWithTransientErrorAfterMaximumAttempts { .. } => {
                // TODO add a Retry-After header
                RestError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
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

                RestError::new(StatusCode::BAD_REQUEST, error_msg)
            }
            TxAlreadyFinalizedWithDifferentUserSignatures => RestError::new(
                StatusCode::CONFLICT,
                "The transaction is already finalized but with different user signatures",
            ),
            SystemOverload { .. } | SystemOverloadRetryAfter { .. } => {
                // TODO add a Retry-After header
                RestError::new(StatusCode::SERVICE_UNAVAILABLE, "system is overloaded")
            }
        }
    }
}
