// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::extract::rejection::JsonRejection;
use std::fmt::Debug;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use fastcrypto::error::FastCryptoError;
use serde::Serialize;
use serde::Serializer;
use serde_json::{Value, json};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use prost::Message;
use sui_rpc::proto::google::rpc::{ErrorInfo, Status as RpcStatus};
use sui_rpc::proto::sui::rpc::v2::{ErrorReason, ExecutionError};
use sui_types::error::{SuiError, SuiErrorKind};

use crate::types::{BlockHash, OperationType, PublicKey, SuiEnv};
use strum::EnumProperty;
use strum_macros::Display;
use strum_macros::EnumDiscriminants;
use thiserror::Error;

/// Sui-Rosetta specific error types.
/// This contains all the errors returns by the sui-rosetta server.
#[derive(Debug, Error, EnumDiscriminants, EnumProperty)]
#[strum_discriminants(
    name(ErrorType),
    derive(Display, EnumIter),
    strum(serialize_all = "kebab-case")
)]
#[allow(clippy::enum_variant_names)]
pub enum Error {
    #[error("Unsupported blockchain: {0}")]
    UnsupportedBlockchain(String),
    #[error("Unsupported network: {0:?}")]
    UnsupportedNetwork(SuiEnv),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Missing input: {0}")]
    MissingInput(String),
    #[error("Missing metadata")]
    MissingMetadata,
    #[error("{0}")]
    MalformedOperationError(String),
    #[error("Unsupported operation: {0:?}")]
    UnsupportedOperation(OperationType),
    #[error("Data error: {0}")]
    DataError(String),
    #[error("Block not found, index: {index:?}, hash: {hash:?}")]
    BlockNotFound {
        index: Option<u64>,
        hash: Option<BlockHash>,
    },
    #[error("Public key deserialization error: {0:?}")]
    PublicKeyDeserializationError(PublicKey),

    #[error("Error executing transaction: {0}")]
    TransactionExecutionError(Box<ExecutionError>),

    #[error("{0}")]
    TransactionDryRunError(Box<ExecutionError>),

    #[error(transparent)]
    InternalError(#[from] anyhow::Error),
    #[error(transparent)]
    BCSSerializationError(#[from] bcs::Error),
    #[error(transparent)]
    CryptoError(#[from] FastCryptoError),
    #[error(transparent)]
    SuiError(#[from] SuiError),
    #[error(transparent)]
    SuiRpcError(#[from] tonic::Status),
    #[error(transparent)]
    EncodingError(#[from] eyre::Report),
    #[error(transparent)]
    JsonExtractorRejection(#[from] JsonRejection),

    #[error("Retries exhausted while getting balance. try again.")]
    #[strum(props(retriable = "true"))]
    RetryExhausted(String),

    // Appended last so existing `ErrorType` discriminant positions — and thus the
    // Rosetta wire `code` integers — stay stable.
    #[error("Coin metadata temporarily unavailable: {0}")]
    #[strum(props(retriable = "true"))]
    CoinMetadataUnavailable(String),
}

impl From<SuiErrorKind> for Error {
    fn from(e: SuiErrorKind) -> Self {
        Error::SuiError(SuiError::from(e))
    }
}

/// The `google.rpc.Status` the fullnode encodes into the details of an `InvalidArgument` status,
/// together with its `ErrorInfo` detail of the given `reason` in domain `ErrorReason::DOMAIN`.
/// `None` for any other code, for details that do not decode, or when no such `ErrorInfo` exists.
fn invalid_argument_error_info(
    status: &tonic::Status,
    reason: ErrorReason,
) -> Option<(RpcStatus, ErrorInfo)> {
    if status.code() != tonic::Code::InvalidArgument {
        return None;
    }
    let rpc_status = RpcStatus::decode(status.details()).ok()?;
    let info = rpc_status
        .details
        .iter()
        .filter_map(|detail| detail.to_msg::<ErrorInfo>().ok())
        .find(|info| info.reason == reason.as_str_name() && info.domain == ErrorReason::DOMAIN)?;
    Some((rpc_status, info))
}

/// The `ErrorInfo` detail (reason `INSUFFICIENT_GAS_BALANCE`, domain `ErrorReason::DOMAIN`) the
/// fullnode attaches to an `InvalidArgument` status when gas selection fails because the gas owner
/// lacks enough SUI for the budget. `None` for any other error, including statuses whose details do
/// not decode.
pub(crate) fn insufficient_gas_balance(error: &Error) -> Option<ErrorInfo> {
    let Error::SuiRpcError(status) = error else {
        return None;
    };
    invalid_argument_error_info(status, ErrorReason::InsufficientGasBalance).map(|(_, info)| info)
}

/// The `requiredBudget` metadata entry, in MIST, of the `ErrorInfo` that
/// [`insufficient_gas_balance`] returns. `None` for any other error, or when the entry is missing
/// or does not parse.
pub(crate) fn insufficient_gas_required_budget(error: &Error) -> Option<u64> {
    insufficient_gas_balance(error)?
        .metadata
        .get("requiredBudget")?
        .parse()
        .ok()
}

/// The `ExecutionError` detail the fullnode attaches, next to an `ErrorInfo` with reason
/// `TRANSACTION_EXECUTION_FAILED` in domain `ErrorReason::DOMAIN`, to an `InvalidArgument` status
/// when a simulated transaction fails in execution and gas selection cannot cover its budget. It is
/// the same error a simulation that can pay for gas reports in its effects. `None` for any other
/// status, including statuses whose details do not decode.
pub(crate) fn execution_failure(status: &tonic::Status) -> Option<ExecutionError> {
    let (rpc_status, _) =
        invalid_argument_error_info(status, ErrorReason::TransactionExecutionFailed)?;
    rpc_status
        .details
        .iter()
        .find_map(|detail| detail.to_msg::<ExecutionError>().ok())
}

impl Serialize for ErrorType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.json().serialize(serializer)
    }
}

trait CustomProperties {
    fn retriable(&self) -> bool;
}

impl CustomProperties for Error {
    fn retriable(&self) -> bool {
        matches!(self.get_str("retriable"), Some("true"))
    }
}

impl ErrorType {
    fn json(&self) -> Value {
        let retriable = false;
        // Safe to unwrap
        let error_code = ErrorType::iter().position(|e| &e == self).unwrap();
        let message = format!("{self}").replace('-', " ");
        let message = message[0..1].to_uppercase() + &message[1..];

        json![{
            "code": error_code,
            "message": message,
            "retriable": retriable,
        }]
    }
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let type_: ErrorType = self.into();
        let mut json = type_.json();
        // Safe to unwrap, we know ErrorType must be an object.
        let error = json.as_object_mut().unwrap();
        let mut details = serde_json::Map::new();
        details.insert("error".into(), json!(format!("{self}")));
        // When gas selection fails for lack of SUI, include the amounts the fullnode reported so
        // callers can prefund the account without parsing the error message.
        if let Some(info) = insufficient_gas_balance(self) {
            for (metadata_key, details_key) in [
                ("requiredBudget", "required_budget"),
                ("coinBalance", "coin_balance"),
                ("addressBalance", "address_balance"),
            ] {
                if let Some(value) = info.metadata.get(metadata_key) {
                    details.insert(details_key.into(), json!(value));
                }
            }
        }
        error.insert("details".into(), Value::Object(details));
        error.insert("retriable".into(), json!(self.retriable()));
        error.serialize(serializer)
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}
