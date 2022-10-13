// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{Debug, Display, Formatter};
use std::num::TryFromIntError;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde::{Deserialize, Serializer};
use serde_json::{json, Value};
use signature::Error as SignatureError;
use strum_macros::EnumIter;

use sui_types::base_types::{ObjectID, ObjectIDParseError};
use sui_types::error::SuiError;

use crate::types::OperationType;

/// Sui-Rosetta specific error types.
/// This contains all the errors returns by the sui-rosetta server.
#[derive(Eq, PartialEq, Copy, Clone, Debug, Serialize, Deserialize, EnumIter)]
#[serde(rename_all = "lowercase")]
pub enum ErrorType {
    UnsupportedBlockchain = 1,
    UnsupportedNetwork,
    InvalidInput,
    MissingInput,
    MissingMetadata,
    InternalError,
    DataError,
    UnsupportedOperation,
    ParsingError,
    IncorrectSignerAddress,
    SignatureError,
    SerializationError,
    UnimplementedTransactionType,
    BlockNotFound,
    MalformedOperationError,
    BalanceNotFound,
}

#[derive(Debug)]
pub struct Error {
    type_: ErrorType,
    detail: Option<Value>,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(detail) = &self.detail {
            write!(f, "{:?} : {}", self.type_, detail)
        } else {
            write!(f, "{:?}", self.type_)
        }
    }
}

impl Error {
    fn new_with_detail(type_: ErrorType, detail: Option<Value>) -> Self {
        Self { type_, detail }
    }
    pub fn new(type_: ErrorType) -> Self {
        Error::new_with_detail(type_, None)
    }
    pub fn missing_input(input: &str) -> Self {
        Error::new_with_detail(ErrorType::MissingInput, Some(json!({ "input": input })))
    }

    pub fn missing_metadata(input: &ObjectID) -> Self {
        Error::new_with_detail(ErrorType::MissingMetadata, Some(json!({ "input": input })))
    }

    pub fn unsupported_operation(type_: OperationType) -> Self {
        Error::new_with_detail(
            ErrorType::UnsupportedOperation,
            Some(json!({ "operation type": type_ })),
        )
    }

    pub fn new_with_msg(type_: ErrorType, msg: &str) -> Self {
        Error::new_with_detail(type_, Some(json!({ "message": msg })))
    }

    pub fn new_with_cause<E: Display>(type_: ErrorType, error: E) -> Self {
        Error::new_with_detail(type_, Some(json!({"cause": error.to_string()})))
    }
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let retriable = false;
        let error_code = self.type_ as u32;
        let message = format!("{:?}", &self.type_);

        if let Some(details) = &self.detail {
            json![{
                "code": error_code,
                "message": message,
                "retriable":retriable,
                "details": details,
            }]
        } else {
            json![{
                "code": error_code,
                "message": message,
                "retriable":retriable,
            }]
        }
        .serialize(serializer)
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}

impl From<SuiError> for Error {
    fn from(e: SuiError) -> Self {
        Error::new_with_cause(ErrorType::InternalError, e)
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::new_with_cause(ErrorType::InternalError, e)
    }
}
impl From<TryFromIntError> for Error {
    fn from(e: TryFromIntError) -> Self {
        Error::new_with_cause(ErrorType::ParsingError, e)
    }
}
impl From<ObjectIDParseError> for Error {
    fn from(e: ObjectIDParseError) -> Self {
        Error::new_with_cause(ErrorType::ParsingError, e)
    }
}
impl From<SignatureError> for Error {
    fn from(e: SignatureError) -> Self {
        Error::new_with_cause(ErrorType::SignatureError, e)
    }
}

impl From<bcs::Error> for Error {
    fn from(e: bcs::Error) -> Self {
        Error::new_with_cause(ErrorType::SerializationError, e)
    }
}

impl From<fastcrypto::error::FastCryptoError> for Error {
    fn from(e: fastcrypto::error::FastCryptoError) -> Self {
        Error::new_with_cause(ErrorType::SignatureError, e)
    }
}
