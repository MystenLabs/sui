// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Deref;

use async_graphql::InputValueError;
use async_graphql::InputValueResult;
use async_graphql::Scalar;
use async_graphql::ScalarType;
use async_graphql::Value;
use async_graphql::connection::CursorType;
use bytes::Bytes;
use fastcrypto::encoding::Base64;
use fastcrypto::encoding::Encoding;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Custom byte encoding for cursors.
///
/// Factors out the conversion to and from bytes for opaque cursors. Those bytes are further
/// decoded to and from Base64 to form an opaque cursor.
pub trait ByteCursor: Sized {
    fn decode_cursor(bytes: &[u8]) -> anyhow::Result<Self>;
    fn encode_cursor(&self) -> Bytes;
}

/// Cursor that hides its value by encoding it as JSON and then Base64.
///
/// In the GraphQL schema this will show up as a `String`.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct JsonCursor<C>(C);

/// Cursor that hides its value by serializing it to BCS and then encoding it as Base64.
///
/// In the GraphQL schema this will show up as a `String`.
#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct BcsCursor<C>(C);

/// Cursor that can be either a primary or secondary format.
///
/// When decoding, the primary format is attempted first, and if that fails, the secondary format
/// is attempted.
///
/// TODO: Remove once cursors have fully migrated to the new format.
#[derive(Clone, Debug)]
pub enum MultiCursor<P, S> {
    Primary(P),
    Secondary(S),
}

/// Cursor whose value uses an opaque, custom byte encoding, then encoded as Base64.
///
/// In the GraphQL schema this will show up as a `String`.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct OpaqueCursor<C>(C);

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid Base64")]
    BadBase64,

    #[error("Invalid BCS")]
    BadBcs,

    #[error("Invalid JSON")]
    BadJson,

    #[error("Invalid encoding: {0:#}")]
    BadEncoding(#[from] anyhow::Error),

    #[error("'{0}' and '{1}'")]
    BadMulti(Box<Error>, Box<Error>),
}

impl<C> JsonCursor<C> {
    pub fn new(cursor: C) -> Self {
        Self(cursor)
    }
}

impl<C> BcsCursor<C> {
    pub(crate) fn new(cursor: C) -> Self {
        Self(cursor)
    }
}

impl<P, S> MultiCursor<P, S> {
    pub(crate) fn new(cursor: P) -> Self {
        Self::Primary(cursor)
    }
}

impl<C: ByteCursor> OpaqueCursor<C> {
    pub(crate) fn new(inner: C) -> Self {
        Self(inner)
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

#[Scalar(name = "String", visible = false)]
impl<P, S> ScalarType for MultiCursor<P, S>
where
    P: Send + Sync,
    P: CursorType<Error = Error>,
    S: Send + Sync,
    S: CursorType<Error = Error>,
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
impl<C> ScalarType for OpaqueCursor<C>
where
    C: Send + Sync,
    C: ByteCursor,
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

impl<P, S> CursorType for MultiCursor<P, S>
where
    P: CursorType<Error = Error>,
    S: CursorType<Error = Error>,
{
    type Error = Error;

    fn decode_cursor(s: &str) -> Result<Self, Self::Error> {
        let errp = match P::decode_cursor(s) {
            Ok(cursor) => return Ok(Self::Primary(cursor)),
            Err(e) => Box::new(e),
        };

        let errs = match S::decode_cursor(s) {
            Ok(cursor) => return Ok(Self::Secondary(cursor)),
            Err(e) => Box::new(e),
        };

        Err(Error::BadMulti(errp, errs))
    }

    fn encode_cursor(&self) -> String {
        match self {
            Self::Primary(c) => c.encode_cursor(),
            Self::Secondary(c) => c.encode_cursor(),
        }
    }
}

impl<C> CursorType for OpaqueCursor<C>
where
    C: Send + Sync,
    C: ByteCursor,
{
    type Error = Error;

    fn decode_cursor(s: &str) -> Result<Self, Self::Error> {
        let bytes = Base64::decode(s).map_err(|_| Error::BadBase64)?;
        Ok(OpaqueCursor(C::decode_cursor(&bytes)?))
    }

    fn encode_cursor(&self) -> String {
        Base64::encode(self.0.encode_cursor())
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

impl<C> Deref for OpaqueCursor<C> {
    type Target = C;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
