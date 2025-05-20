// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Deref;

use async_graphql::{
    connection::CursorType, InputValueError, InputValueResult, Scalar, ScalarType, Value,
};
use fastcrypto::encoding::{Base64, Encoding};
use serde::{de::DeserializeOwned, Serialize};

/// Cursor that hides its value by encoding it as JSON and then Base64.
///
/// In the GraphQL schema this will show up as a `String`.
#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct JsonCursor<C>(C);

/// Cursor that hides its value by serializing it to BCS and then encoding it as Base64.
///
/// In the GraphQL schema this will show up as a `String`.
#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct BcsCursor<C>(C);

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Invalid Base64")]
    BadBase64,

    #[error("Invalid BCS")]
    BadBcs,

    #[error("Invalid JSON")]
    BadJson,
}

impl<C> JsonCursor<C> {
    pub(crate) fn new(cursor: C) -> Self {
        Self(cursor)
    }
}

impl<C> BcsCursor<C> {
    pub(crate) fn new(cursor: C) -> Self {
        Self(cursor)
    }
}

#[Scalar(name = "String", visible = false)]
impl<C> ScalarType for JsonCursor<C>
where
    C: Send + Sync,
    C: Serialize + DeserializeOwned,
{
    fn parse(value: Value) -> InputValueResult<Self> {
        if let Value::String(s) = value {
            Self::decode_cursor(&s).map_err(InputValueError::custom)
        } else {
            Err(InputValueError::expected_type(value))
        }
    }

    /// Just check that the value is a string, as we'll do more involved tests during parsing.
    fn is_valid(value: &Value) -> bool {
        matches!(value, Value::String(_))
    }

    fn to_value(&self) -> Value {
        Value::String(self.encode_cursor())
    }
}

#[Scalar(name = "String", visible = false)]
impl<C> ScalarType for BcsCursor<C>
where
    C: Send + Sync,
    C: Serialize + DeserializeOwned,
{
    fn parse(value: Value) -> InputValueResult<Self> {
        if let Value::String(s) = value {
            Self::decode_cursor(&s).map_err(InputValueError::custom)
        } else {
            Err(InputValueError::expected_type(value))
        }
    }

    /// Just check that the value is a string, as we'll do more involved tests during parsing.
    fn is_valid(value: &Value) -> bool {
        matches!(value, Value::String(_))
    }

    fn to_value(&self) -> Value {
        Value::String(self.encode_cursor())
    }
}

impl<C> CursorType for JsonCursor<C>
where
    C: Send + Sync,
    C: Serialize + DeserializeOwned,
{
    type Error = Error;

    fn decode_cursor(s: &str) -> Result<Self, Self::Error> {
        let bytes = Base64::decode(s).map_err(|_| Error::BadBase64)?;
        let cursor = serde_json::from_slice(&bytes).map_err(|_| Error::BadJson)?;
        Ok(JsonCursor(cursor))
    }

    fn encode_cursor(&self) -> String {
        Base64::encode(serde_json::to_vec(&self.0).unwrap_or_default())
    }
}

impl<C> CursorType for BcsCursor<C>
where
    C: Send + Sync,
    C: Serialize + DeserializeOwned,
{
    type Error = Error;

    fn decode_cursor(s: &str) -> Result<Self, Self::Error> {
        let bytes = Base64::decode(s).map_err(|_| Error::BadBase64)?;
        let cursor = bcs::from_bytes(&bytes).map_err(|_| Error::BadBcs)?;
        Ok(BcsCursor(cursor))
    }

    fn encode_cursor(&self) -> String {
        Base64::encode(bcs::to_bytes(&self.0).unwrap_or_default())
    }
}

impl<C> Deref for JsonCursor<C> {
    type Target = C;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<C> Deref for BcsCursor<C> {
    type Target = C;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
