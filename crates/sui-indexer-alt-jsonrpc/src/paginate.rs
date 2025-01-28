// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::{
    encoding::{Base64, Encoding},
    error::FastCryptoError,
};
use serde::{de::DeserializeOwned, Serialize};

use crate::error::{invalid_params, RpcError};

/// This type wraps a value used as a cursor in a paginated request or response. Cursors are
/// serialized to JSON and then encoded as Base64.
pub(crate) struct Cursor<T>(pub T);

/// Description of a page to be fetched.
pub(crate) struct Page<T> {
    pub cursor: Option<Cursor<T>>,
    pub limit: i64,
    pub descending: bool,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Failed to decode Base64: {0}")]
    DecodingBase64(FastCryptoError),

    #[error("Failed to decode JSON: {0}")]
    DecodingJson(serde_json::error::Error),

    #[error("Failed to encode JSON: {0}")]
    EncodingJson(serde_json::error::Error),

    #[error("Requested page size {requested} exceeds maximum {max}")]
    ExceededMaxPageSize { requested: usize, max: usize },
}

impl<T: DeserializeOwned> Cursor<T> {
    /// Interpret the string as a cursor, Base64-decode it, and then deserialize it from JSON. A
    /// failure to do so implies the cursor is invalid, which is treated as a user error.
    pub(crate) fn decode(s: &str) -> Result<Self, Error> {
        let bytes = Base64::decode(s).map_err(Error::DecodingBase64)?;
        let value: T = serde_json::from_slice(&bytes).map_err(Error::DecodingJson)?;
        Ok(Cursor(value))
    }
}

impl<T: Serialize> Cursor<T> {
    /// Represent the cursor in JSON, Base64-encoded. A failure implies the cursor is not properly
    /// set-up, which is treated as an internal error.
    pub(crate) fn encode(&self) -> Result<String, Error> {
        let bytes = serde_json::to_vec(&self.0).map_err(Error::EncodingJson)?;
        Ok(Base64::encode(&bytes))
    }
}

impl<T: DeserializeOwned> Page<T> {
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
            .map(|c| Cursor::decode(&c))
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
