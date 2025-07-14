// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::{
    committee::EpochId,
    digests::{TransactionDigest, TransactionEffectsDigest},
    effects::{TransactionEffects, TransactionEvents},
    error::SuiError,
    messages_consensus::{ConsensusPosition, Round},
    messages_grpc::{
        RawExecutedData, RawExecutedStatus, RawRejectReason, RawRejectedStatus,
        RawValidatorTransactionStatus, RawWaitForEffectsRequest, RawWaitForEffectsResponse,
    },
    object::Object,
};

pub(crate) struct WaitForEffectsRequest {
    pub epoch: EpochId,
    pub transaction_digest: TransactionDigest,
    pub transaction_position: ConsensusPosition,
    /// Whether to include details of the effects,
    /// including the effects content, events, input objects, and output objects.
    pub include_details: bool,
}

#[derive(Clone)]
pub struct ExecutedData {
    pub effects: TransactionEffects,
    pub events: Option<TransactionEvents>,
    pub input_objects: Vec<Object>,
    pub output_objects: Vec<Object>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectReason {
    // Transaction is not voted to be rejected locally.
    None,
    // Rejected due to lock conflict.
    LockConflict(String),
    // Rejected due to package verification.
    PackageVerification(String),
    // Rejected due to overload.
    Overload(String),
    // Rejected due to coin deny list.
    CoinDenyList,
}

impl std::fmt::Display for RejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RejectReason::None => write!(f, "Rejected with no reason"),
            RejectReason::LockConflict(msg) => write!(f, "Lock conflict: {}", msg),
            RejectReason::PackageVerification(msg) => {
                write!(f, "Package verification failed: {}", msg)
            }
            RejectReason::Overload(msg) => write!(f, "Overload: {}", msg),
            RejectReason::CoinDenyList => write!(f, "Coin deny list"),
        }
    }
}

pub enum WaitForEffectsResponse {
    Executed {
        effects_digest: TransactionEffectsDigest,
        details: Option<Box<ExecutedData>>,
    },
    Rejected {
        // The rejection reason known locally.
        reason: RejectReason,
    },
    // The transaction position is expired at the committed round.
    Expired(Round),
}

impl TryFrom<RawWaitForEffectsRequest> for WaitForEffectsRequest {
    type Error = SuiError;

    fn try_from(value: RawWaitForEffectsRequest) -> Result<Self, Self::Error> {
        let transaction_digest = bcs::from_bytes(&value.transaction_digest).map_err(|err| {
            SuiError::GrpcMessageDeserializeError {
                type_info: "RawWaitForEffectsRequest.transaction_digest".to_string(),
                error: err.to_string(),
            }
        })?;
        let transaction_position = bcs::from_bytes(&value.transaction_position).map_err(|err| {
            SuiError::GrpcMessageDeserializeError {
                type_info: "RawWaitForEffectsRequest.transaction_position".to_string(),
                error: err.to_string(),
            }
        })?;
        Ok(Self {
            epoch: value.epoch,
            transaction_digest,
            transaction_position,
            include_details: value.include_details,
        })
    }
}

impl TryFrom<RawWaitForEffectsResponse> for WaitForEffectsResponse {
    type Error = SuiError;

    fn try_from(value: RawWaitForEffectsResponse) -> Result<Self, Self::Error> {
        match value.inner {
            Some(RawValidatorTransactionStatus::Executed(executed)) => {
                let effects_digest = bcs::from_bytes(&executed.effects_digest).map_err(|err| {
                    SuiError::GrpcMessageDeserializeError {
                        type_info: "RawWaitForEffectsResponse.effects_digest".to_string(),
                        error: err.to_string(),
                    }
                })?;
                let details = if let Some(details) = executed.details {
                    let effects = bcs::from_bytes(&details.effects).map_err(|err| {
                        SuiError::GrpcMessageDeserializeError {
                            type_info: "RawWaitForEffectsResponse.details.effects".to_string(),
                            error: err.to_string(),
                        }
                    })?;
                    let events = if let Some(events) = details.events {
                        Some(bcs::from_bytes(&events).map_err(|err| {
                            SuiError::GrpcMessageDeserializeError {
                                type_info: "RawWaitForEffectsResponse.details.events".to_string(),
                                error: err.to_string(),
                            }
                        })?)
                    } else {
                        None
                    };
                    let mut input_objects = Vec::with_capacity(details.input_objects.len());
                    for object in details.input_objects {
                        input_objects.push(bcs::from_bytes(&object).map_err(|err| {
                            SuiError::GrpcMessageDeserializeError {
                                type_info: "RawWaitForEffectsResponse.input_objects".to_string(),
                                error: err.to_string(),
                            }
                        })?);
                    }
                    let mut output_objects = Vec::with_capacity(details.output_objects.len());
                    for object in details.output_objects {
                        output_objects.push(bcs::from_bytes(&object).map_err(|err| {
                            SuiError::GrpcMessageDeserializeError {
                                type_info: "RawWaitForEffectsResponse.output_objects".to_string(),
                                error: err.to_string(),
                            }
                        })?);
                    }
                    Some(Box::new(ExecutedData {
                        effects,
                        events,
                        input_objects,
                        output_objects,
                    }))
                } else {
                    None
                };
                Ok(Self::Executed {
                    effects_digest,
                    details,
                })
            }
            Some(RawValidatorTransactionStatus::Rejected(rejected)) => {
                let raw_reason = RawRejectReason::try_from(rejected.reason).map_err(|err| {
                    SuiError::GrpcMessageDeserializeError {
                        type_info: "RawWaitForEffectsResponse.rejected.reason".to_string(),
                        error: err.to_string(),
                    }
                })?;
                let reason = match raw_reason {
                    RawRejectReason::None => RejectReason::None,
                    RawRejectReason::LockConflict => {
                        RejectReason::LockConflict(rejected.message.unwrap_or_default())
                    }
                    RawRejectReason::PackageVerification => {
                        RejectReason::PackageVerification(rejected.message.unwrap_or_default())
                    }
                    RawRejectReason::Overload => {
                        RejectReason::Overload(rejected.message.unwrap_or_default())
                    }
                    RawRejectReason::CoinDenyList => RejectReason::CoinDenyList,
                };
                Ok(Self::Rejected { reason })
            }
            Some(RawValidatorTransactionStatus::Expired(round)) => Ok(Self::Expired(round)),
            None => Err(SuiError::GrpcMessageDeserializeError {
                type_info: "RawWaitForEffectsResponse.inner".to_string(),
                error: "RawWaitForEffectsResponse.inner is None".to_string(),
            }),
        }
    }
}

impl TryFrom<WaitForEffectsRequest> for RawWaitForEffectsRequest {
    type Error = SuiError;

    fn try_from(value: WaitForEffectsRequest) -> Result<Self, Self::Error> {
        let transaction_digest = bcs::to_bytes(&value.transaction_digest)
            .map_err(|err| SuiError::GrpcMessageSerializeError {
                type_info: "RawWaitForEffectsRequest.transaction_digest".to_string(),
                error: err.to_string(),
            })?
            .into();
        let transaction_position = bcs::to_bytes(&value.transaction_position)
            .map_err(|err| SuiError::GrpcMessageSerializeError {
                type_info: "RawWaitForEffectsRequest.transaction_position".to_string(),
                error: err.to_string(),
            })?
            .into();
        Ok(Self {
            epoch: value.epoch,
            transaction_digest,
            transaction_position,
            include_details: value.include_details,
        })
    }
}

impl TryFrom<WaitForEffectsResponse> for RawWaitForEffectsResponse {
    type Error = SuiError;

    fn try_from(value: WaitForEffectsResponse) -> Result<Self, Self::Error> {
        let inner = match value {
            WaitForEffectsResponse::Executed {
                effects_digest,
                details,
            } => {
                let effects_digest = bcs::to_bytes(&effects_digest)
                    .map_err(|err| SuiError::GrpcMessageSerializeError {
                        type_info: "RawWaitForEffectsResponse.effects_digest".to_string(),
                        error: err.to_string(),
                    })?
                    .into();
                let details = if let Some(details) = details {
                    let effects = bcs::to_bytes(&details.effects)
                        .map_err(|err| SuiError::GrpcMessageSerializeError {
                            type_info: "RawWaitForEffectsResponse.details.effects".to_string(),
                            error: err.to_string(),
                        })?
                        .into();
                    let events = if let Some(events) = &details.events {
                        Some(
                            bcs::to_bytes(events)
                                .map_err(|err| SuiError::GrpcMessageSerializeError {
                                    type_info: "RawWaitForEffectsResponse.details.events"
                                        .to_string(),
                                    error: err.to_string(),
                                })?
                                .into(),
                        )
                    } else {
                        None
                    };
                    let mut input_objects = Vec::with_capacity(details.input_objects.len());
                    for object in details.input_objects {
                        input_objects.push(
                            bcs::to_bytes(&object)
                                .map_err(|err| SuiError::GrpcMessageSerializeError {
                                    type_info: "RawWaitForEffectsResponse.input_objects"
                                        .to_string(),
                                    error: err.to_string(),
                                })?
                                .into(),
                        );
                    }
                    let mut output_objects = Vec::with_capacity(details.output_objects.len());
                    for object in details.output_objects {
                        output_objects.push(
                            bcs::to_bytes(&object)
                                .map_err(|err| SuiError::GrpcMessageSerializeError {
                                    type_info: "RawWaitForEffectsResponse.output_objects"
                                        .to_string(),
                                    error: err.to_string(),
                                })?
                                .into(),
                        );
                    }
                    Some(RawExecutedData {
                        effects,
                        events,
                        input_objects,
                        output_objects,
                    })
                } else {
                    None
                };
                RawValidatorTransactionStatus::Executed(RawExecutedStatus {
                    effects_digest,
                    details,
                })
            }
            WaitForEffectsResponse::Rejected { reason } => {
                let (reason, message) = match reason {
                    RejectReason::None => (RawRejectReason::None, None),
                    RejectReason::LockConflict(message) => {
                        (RawRejectReason::LockConflict, Some(message))
                    }
                    RejectReason::PackageVerification(message) => {
                        (RawRejectReason::PackageVerification, Some(message))
                    }
                    RejectReason::Overload(message) => (RawRejectReason::Overload, Some(message)),
                    RejectReason::CoinDenyList => (RawRejectReason::CoinDenyList, None),
                };
                RawValidatorTransactionStatus::Rejected(RawRejectedStatus {
                    reason: reason as i32,
                    message,
                })
            }
            WaitForEffectsResponse::Expired(round) => RawValidatorTransactionStatus::Expired(round),
        };
        Ok(RawWaitForEffectsResponse { inner: Some(inner) })
    }
}
