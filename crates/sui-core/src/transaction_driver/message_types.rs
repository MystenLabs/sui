// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use sui_types::{
    digests::{TransactionDigest, TransactionEffectsDigest},
    effects::{TransactionEffects, TransactionEvents},
    error::SuiError,
    messages_consensus::ConsensusPosition,
    messages_grpc::{
        RawExecutedData, RawExecutedStatus, RawExpiredStatus, RawRejectedStatus,
        RawSubmitTxRequest, RawSubmitTxResponse, RawValidatorSubmitStatus,
        RawValidatorTransactionStatus, RawWaitForEffectsRequest, RawWaitForEffectsResponse,
    },
    object::Object,
    quorum_driver_types::FinalizedEffects,
    transaction::Transaction,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SubmitTxRequest {
    pub transaction: Transaction,
}

impl SubmitTxRequest {
    pub fn into_raw(&self) -> Result<RawSubmitTxRequest, SuiError> {
        Ok(RawSubmitTxRequest {
            transaction: bcs::to_bytes(&self.transaction)
                .map_err(|e| SuiError::TransactionSerializationError {
                    error: e.to_string(),
                })?
                .into(),
        })
    }
}

pub enum SubmitTxResponse {
    Submitted {
        consensus_position: ConsensusPosition,
    },
    Executed {
        effects_digest: TransactionEffectsDigest,
        // Response should always include details for executed transactions.
        // TODO(fastpath): validate this field is always present and return an error during deserialization.
        details: Option<Box<ExecutedData>>,
    },
}

impl TryFrom<RawSubmitTxResponse> for SubmitTxResponse {
    type Error = SuiError;

    fn try_from(value: RawSubmitTxResponse) -> Result<Self, Self::Error> {
        match value.inner {
            Some(RawValidatorSubmitStatus::Submitted(consensus_position)) => Ok(Self::Submitted {
                consensus_position: consensus_position.as_ref().try_into()?,
            }),
            Some(RawValidatorSubmitStatus::Executed(executed)) => {
                let (effects_digest, details) = try_from_raw_executed_status(executed)?;
                Ok(Self::Executed {
                    effects_digest,
                    details,
                })
            }
            None => Err(SuiError::GrpcMessageDeserializeError {
                type_info: "RawSubmitTxResponse.inner".to_string(),
                error: "RawSubmitTxResponse.inner is None".to_string(),
            }),
        }
    }
}

impl TryFrom<SubmitTxResponse> for RawSubmitTxResponse {
    type Error = SuiError;

    fn try_from(value: SubmitTxResponse) -> Result<Self, Self::Error> {
        let inner = match value {
            SubmitTxResponse::Submitted { consensus_position } => {
                let consensus_position = consensus_position.into_raw()?;
                RawValidatorSubmitStatus::Submitted(consensus_position)
            }
            SubmitTxResponse::Executed {
                effects_digest,
                details,
            } => {
                let raw_executed = try_from_response_executed(effects_digest, details)?;
                RawValidatorSubmitStatus::Executed(raw_executed)
            }
        };
        Ok(RawSubmitTxResponse { inner: Some(inner) })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QuorumTransactionResponse {
    // TODO(fastpath): Stop using QD types
    pub effects: FinalizedEffects,

    pub events: Option<TransactionEvents>,
    // Input objects will only be populated in the happy path
    pub input_objects: Option<Vec<Object>>,
    // Output objects will only be populated in the happy path
    pub output_objects: Option<Vec<Object>>,
    pub auxiliary_data: Option<Vec<u8>>,
}

pub(crate) struct WaitForEffectsRequest {
    pub transaction_digest: TransactionDigest,
    /// If consensus position is provided, waits in the server handler for the transaction in it to execute,
    /// either in fastpath outputs or finalized.
    /// If it is not provided, only waits for finalized effects of the transaction in the server handler,
    /// but not for fastpath outputs.
    pub consensus_position: Option<ConsensusPosition>,
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

pub enum WaitForEffectsResponse {
    Executed {
        effects_digest: TransactionEffectsDigest,
        details: Option<Box<ExecutedData>>,
    },
    Rejected {
        // The rejection status known locally.
        error: SuiError,
    },
    // The transaction position is expired, with the local epoch and committed round.
    // When round is None, the expiration is due to lagging epoch in the request.
    Expired {
        epoch: u64,
        round: Option<u32>,
    },
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
        let consensus_position = match value.consensus_position {
            Some(cp) => Some(cp.as_ref().try_into()?),
            None => None,
        };
        Ok(Self {
            consensus_position,
            transaction_digest,
            include_details: value.include_details,
        })
    }
}

impl TryFrom<RawWaitForEffectsResponse> for WaitForEffectsResponse {
    type Error = SuiError;

    fn try_from(value: RawWaitForEffectsResponse) -> Result<Self, Self::Error> {
        match value.inner {
            Some(RawValidatorTransactionStatus::Executed(executed)) => {
                let (effects_digest, details) = try_from_raw_executed_status(executed)?;
                Ok(Self::Executed {
                    effects_digest,
                    details,
                })
            }
            Some(RawValidatorTransactionStatus::Rejected(rejected)) => {
                let error = try_from_raw_rejected_status(rejected)?;
                Ok(Self::Rejected { error })
            }
            Some(RawValidatorTransactionStatus::Expired(expired)) => Ok(Self::Expired {
                epoch: expired.epoch,
                round: expired.round,
            }),
            None => Err(SuiError::GrpcMessageDeserializeError {
                type_info: "RawWaitForEffectsResponse.inner".to_string(),
                error: "RawWaitForEffectsResponse.inner is None".to_string(),
            }),
        }
    }
}

fn try_from_raw_executed_status(
    executed: RawExecutedStatus,
) -> Result<(TransactionEffectsDigest, Option<Box<ExecutedData>>), SuiError> {
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
            Some(
                bcs::from_bytes(&events).map_err(|err| SuiError::GrpcMessageDeserializeError {
                    type_info: "RawWaitForEffectsResponse.details.events".to_string(),
                    error: err.to_string(),
                })?,
            )
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
    Ok((effects_digest, details))
}

fn try_from_raw_rejected_status(rejected: RawRejectedStatus) -> Result<SuiError, SuiError> {
    let error =
        bcs::from_bytes(&rejected.error).map_err(|err| SuiError::GrpcMessageDeserializeError {
            type_info: "RawWaitForEffectsResponse.rejected.reason".to_string(),
            error: err.to_string(),
        })?;
    Ok(error)
}

fn try_from_response_rejected(error: SuiError) -> Result<RawRejectedStatus, SuiError> {
    let error = bcs::to_bytes(&error)
        .map_err(|err| SuiError::GrpcMessageSerializeError {
            type_info: "RawRejectedStatus.error".to_string(),
            error: err.to_string(),
        })?
        .into();
    Ok(RawRejectedStatus { error })
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
        let consensus_position = match value.consensus_position {
            Some(cp) => Some(cp.into_raw()?),
            None => None,
        };
        Ok(Self {
            consensus_position,
            transaction_digest,
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
                let raw_executed = try_from_response_executed(effects_digest, details)?;
                RawValidatorTransactionStatus::Executed(raw_executed)
            }
            WaitForEffectsResponse::Rejected { error } => {
                let raw_rejected = try_from_response_rejected(error)?;
                RawValidatorTransactionStatus::Rejected(raw_rejected)
            }
            WaitForEffectsResponse::Expired { epoch, round } => {
                RawValidatorTransactionStatus::Expired(RawExpiredStatus { epoch, round })
            }
        };
        Ok(RawWaitForEffectsResponse { inner: Some(inner) })
    }
}

fn try_from_response_executed(
    effects_digest: TransactionEffectsDigest,
    details: Option<Box<ExecutedData>>,
) -> Result<RawExecutedStatus, SuiError> {
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
                        type_info: "RawWaitForEffectsResponse.details.events".to_string(),
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
                        type_info: "RawWaitForEffectsResponse.input_objects".to_string(),
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
                        type_info: "RawWaitForEffectsResponse.output_objects".to_string(),
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
    Ok(RawExecutedStatus {
        effects_digest,
        details,
    })
}
