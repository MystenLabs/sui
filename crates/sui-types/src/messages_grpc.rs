// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{ObjectID, SequenceNumber, TransactionDigest};
use crate::crypto::{AuthoritySignInfo, AuthorityStrongQuorumSignInfo};
use crate::effects::{
    SignedTransactionEffects, TransactionEffects, TransactionEvents,
    VerifiedSignedTransactionEffects,
};
use crate::object::Object;
use crate::transaction::{CertifiedTransaction, SenderSignedData, SignedTransaction, Transaction};
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

/// Response type for version 3 of the handle certifacte validator API.
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandleTransactionRequestV2 {
    pub transaction: Transaction,

    pub include_events: bool,
    pub include_input_objects: bool,
    pub include_output_objects: bool,
    pub include_auxiliary_data: bool,
}

/// Response type for version 2 of the handle transaction validator API.
///
/// The corresponding version 2 request type allows for a client to request events as well as
/// input/output objects from a transaction's execution. Given Validators operate with very
/// aggressive object pruning, the return of input/output objects is only done immediately after
/// the transaction has been executed locally on the validator and will not be returned for
/// requests to previously executed transactions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandleTransactionResponseV2 {
    pub effects: TransactionEffects,
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
