// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Deref;

use fastcrypto::{
    encoding::{Base64, Encoding},
    error::FastCryptoError,
};
use serde::{de::DeserializeOwned, Serialize};

use crate::error::{invalid_params, RpcError};

pub(crate) trait Cursor: Sized {
    /// Interpret the string as a cursor, Base64-decode it, and then deserialize it from JSON. A
    /// failure to do so implies the cursor is invalid, which is treated as a user error.
    fn decode(s: &str) -> Result<Self, Error>;

    /// Represent the cursor in JSON, Base64-encoded. A failure implies the cursor is not properly
    /// set-up, which is treated as an internal error.
    fn encode(&self) -> Result<String, Error>;
}

/// Wraps a value used as a cursor in a paginated request or response. This cursor format
/// serializes to BCS and then encodes as Base64.
pub(crate) struct BcsCursor<T>(pub T);

/// Wraps a value used as a cursor in a paginated request or response. This cursor format
/// serializes to JSON and then encodes as Base64.
pub(crate) struct JsonCursor<T>(pub T);

/// Description of a page to be fetched.
pub(crate) struct Page<C: Cursor> {
    pub cursor: Option<C>,
    pub limit: i64,
    pub descending: bool,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Failed to decode Base64: {0}")]
    DecodingBase64(FastCryptoError),

    #[error("Failed to decode BCS: {0}")]
    DecodingBcs(bcs::Error),

    #[error("Failed to decode JSON: {0}")]
    DecodingJson(serde_json::error::Error),

    #[error("Failed to encode BCS: {0}")]
    EncodingBcs(bcs::Error),

    #[error("Failed to encode JSON: {0}")]
    EncodingJson(serde_json::error::Error),

    #[error("Requested page size {requested} exceeds maximum {max}")]
    ExceededMaxPageSize { requested: usize, max: usize },
}

impl<T: Serialize + DeserializeOwned> Cursor for BcsCursor<T> {
    fn decode(s: &str) -> Result<Self, Error> {
        let bytes = Base64::decode(s).map_err(Error::DecodingBase64)?;
        let value = bcs::from_bytes(&bytes).map_err(Error::DecodingBcs)?;
        Ok(BcsCursor(value))
    }

    fn encode(&self) -> Result<String, Error> {
        let bytes = bcs::to_bytes(&self.0).map_err(Error::EncodingBcs)?;
        Ok(Base64::encode(&bytes))
    }
}

impl<T: Serialize + DeserializeOwned> Cursor for JsonCursor<T> {
    fn decode(s: &str) -> Result<Self, Error> {
        let bytes = Base64::decode(s).map_err(Error::DecodingBase64)?;
        let value: T = serde_json::from_slice(&bytes).map_err(Error::DecodingJson)?;
        Ok(JsonCursor(value))
    }

    fn encode(&self) -> Result<String, Error> {
        let bytes = serde_json::to_vec(&self.0).map_err(Error::EncodingJson)?;
        Ok(Base64::encode(&bytes))
    }
}

impl<C: Cursor> Page<C> {
    /// Interpret RPC method parameters as a description of a page to fetch.
    ///
    /// This operation can fail if the Cursor cannot be decoded, or the requested page is too
    /// large. These are all consider user errors.
    pub(crate) fn from_params<E: From<Error> + std::error::Error>(
        default_page_size: usize,
        max_page_size: usize,
        cursor: Option<String>,
        limit: Option<usize>,
        descending: Option<bool>,
    ) -> Result<Self, RpcError<E>> {
        let cursor = cursor
            .map(|c| C::decode(&c))
            .transpose()
            .map_err(|e| invalid_params(E::from(e)))?;

        let limit = limit.unwrap_or(default_page_size);
        if limit > max_page_size {
            return Err(invalid_params(E::from(Error::ExceededMaxPageSize {
                requested: limit,
                max: max_page_size,
            })));
        }

        Ok(Page {
            cursor,
            limit: limit as i64,
            descending: descending.unwrap_or(false),
        })
    }
}

impl<T> Deref for BcsCursor<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> Deref for JsonCursor<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}
