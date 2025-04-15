// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tonic::Code;

use crate::proto::google::rpc::{BadRequest, ErrorInfo, RetryInfo};

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
    details: Option<Box<ErrorDetails>>,
}

impl RpcError {
    pub fn new<T: Into<String>>(code: Code, message: T) -> Self {
        Self {
            code,
            message: Some(message.into()),
            details: None,
        }
    }

    pub fn not_found() -> Self {
        Self {
            code: Code::NotFound,
            message: None,
            details: None,
        }
    }
}

impl From<RpcError> for tonic::Status {
    fn from(value: RpcError) -> Self {
        use prost::Message;

        let status = crate::proto::google::rpc::Status {
            code: value.code.into(),
            message: value.message.unwrap_or_default(),
            details: value
                .details
                .map(ErrorDetails::into_status_details)
                .unwrap_or_default(),
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
            details: None,
        }
    }
}

impl From<anyhow::Error> for RpcError {
    fn from(value: anyhow::Error) -> Self {
        Self {
            code: Code::Internal,
            message: Some(value.to_string()),
            details: None,
        }
    }
}

impl From<sui_types::sui_sdk_types_conversions::SdkTypeConversionError> for RpcError {
    fn from(value: sui_types::sui_sdk_types_conversions::SdkTypeConversionError) -> Self {
        Self {
            code: Code::Internal,
            message: Some(value.to_string()),
            details: None,
        }
    }
}

impl From<bcs::Error> for RpcError {
    fn from(value: bcs::Error) -> Self {
        Self {
            code: Code::Internal,
            message: Some(value.to_string()),
            details: None,
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

//TODO define proto for this
pub enum ErrorReason {
    FieldInvalid,
    FieldMissing,
}

impl ErrorReason {
    fn as_str(&self) -> &'static str {
        match self {
            ErrorReason::FieldInvalid => "FIELD_INVALID",
            ErrorReason::FieldMissing => "FIELD_MISSING",
        }
    }
}

impl AsRef<str> for ErrorReason {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<ErrorReason> for String {
    fn from(value: ErrorReason) -> Self {
        value.as_ref().into()
    }
}

impl From<crate::proto::google::rpc::bad_request::FieldViolation> for RpcError {
    fn from(value: crate::proto::google::rpc::bad_request::FieldViolation) -> Self {
        BadRequest::from(value).into()
    }
}

impl From<BadRequest> for RpcError {
    fn from(value: BadRequest) -> Self {
        let message = value
            .field_violations
            .first()
            .map(|violation| violation.description.clone());
        let details = ErrorDetails::new().with_bad_request(value);

        RpcError {
            code: Code::InvalidArgument,
            message,
            details: Some(Box::new(details)),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ErrorDetails {
    error_info: Option<ErrorInfo>,
    bad_request: Option<BadRequest>,
    retry_info: Option<RetryInfo>,
}

impl ErrorDetails {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn error_info(&self) -> Option<&ErrorInfo> {
        self.error_info.as_ref()
    }

    pub fn bad_request(&self) -> Option<&BadRequest> {
        self.bad_request.as_ref()
    }

    pub fn retry_info(&self) -> Option<&RetryInfo> {
        self.retry_info.as_ref()
    }

    pub fn details(&self) -> &[prost_types::Any] {
        &[]
    }

    pub fn with_bad_request(mut self, bad_request: BadRequest) -> Self {
        self.bad_request = Some(bad_request);
        self
    }

    #[allow(clippy::boxed_local)]
    fn into_status_details(self: Box<Self>) -> Vec<prost_types::Any> {
        let mut details = Vec::new();

        if let Some(error_info) = &self.error_info {
            details.push(
                prost_types::Any::from_msg(error_info).expect("Message encoding cannot fail"),
            );
        }

        if let Some(bad_request) = &self.bad_request {
            details.push(
                prost_types::Any::from_msg(bad_request).expect("Message encoding cannot fail"),
            );
        }

        if let Some(retry_info) = &self.retry_info {
            details.push(
                prost_types::Any::from_msg(retry_info).expect("Message encoding cannot fail"),
            );
        }
        details
    }
}

#[derive(Debug)]
pub struct ObjectNotFoundError {
    object_id: sui_sdk_types::ObjectId,
    version: Option<sui_sdk_types::Version>,
}

impl ObjectNotFoundError {
    pub fn new(object_id: sui_sdk_types::ObjectId) -> Self {
        Self {
            object_id,
            version: None,
        }
    }

    pub fn new_with_version(
        object_id: sui_sdk_types::ObjectId,
        version: sui_sdk_types::Version,
    ) -> Self {
        Self {
            object_id,
            version: Some(version),
        }
    }
}

impl std::fmt::Display for ObjectNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Object {}", self.object_id)?;

        if let Some(version) = self.version {
            write!(f, " with version {version}")?;
        }

        write!(f, " not found")
    }
}

impl std::error::Error for ObjectNotFoundError {}

impl From<ObjectNotFoundError> for crate::RpcError {
    fn from(value: ObjectNotFoundError) -> Self {
        Self::new(tonic::Code::NotFound, value.to_string())
    }
}

#[derive(Debug)]
pub struct CheckpointNotFoundError {
    sequence_number: Option<u64>,
    digest: Option<sui_sdk_types::CheckpointDigest>,
}

impl CheckpointNotFoundError {
    pub fn sequence_number(sequence_number: u64) -> Self {
        Self {
            sequence_number: Some(sequence_number),
            digest: None,
        }
    }

    pub fn digest(digest: sui_sdk_types::CheckpointDigest) -> Self {
        Self {
            sequence_number: None,
            digest: Some(digest),
        }
    }
}

impl std::fmt::Display for CheckpointNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Checkpoint ")?;

        if let Some(s) = self.sequence_number {
            write!(f, "{s} ")?;
        }

        if let Some(d) = &self.digest {
            write!(f, "{d} ")?;
        }

        write!(f, "not found")
    }
}

impl std::error::Error for CheckpointNotFoundError {}

impl From<CheckpointNotFoundError> for crate::RpcError {
    fn from(value: CheckpointNotFoundError) -> Self {
        Self::new(tonic::Code::NotFound, value.to_string())
    }
}
