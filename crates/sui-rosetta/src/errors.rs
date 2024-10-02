// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::extract::rejection::JsonRejection;
use std::fmt::Debug;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use fastcrypto::error::FastCryptoError;
use serde::Serialize;
use serde::Serializer;
use serde_json::{json, Value};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use sui_types::error::SuiError;

use crate::types::{BlockHash, OperationType, PublicKey, SuiEnv};
use strum::EnumProperty;
use strum_macros::Display;
use strum_macros::EnumDiscriminants;
use thiserror::Error;
use typed_store::TypedStoreError;

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
    TransactionExecutionError(String),

    #[error("{0}")]
    TransactionDryRunError(String),

    #[error(transparent)]
    InternalError(#[from] anyhow::Error),
    #[error(transparent)]
    BCSSerializationError(#[from] bcs::Error),
    #[error(transparent)]
    CryptoError(#[from] FastCryptoError),
    #[error(transparent)]
    SuiError(#[from] SuiError),
    #[error(transparent)]
    SuiRpcError(#[from] sui_sdk::error::Error),
    #[error(transparent)]
    EncodingError(#[from] eyre::Report),
    #[error(transparent)]
    DBError(#[from] TypedStoreError),
    #[error(transparent)]
    JsonExtractorRejection(#[from] JsonRejection),

    #[error("Retries exhausted while getting balance. try again.")]
    #[strum(props(retriable = "true"))]
    RetryExhausted(String),
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
        error.insert("details".into(), json!({ "error": format!("{self}") }));
        error.insert("retriable".into(), json!(self.retriable()));
        error.serialize(serializer)
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}
