// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{ObjectID, SequenceNumber, TransactionDigest};
use crate::crypto::{AuthoritySignInfo, AuthorityStrongQuorumSignInfo};
use crate::effects::{
    SignedTransactionEffects, TransactionEvents, VerifiedSignedTransactionEffects,
};
use crate::error::SuiError;
use crate::object::Object;
use crate::transaction::{CertifiedTransaction, SenderSignedData, SignedTransaction, Transaction};

use bytes::Bytes;
use move_core_types::annotated_value::MoveStructLayout;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum ObjectInfoRequestKind {
    /// Request the latest object state.
    LatestObjectInfo,
    /// Request a specific version of the object.
    /// This is used only for debugging purpose and will not work as a generic solution
    /// since we don't keep around all historic object versions.
    /// No production code should depend on this kind.
    PastObjectInfoDebug(SequenceNumber),
}

/// Layout generation options -- you can either generate or not generate the layout.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum LayoutGenerationOption {
    Generate,
    None,
}

/// A request for information about an object and optionally its
/// parent certificate at a specific version.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct ObjectInfoRequest {
    /// The id of the object to retrieve, at the latest version.
    pub object_id: ObjectID,
    /// if true return the layout of the object.
    pub generate_layout: LayoutGenerationOption,
    /// The type of request, either latest object info or the past.
    pub request_kind: ObjectInfoRequestKind,
}

impl ObjectInfoRequest {
    pub fn past_object_info_debug_request(
        object_id: ObjectID,
        version: SequenceNumber,
        generate_layout: LayoutGenerationOption,
    ) -> Self {
        ObjectInfoRequest {
            object_id,
            generate_layout,
            request_kind: ObjectInfoRequestKind::PastObjectInfoDebug(version),
        }
    }

    pub fn latest_object_info_request(
        object_id: ObjectID,
        generate_layout: LayoutGenerationOption,
    ) -> Self {
        ObjectInfoRequest {
            object_id,
            generate_layout,
            request_kind: ObjectInfoRequestKind::LatestObjectInfo,
        }
    }
}

/// This message provides information about the latest object and its lock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectInfoResponse {
    /// Value of the requested object in this authority
    pub object: Object,
    /// Schema of the Move value inside this object.
    /// None if the object is a Move package, or the request did not ask for the layout
    pub layout: Option<MoveStructLayout>,
    /// Transaction the object is locked on in this authority.
    /// None if the object is not currently locked by this authority.
    /// This should be only used for debugging purpose, such as from sui-tool. No prod clients should
    /// rely on it.
    pub lock_for_debugging: Option<SignedTransaction>,
}

/// Verified version of `ObjectInfoResponse`. `layout` and `lock_for_debugging` are skipped because they
/// are not needed and we don't want to verify them.
#[derive(Debug, Clone)]
pub struct VerifiedObjectInfoResponse {
    /// Value of the requested object in this authority
    pub object: Object,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionInfoRequest {
    pub transaction_digest: TransactionDigest,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TransactionStatus {
    /// Signature over the transaction.
    Signed(AuthoritySignInfo),
    /// For executed transaction, we could return an optional certificate signature on the transaction
    /// (i.e. the signature part of the CertifiedTransaction), as well as the signed effects.
    /// The certificate signature is optional because for transactions executed in previous
    /// epochs, we won't keep around the certificate signatures.
    Executed(
        Option<AuthorityStrongQuorumSignInfo>,
        SignedTransactionEffects,
        TransactionEvents,
    ),
}

impl TransactionStatus {
    pub fn into_signed_for_testing(self) -> AuthoritySignInfo {
        match self {
            Self::Signed(s) => s,
            _ => unreachable!("Incorrect response type"),
        }
    }

    pub fn into_effects_for_testing(self) -> SignedTransactionEffects {
        match self {
            Self::Executed(_, e, _) => e,
            _ => unreachable!("Incorrect response type"),
        }
    }
}

impl PartialEq for TransactionStatus {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Self::Signed(s1) => match other {
                Self::Signed(s2) => s1.epoch == s2.epoch,
                _ => false,
            },
            Self::Executed(c1, e1, ev1) => match other {
                Self::Executed(c2, e2, ev2) => {
                    c1.as_ref().map(|a| a.epoch) == c2.as_ref().map(|a| a.epoch)
                        && e1.epoch() == e2.epoch()
                        && e1.digest() == e2.digest()
                        && ev1.digest() == ev2.digest()
                }
                _ => false,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HandleTransactionResponse {
    pub status: TransactionStatus,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransactionInfoResponse {
    pub transaction: SenderSignedData,
    pub status: TransactionStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandleCertificateResponseV2 {
    pub signed_effects: SignedTransactionEffects,
    pub events: TransactionEvents,
    /// Not used. Full node local execution fast path was deprecated.
    pub fastpath_input_objects: Vec<Object>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubmitCertificateResponse {
    /// If transaction is already executed, return same result as handle_certificate
    pub executed: Option<HandleCertificateResponseV2>,
}

#[derive(Clone, Debug)]
pub struct VerifiedHandleCertificateResponse {
    pub signed_effects: VerifiedSignedTransactionEffects,
    pub events: TransactionEvents,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SystemStateRequest {
    // This is needed to make gRPC happy.
    pub _unused: bool,
}

/// Response type for version 3 of the handle certificate validator API.
///
/// The corresponding version 3 request type allows for a client to request events as well as
/// input/output objects from a transaction's execution. Given Validators operate with very
/// aggressive object pruning, the return of input/output objects is only done immediately after
/// the transaction has been executed locally on the validator and will not be returned for
/// requests to previously executed transactions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandleCertificateResponseV3 {
    pub effects: SignedTransactionEffects,
    pub events: Option<TransactionEvents>,

    /// If requested, will included all initial versions of objects modified in this transaction.
    /// This includes owned objects included as input into the transaction as well as the assigned
    /// versions of shared objects.
    //
    // TODO: In the future we may want to include shared objects or child objects which were read
    // but not modified during execution.
    pub input_objects: Option<Vec<Object>>,

    /// If requested, will included all changed objects, including mutated, created and unwrapped
    /// objects. In other words, all objects that still exist in the object state after this
    /// transaction.
    pub output_objects: Option<Vec<Object>>,
    pub auxiliary_data: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandleCertificateRequestV3 {
    pub certificate: CertifiedTransaction,

    pub include_events: bool,
    pub include_input_objects: bool,
    pub include_output_objects: bool,
    pub include_auxiliary_data: bool,
}

impl From<HandleCertificateResponseV3> for HandleCertificateResponseV2 {
    fn from(value: HandleCertificateResponseV3) -> Self {
        Self {
            signed_effects: value.effects,
            events: value.events.unwrap_or_default(),
            fastpath_input_objects: Vec::new(),
        }
    }
}

/// Response type for the handle Soft Bundle certificates validator API.
/// If `wait_for_effects` is true, it is guaranteed that:
///  - Number of responses will be equal to the number of input transactions.
///  - The order of the responses matches the order of the input transactions.
///
/// Otherwise, `responses` will be empty.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandleSoftBundleCertificatesResponseV3 {
    pub responses: Vec<HandleCertificateResponseV3>,
}

/// Soft Bundle request.  See [SIP-19](https://github.com/sui-foundation/sips/blob/main/sips/sip-19.md).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandleSoftBundleCertificatesRequestV3 {
    pub certificates: Vec<CertifiedTransaction>,

    pub wait_for_effects: bool,
    pub include_events: bool,
    pub include_input_objects: bool,
    pub include_output_objects: bool,
    pub include_auxiliary_data: bool,
}

// =========== ExecutedData ===========

#[derive(Default, Clone)]
pub struct ExecutedData {
    pub effects: crate::effects::TransactionEffects,
    pub events: Option<crate::effects::TransactionEvents>,
    pub input_objects: Vec<crate::object::Object>,
    pub output_objects: Vec<crate::object::Object>,
}

#[derive(Clone, prost::Message)]
pub struct RawExecutedData {
    #[prost(bytes = "bytes", tag = "1")]
    pub effects: Bytes,
    #[prost(bytes = "bytes", optional, tag = "2")]
    pub events: Option<Bytes>,
    #[prost(bytes = "bytes", repeated, tag = "3")]
    pub input_objects: Vec<Bytes>,
    #[prost(bytes = "bytes", repeated, tag = "4")]
    pub output_objects: Vec<Bytes>,
}

// =========== SubmitTx types ===========

#[derive(Clone, Debug)]
pub struct SubmitTxRequest {
    pub transaction: Transaction,
}

impl SubmitTxRequest {
    pub fn new_transaction(transaction: Transaction) -> Self {
        Self { transaction }
    }
}

impl SubmitTxRequest {
    pub fn into_raw(&self) -> Result<RawSubmitTxRequest, SuiError> {
        let transactions = vec![bcs::to_bytes(&self.transaction)
            .map_err(|e| SuiError::TransactionSerializationError {
                error: e.to_string(),
            })?
            .into()];
        Ok(RawSubmitTxRequest {
            transactions,
            ..Default::default()
        })
    }
}

#[derive(Clone)]
pub enum SubmitTxResult {
    Submitted {
        consensus_position: crate::messages_consensus::ConsensusPosition,
    },
    Executed {
        effects_digest: crate::digests::TransactionEffectsDigest,
        // Response should always include details for executed transactions.
        // TODO(fastpath): validate this field is always present and return an error during deserialization.
        details: Option<Box<ExecutedData>>,
        // Whether the transaction was executed using fast path.
        fast_path: bool,
    },
    Rejected {
        error: crate::error::SuiError,
    },
}

impl std::fmt::Debug for SubmitTxResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

#[derive(Clone, Debug)]
pub struct SubmitTxResponse {
    pub results: Vec<SubmitTxResult>,
}

#[derive(Clone, prost::Message)]
pub struct RawSubmitTxRequest {
    /// The transactions to be submitted. When the vector is empty, then this is treated as a ping request.
    #[prost(bytes = "bytes", repeated, tag = "1")]
    pub transactions: Vec<Bytes>,

    /// When submitting multiple transactions, attempt to include them in
    /// the same block with the same order (soft bundle), if true.
    /// Otherwise, allow the transactions to be included separately and
    /// out of order in blocks (batch).
    #[prost(bool, tag = "2")]
    pub soft_bundle: bool,
}

#[derive(Clone, prost::Message)]
pub struct RawSubmitTxResponse {
    // Results corresponding to each transaction in the request.
    #[prost(message, repeated, tag = "1")]
    pub results: Vec<RawSubmitTxResult>,
}

#[derive(Clone, prost::Message)]
pub struct RawSubmitTxResult {
    #[prost(oneof = "RawValidatorSubmitStatus", tags = "1, 2, 3")]
    pub inner: Option<RawValidatorSubmitStatus>,
}

#[derive(Clone, prost::Oneof)]
pub enum RawValidatorSubmitStatus {
    // Serialized Consensus Position.
    #[prost(bytes = "bytes", tag = "1")]
    Submitted(Bytes),

    // Transaction has already been executed (finalized).
    #[prost(message, tag = "2")]
    Executed(RawExecutedStatus),

    // Transaction is rejected from consensus submission.
    #[prost(message, tag = "3")]
    Rejected(RawRejectedStatus),
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum RawPingType {
    FastPath = 0,
    Consensus = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PingType {
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

// =========== WaitForEffects types ===========

pub struct WaitForEffectsRequest {
    pub transaction_digest: Option<crate::digests::TransactionDigest>,
    /// If consensus position is provided, waits in the server handler for the transaction in it to execute,
    /// either in fastpath outputs or finalized.
    /// If it is not provided, only waits for finalized effects of the transaction in the server handler,
    /// but not for fastpath outputs.
    pub consensus_position: Option<crate::messages_consensus::ConsensusPosition>,
    /// Whether to include details of the effects,
    /// including the effects content, events, input objects, and output objects.
    pub include_details: bool,
    /// If this is a ping request, then this is the type of ping.
    pub ping: Option<PingType>,
}

#[derive(Clone)]
pub enum WaitForEffectsResponse {
    Executed {
        effects_digest: crate::digests::TransactionEffectsDigest,
        details: Option<Box<ExecutedData>>,
        fast_path: bool,
    },
    // The transaction was rejected by consensus.
    Rejected {
        // The reason of the reject vote casted by the validator.
        // If None, the validator did not cast a reject vote.
        error: Option<crate::error::SuiError>,
    },
    // The transaction position is expired, with the local epoch and committed round.
    // When round is None, the expiration is due to lagging epoch in the request.
    Expired {
        epoch: u64,
        round: Option<u32>,
    },
}

impl std::fmt::Debug for WaitForEffectsResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

#[derive(Clone, prost::Message)]
pub struct RawWaitForEffectsRequest {
    /// The transaction's digest. If it's a ping request, then this will practically be ignored.
    #[prost(bytes = "bytes", optional, tag = "1")]
    pub transaction_digest: Option<Bytes>,

    /// If provided, wait for the consensus position to execute and wait for fastpath outputs of the transaction,
    /// in addition to waiting for finalized effects.
    /// If not provided, only wait for finalized effects.
    #[prost(bytes = "bytes", optional, tag = "2")]
    pub consensus_position: Option<Bytes>,

    /// Whether to include details of the effects,
    /// including the effects content, events, input objects, and output objects.
    #[prost(bool, tag = "3")]
    pub include_details: bool,

    /// Set when this is a ping request, to differentiate between fastpath and consensus pings.
    #[prost(enumeration = "RawPingType", optional, tag = "4")]
    pub ping_type: Option<i32>,
}

#[derive(Clone, prost::Message)]
pub struct RawWaitForEffectsResponse {
    // In order to represent an enum in protobuf, we need to use oneof.
    // However, oneof also allows the value to be unset, which corresponds to None value.
    // Hence, we need to use Option type for `inner`.
    // We expect the value to be set in a valid response.
    #[prost(oneof = "RawValidatorTransactionStatus", tags = "1, 2, 3")]
    pub inner: Option<RawValidatorTransactionStatus>,
}

#[derive(Clone, prost::Oneof)]
pub enum RawValidatorTransactionStatus {
    #[prost(message, tag = "1")]
    Executed(RawExecutedStatus),
    #[prost(message, tag = "2")]
    Rejected(RawRejectedStatus),
    #[prost(message, tag = "3")]
    Expired(RawExpiredStatus),
}

#[derive(Clone, prost::Message)]
pub struct RawExecutedStatus {
    #[prost(bytes = "bytes", tag = "1")]
    pub effects_digest: Bytes,
    #[prost(message, optional, tag = "2")]
    pub details: Option<RawExecutedData>,
    #[prost(bool, tag = "3")]
    pub fast_path: bool,
}

#[derive(Clone, prost::Message)]
pub struct RawRejectedStatus {
    #[prost(bytes = "bytes", optional, tag = "1")]
    pub error: Option<Bytes>,
}

#[derive(Clone, prost::Message)]
pub struct RawExpiredStatus {
    // Validator's current epoch.
    #[prost(uint64, tag = "1")]
    pub epoch: u64,
    // Validator's current round. 0 if it is not yet checked.
    #[prost(uint32, optional, tag = "2")]
    pub round: Option<u32>,
}

// =========== ValidatorHealth types ===========

/// Request for validator health information (used for latency measurement)
#[derive(Clone, Debug, Default)]
pub struct ValidatorHealthRequest {}

/// Response with validator health metrics (data collected but not used for scoring yet)
#[derive(Clone, Debug, Default)]
pub struct ValidatorHealthResponse {
    /// Number of in-flight execution transactions from execution scheduler
    pub num_inflight_execution_transactions: u64,
    /// Number of in-flight consensus transactions
    pub num_inflight_consensus_transactions: u64,
    /// Last committed leader round from Mysticeti consensus
    pub last_committed_leader_round: u32,
    /// Last locally built checkpoint sequence number
    pub last_locally_built_checkpoint: u64,
}

/// Raw protobuf request for validator health information (evolvable)
#[derive(Clone, prost::Message)]
pub struct RawValidatorHealthRequest {}

/// Raw protobuf response with validator health metrics (evolvable)
#[derive(Clone, prost::Message)]
pub struct RawValidatorHealthResponse {
    /// Number of pending certificates
    #[prost(uint64, optional, tag = "1")]
    pub pending_certificates: Option<u64>,
    /// Number of in-flight consensus messages
    #[prost(uint64, optional, tag = "2")]
    pub inflight_consensus_messages: Option<u64>,
    /// Current consensus round
    #[prost(uint64, optional, tag = "3")]
    pub consensus_round: Option<u64>,
    /// Current checkpoint sequence number
    #[prost(uint64, optional, tag = "4")]
    pub checkpoint_sequence: Option<u64>,
}

// =========== Parse helpers ===========

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

impl TryFrom<ExecutedData> for RawExecutedData {
    type Error = crate::error::SuiError;

    fn try_from(value: ExecutedData) -> Result<Self, Self::Error> {
        let effects = bcs::to_bytes(&value.effects)
            .map_err(|err| crate::error::SuiError::GrpcMessageSerializeError {
                type_info: "ExecutedData.effects".to_string(),
                error: err.to_string(),
            })?
            .into();
        let events = if let Some(events) = &value.events {
            Some(
                bcs::to_bytes(events)
                    .map_err(|err| crate::error::SuiError::GrpcMessageSerializeError {
                        type_info: "ExecutedData.events".to_string(),
                        error: err.to_string(),
                    })?
                    .into(),
            )
        } else {
            None
        };
        let mut input_objects = Vec::with_capacity(value.input_objects.len());
        for object in value.input_objects {
            input_objects.push(
                bcs::to_bytes(&object)
                    .map_err(|err| crate::error::SuiError::GrpcMessageSerializeError {
                        type_info: "ExecutedData.input_objects".to_string(),
                        error: err.to_string(),
                    })?
                    .into(),
            );
        }
        let mut output_objects = Vec::with_capacity(value.output_objects.len());
        for object in value.output_objects {
            output_objects.push(
                bcs::to_bytes(&object)
                    .map_err(|err| crate::error::SuiError::GrpcMessageSerializeError {
                        type_info: "ExecutedData.output_objects".to_string(),
                        error: err.to_string(),
                    })?
                    .into(),
            );
        }
        Ok(RawExecutedData {
            effects,
            events,
            input_objects,
            output_objects,
        })
    }
}

impl TryFrom<RawExecutedData> for ExecutedData {
    type Error = crate::error::SuiError;

    fn try_from(value: RawExecutedData) -> Result<Self, Self::Error> {
        let effects = bcs::from_bytes(&value.effects).map_err(|err| {
            crate::error::SuiError::GrpcMessageDeserializeError {
                type_info: "RawExecutedData.effects".to_string(),
                error: err.to_string(),
            }
        })?;
        let events = if let Some(events) = value.events {
            Some(bcs::from_bytes(&events).map_err(|err| {
                crate::error::SuiError::GrpcMessageDeserializeError {
                    type_info: "RawExecutedData.events".to_string(),
                    error: err.to_string(),
                }
            })?)
        } else {
            None
        };
        let mut input_objects = Vec::with_capacity(value.input_objects.len());
        for object in value.input_objects {
            input_objects.push(bcs::from_bytes(&object).map_err(|err| {
                crate::error::SuiError::GrpcMessageDeserializeError {
                    type_info: "RawExecutedData.input_objects".to_string(),
                    error: err.to_string(),
                }
            })?);
        }
        let mut output_objects = Vec::with_capacity(value.output_objects.len());
        for object in value.output_objects {
            output_objects.push(bcs::from_bytes(&object).map_err(|err| {
                crate::error::SuiError::GrpcMessageDeserializeError {
                    type_info: "RawExecutedData.output_objects".to_string(),
                    error: err.to_string(),
                }
            })?);
        }
        Ok(ExecutedData {
            effects,
            events,
            input_objects,
            output_objects,
        })
    }
}

impl TryFrom<SubmitTxResult> for RawSubmitTxResult {
    type Error = crate::error::SuiError;

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
    type Error = crate::error::SuiError;

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
                    crate::error::SuiError::GrpcMessageDeserializeError {
                        type_info: "RawSubmitTxResult.inner.Error".to_string(),
                        error: "RawSubmitTxResult.inner.Error is None".to_string(),
                    },
                );
                Ok(SubmitTxResult::Rejected { error })
            }
            None => Err(crate::error::SuiError::GrpcMessageDeserializeError {
                type_info: "RawSubmitTxResult.inner".to_string(),
                error: "RawSubmitTxResult.inner is None".to_string(),
            }),
        }
    }
}

impl TryFrom<RawSubmitTxResponse> for SubmitTxResponse {
    type Error = crate::error::SuiError;

    fn try_from(value: RawSubmitTxResponse) -> Result<Self, Self::Error> {
        // TODO(fastpath): handle multiple transactions.
        if value.results.len() != 1 {
            return Err(crate::error::SuiError::GrpcMessageDeserializeError {
                type_info: "RawSubmitTxResponse.results".to_string(),
                error: format!("Expected exactly 1 result, got {}", value.results.len()),
            });
        }

        let results = value
            .results
            .into_iter()
            .map(|result| result.try_into())
            .collect::<Result<Vec<SubmitTxResult>, crate::error::SuiError>>()?;

        Ok(Self { results })
    }
}

fn try_from_raw_executed_status(
    executed: RawExecutedStatus,
) -> Result<
    (
        crate::digests::TransactionEffectsDigest,
        Option<Box<ExecutedData>>,
        bool,
    ),
    crate::error::SuiError,
> {
    let effects_digest = bcs::from_bytes(&executed.effects_digest).map_err(|err| {
        crate::error::SuiError::GrpcMessageDeserializeError {
            type_info: "RawWaitForEffectsResponse.effects_digest".to_string(),
            error: err.to_string(),
        }
    })?;
    let executed_data = if let Some(details) = executed.details {
        Some(Box::new(details.try_into()?))
    } else {
        None
    };
    Ok((effects_digest, executed_data, executed.fast_path))
}

fn try_from_raw_rejected_status(
    rejected: RawRejectedStatus,
) -> Result<Option<crate::error::SuiError>, crate::error::SuiError> {
    match rejected.error {
        Some(error_bytes) => {
            let error = bcs::from_bytes(&error_bytes).map_err(|err| {
                crate::error::SuiError::GrpcMessageDeserializeError {
                    type_info: "RawWaitForEffectsResponse.rejected.reason".to_string(),
                    error: err.to_string(),
                }
            })?;
            Ok(Some(error))
        }
        None => Ok(None),
    }
}

fn try_from_response_rejected(
    error: Option<crate::error::SuiError>,
) -> Result<RawRejectedStatus, crate::error::SuiError> {
    let error = match error {
        Some(e) => Some(
            bcs::to_bytes(&e)
                .map_err(|err| crate::error::SuiError::GrpcMessageSerializeError {
                    type_info: "RawRejectedStatus.error".to_string(),
                    error: err.to_string(),
                })?
                .into(),
        ),
        None => None,
    };
    Ok(RawRejectedStatus { error })
}

fn try_from_response_executed(
    effects_digest: crate::digests::TransactionEffectsDigest,
    details: Option<Box<ExecutedData>>,
    fast_path: bool,
) -> Result<RawExecutedStatus, crate::error::SuiError> {
    let effects_digest = bcs::to_bytes(&effects_digest)
        .map_err(|err| crate::error::SuiError::GrpcMessageSerializeError {
            type_info: "RawWaitForEffectsResponse.effects_digest".to_string(),
            error: err.to_string(),
        })?
        .into();
    let details = if let Some(details) = details {
        Some((*details).try_into()?)
    } else {
        None
    };
    Ok(RawExecutedStatus {
        effects_digest,
        details,
        fast_path,
    })
}

impl TryFrom<RawWaitForEffectsRequest> for WaitForEffectsRequest {
    type Error = crate::error::SuiError;

    fn try_from(value: RawWaitForEffectsRequest) -> Result<Self, Self::Error> {
        let transaction_digest = match value.transaction_digest {
            Some(digest) => Some(bcs::from_bytes(&digest).map_err(|err| {
                crate::error::SuiError::GrpcMessageDeserializeError {
                    type_info: "RawWaitForEffectsRequest.transaction_digest".to_string(),
                    error: err.to_string(),
                }
            })?),
            None => None,
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

impl TryFrom<WaitForEffectsRequest> for RawWaitForEffectsRequest {
    type Error = crate::error::SuiError;

    fn try_from(value: WaitForEffectsRequest) -> Result<Self, Self::Error> {
        let transaction_digest = match value.transaction_digest {
            Some(digest) => Some(
                bcs::to_bytes(&digest)
                    .map_err(|err| crate::error::SuiError::GrpcMessageSerializeError {
                        type_info: "RawWaitForEffectsRequest.transaction_digest".to_string(),
                        error: err.to_string(),
                    })?
                    .into(),
            ),
            None => None,
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

impl TryFrom<RawWaitForEffectsResponse> for WaitForEffectsResponse {
    type Error = crate::error::SuiError;

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
            None => Err(crate::error::SuiError::GrpcMessageDeserializeError {
                type_info: "RawWaitForEffectsResponse.inner".to_string(),
                error: "RawWaitForEffectsResponse.inner is None".to_string(),
            }),
        }
    }
}

impl TryFrom<WaitForEffectsResponse> for RawWaitForEffectsResponse {
    type Error = crate::error::SuiError;

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

impl TryFrom<ValidatorHealthRequest> for RawValidatorHealthRequest {
    type Error = crate::error::SuiError;

    fn try_from(_value: ValidatorHealthRequest) -> Result<Self, Self::Error> {
        Ok(Self {})
    }
}

impl TryFrom<RawValidatorHealthRequest> for ValidatorHealthRequest {
    type Error = crate::error::SuiError;

    fn try_from(_value: RawValidatorHealthRequest) -> Result<Self, Self::Error> {
        // Empty request - ignore reserved field for now
        Ok(Self {})
    }
}

impl TryFrom<ValidatorHealthResponse> for RawValidatorHealthResponse {
    type Error = crate::error::SuiError;

    fn try_from(value: ValidatorHealthResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            pending_certificates: Some(value.num_inflight_execution_transactions),
            inflight_consensus_messages: Some(value.num_inflight_consensus_transactions),
            consensus_round: Some(value.last_committed_leader_round as u64),
            checkpoint_sequence: Some(value.last_locally_built_checkpoint),
        })
    }
}

impl TryFrom<RawValidatorHealthResponse> for ValidatorHealthResponse {
    type Error = crate::error::SuiError;

    fn try_from(value: RawValidatorHealthResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            num_inflight_consensus_transactions: value.inflight_consensus_messages.unwrap_or(0),
            num_inflight_execution_transactions: value.pending_certificates.unwrap_or(0),
            last_locally_built_checkpoint: value.checkpoint_sequence.unwrap_or(0),
            last_committed_leader_round: value.consensus_round.unwrap_or(0) as u32,
        })
    }
}
