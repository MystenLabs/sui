// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::http::StatusCode;
use std::fmt::Display;
use std::num::TryFromIntError;

use axum::response::{IntoResponse, Response};
use axum::Json;
use itertools::Itertools;
use serde::Serialize;
use serde::{Deserialize, Serializer};
use serde_json::{json, Value};
use signature::Error as SignatureError;
use strum_macros::EnumIter;
use sui_types::base_types::ObjectIDParseError;
use sui_types::error::SuiError;

#[derive(Eq, PartialEq, Copy, Clone, Debug, Serialize, Deserialize, EnumIter)]
#[serde(rename_all = "lowercase")]
pub enum ErrorType {
    UnsupportedBlockchain = 1,
    UnsupportedNetwork,
    InvalidInput,
    MissingInput,
    InternalError,
    DataError,
    UnsupportedOperation,
    ParsingError,
    IncorrectSignerAddress,
    SignatureError,
    SerializationError,
    UnimplementedTransactionType,
}

pub struct Error {
    type_: ErrorType,
    detail: Option<Value>,
}

impl Error {
    pub fn new(type_: ErrorType) -> Self {
        Self {
            type_,
            detail: None,
        }
    }
    pub fn new_with_detail(type_: ErrorType, detail: Value) -> Self {
        Self {
            type_,
            detail: Some(detail),
        }
    }
    pub fn new_with_cause<E: Display>(type_: ErrorType, error: E) -> Self {
        Self {
            type_,
            detail: Some(json!({"cause": error.to_string()})),
        }
    }
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let retriable = false;
        let error_code = self.type_ as u32;
        // Add space before upper case char, are there better ways?
        let message = format!("{:?}", &self.type_)
            .chars()
            .rev()
            .collect::<String>()
            .split_inclusive(char::is_uppercase)
            .join(" ")
            .chars()
            .rev()
            .collect::<String>();

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
