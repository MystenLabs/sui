// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::fmt;

use sui_types::{
    digests::{TransactionDigest, TransactionEffectsDigest},
    effects::{TransactionEffects, TransactionEvents},
    error::SuiError,
    messages_consensus::ConsensusPosition,
    messages_grpc::{
        RawExecutedData, RawExecutedStatus, RawExpiredStatus, RawPingType, RawRejectedStatus,
        RawSubmitTxRequest, RawSubmitTxResponse, RawSubmitTxResult, RawValidatorSubmitStatus,
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
            transactions: vec![bcs::to_bytes(&self.transaction)
                .map_err(|e| SuiError::TransactionSerializationError {
                    error: e.to_string(),
                })?
                .into()],
            ..Default::default()
        })
    }
}

#[derive(Clone)]
pub enum SubmitTxResult {
    Submitted {
        consensus_position: ConsensusPosition,
    },
    Executed {
        effects_digest: TransactionEffectsDigest,
        // Response should always include details for executed transactions.
        // TODO(fastpath): validate this field is always present and return an error during deserialization.
        details: Option<Box<ExecutedData>>,
        // Whether the transaction was executed using fast path.
        fast_path: bool,
    },
    Rejected {
        error: SuiError,
    },
}

impl fmt::Debug for SubmitTxResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Submitted { consensus_position } => f
                .debug_struct("Submitted")
                .field("consensus_position", consensus_position)
                .finish(),
            Self::Executed {
                effects_digest,
                fast_path,
                ..
            } => f
                .debug_struct("Executed")
                .field("effects_digest", &format_args!("{}", effects_digest))
                .field("fast_path", fast_path)
                .finish(),
            Self::Rejected { error } => f.debug_struct("Rejected").field("error", &error).finish(),
        }
    }
}

impl TryFrom<SubmitTxResult> for RawSubmitTxResult {
    type Error = SuiError;

    fn try_from(value: SubmitTxResult) -> Result<Self, Self::Error> {
        let inner = match value {
            SubmitTxResult::Submitted { consensus_position } => {
                let consensus_position = consensus_position.into_raw()?;
                RawValidatorSubmitStatus::Submitted(consensus_position)
            }
            SubmitTxResult::Executed {
                effects_digest,
                details,
                fast_path,
            } => {
                let raw_executed = try_from_response_executed(effects_digest, details, fast_path)?;
                RawValidatorSubmitStatus::Executed(raw_executed)
            }
            SubmitTxResult::Rejected { error } => {
                RawValidatorSubmitStatus::Rejected(try_from_response_rejected(Some(error))?)
            }
        };
        Ok(RawSubmitTxResult { inner: Some(inner) })
    }
}

impl TryFrom<RawSubmitTxResult> for SubmitTxResult {
    type Error = SuiError;

    fn try_from(value: RawSubmitTxResult) -> Result<Self, Self::Error> {
        match value.inner {
            Some(RawValidatorSubmitStatus::Submitted(consensus_position)) => {
                Ok(SubmitTxResult::Submitted {
                    consensus_position: consensus_position.as_ref().try_into()?,
                })
            }
            Some(RawValidatorSubmitStatus::Executed(executed)) => {
                let (effects_digest, details, fast_path) = try_from_raw_executed_status(executed)?;
                Ok(SubmitTxResult::Executed {
                    effects_digest,
                    details,
                    fast_path,
                })
            }
            Some(RawValidatorSubmitStatus::Rejected(error)) => {
                let error = try_from_raw_rejected_status(error)?.unwrap_or(
                    SuiError::GrpcMessageDeserializeError {
                        type_info: "RawSubmitTxResult.inner.Error".to_string(),
                        error: "RawSubmitTxResult.inner.Error is None".to_string(),
                    },
                );
                Ok(SubmitTxResult::Rejected { error })
            }
            None => Err(SuiError::GrpcMessageDeserializeError {
                type_info: "RawSubmitTxResult.inner".to_string(),
                error: "RawSubmitTxResult.inner is None".to_string(),
            }),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SubmitTxResponse {
    pub results: Vec<SubmitTxResult>,
}

impl TryFrom<RawSubmitTxResponse> for SubmitTxResponse {
    type Error = SuiError;

    fn try_from(value: RawSubmitTxResponse) -> Result<Self, Self::Error> {
        // TODO(fastpath): handle multiple transactions.
        if value.results.len() != 1 {
            return Err(SuiError::GrpcMessageDeserializeError {
                type_info: "RawSubmitTxResponse.results".to_string(),
                error: format!("Expected exactly 1 result, got {}", value.results.len()),
            });
        }

        let results = value
            .results
            .into_iter()
            .map(|result| result.try_into())
            .collect::<Result<Vec<SubmitTxResult>, SuiError>>()?;

        Ok(Self { results })
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
    pub transaction_digest: Option<TransactionDigest>,
    /// If consensus position is provided, waits in the server handler for the transaction in it to execute,
    /// either in fastpath outputs or finalized.
    /// If it is not provided, only waits for finalized effects of the transaction in the server handler,
    /// but not for fastpath outputs.
    pub consensus_position: Option<ConsensusPosition>,
    /// Whether to include details of the effects,
    /// including the effects content, events, input objects, and output objects.
    pub include_details: bool,
    /// If this is a ping request, then this is the type of ping.
    pub ping: Option<PingType>,
}

#[derive(PartialEq)]
pub(crate) enum PingType {
    // Testing the time that it takes for the block of the ping transaction to appear as certified via the FastPath.
    FastPath,
    // Testing the time that it takes for the block of the ping transaction to appear as certified via the Consensus.
    // This is useful when want to test the end to end latency from when a block is proposed up to when it comes out of consensus as certified.
    Consensus,
}

impl PingType {
    pub fn as_str(&self) -> &str {
        match self {
            PingType::FastPath => "fastpath",
            PingType::Consensus => "consensus",
        }
    }
}

#[derive(Default, Clone)]
pub struct ExecutedData {
    pub effects: TransactionEffects,
    pub events: Option<TransactionEvents>,
    pub input_objects: Vec<Object>,
    pub output_objects: Vec<Object>,
}

#[derive(Clone)]
pub enum WaitForEffectsResponse {
    Executed {
        effects_digest: TransactionEffectsDigest,
        details: Option<Box<ExecutedData>>,
        fast_path: bool,
    },
    // The transaction was rejected by consensus.
    Rejected {
        // The reason of the reject vote casted by the validator.
        // If None, the validator did not cast a reject vote.
        error: Option<SuiError>,
    },
    // The transaction position is expired, with the local epoch and committed round.
    // When round is None, the expiration is due to lagging epoch in the request.
    Expired {
        epoch: u64,
        round: Option<u32>,
    },
}

impl fmt::Debug for WaitForEffectsResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Executed {
                effects_digest,
                fast_path,
                ..
            } => f
                .debug_struct("Executed")
                .field("effects_digest", effects_digest)
                .field("fast_path", fast_path)
                .finish(),
            Self::Rejected { error } => f.debug_struct("Rejected").field("error", error).finish(),
            Self::Expired { epoch, round } => f
                .debug_struct("Expired")
                .field("epoch", epoch)
                .field("round", round)
                .finish(),
        }
    }
}

impl From<RawPingType> for PingType {
    fn from(value: RawPingType) -> Self {
        match value {
            RawPingType::FastPath => PingType::FastPath,
            RawPingType::Consensus => PingType::Consensus,
        }
    }
}

impl From<PingType> for RawPingType {
    fn from(value: PingType) -> Self {
        match value {
            PingType::FastPath => RawPingType::FastPath,
            PingType::Consensus => RawPingType::Consensus,
        }
    }
}

impl TryFrom<RawWaitForEffectsRequest> for WaitForEffectsRequest {
    type Error = SuiError;

    fn try_from(value: RawWaitForEffectsRequest) -> Result<Self, Self::Error> {
        let transaction_digest = if let Some(transaction_digest) = value.transaction_digest {
            Some(bcs::from_bytes(&transaction_digest).map_err(|err| {
                SuiError::GrpcMessageDeserializeError {
                    type_info: "RawWaitForEffectsRequest.transaction_digest".to_string(),
                    error: err.to_string(),
                }
            })?)
        } else {
            None
        };
        let consensus_position = match value.consensus_position {
            Some(cp) => Some(cp.as_ref().try_into()?),
            None => None,
        };
        let ping = value
            .ping_type
            .map(|p| {
                RawPingType::try_from(p).map(PingType::from).map_err(|e| {
                    SuiError::GrpcMessageDeserializeError {
                        type_info: "RawWaitForEffectsRequest.ping".to_string(),
                        error: e.to_string(),
                    }
                })
            })
            .transpose()?;
        Ok(Self {
            consensus_position,
            transaction_digest,
            include_details: value.include_details,
            ping,
        })
    }
}

impl TryFrom<RawWaitForEffectsResponse> for WaitForEffectsResponse {
    type Error = SuiError;

    fn try_from(value: RawWaitForEffectsResponse) -> Result<Self, Self::Error> {
        match value.inner {
            Some(RawValidatorTransactionStatus::Executed(executed)) => {
                let (effects_digest, details, fast_path) = try_from_raw_executed_status(executed)?;
                Ok(Self::Executed {
                    effects_digest,
                    details,
                    fast_path,
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
) -> Result<(TransactionEffectsDigest, Option<Box<ExecutedData>>, bool), SuiError> {
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
    Ok((effects_digest, details, executed.fast_path))
}

fn try_from_raw_rejected_status(rejected: RawRejectedStatus) -> Result<Option<SuiError>, SuiError> {
    match rejected.error {
        Some(error_bytes) => {
            let error = bcs::from_bytes(&error_bytes).map_err(|err| {
                SuiError::GrpcMessageDeserializeError {
                    type_info: "RawWaitForEffectsResponse.rejected.reason".to_string(),
                    error: err.to_string(),
                }
            })?;
            Ok(Some(error))
        }
        None => Ok(None),
    }
}

fn try_from_response_rejected(error: Option<SuiError>) -> Result<RawRejectedStatus, SuiError> {
    let error = match error {
        Some(e) => Some(
            bcs::to_bytes(&e)
                .map_err(|err| SuiError::GrpcMessageSerializeError {
                    type_info: "RawRejectedStatus.error".to_string(),
                    error: err.to_string(),
                })?
                .into(),
        ),
        None => None,
    };
    Ok(RawRejectedStatus { error })
}

impl TryFrom<WaitForEffectsRequest> for RawWaitForEffectsRequest {
    type Error = SuiError;

    fn try_from(value: WaitForEffectsRequest) -> Result<Self, Self::Error> {
        let transaction_digest = if let Some(transaction_digest) = value.transaction_digest {
            Some(
                bcs::to_bytes(&transaction_digest)
                    .map_err(|err| SuiError::GrpcMessageSerializeError {
                        type_info: "RawWaitForEffectsRequest.transaction_digest".to_string(),
                        error: err.to_string(),
                    })?
                    .into(),
            )
        } else {
            None
        };
        let consensus_position = match value.consensus_position {
            Some(cp) => Some(cp.into_raw()?),
            None => None,
        };
        let ping_type = value.ping.map(|p| {
            let raw: RawPingType = p.into();
            raw.into()
        });
        Ok(Self {
            consensus_position,
            transaction_digest,
            include_details: value.include_details,
            ping_type,
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
                fast_path,
            } => {
                let raw_executed = try_from_response_executed(effects_digest, details, fast_path)?;
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
    fast_path: bool,
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
        fast_path,
    })
}
