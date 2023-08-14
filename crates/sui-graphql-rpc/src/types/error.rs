// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Error, ErrorExtensions};

#[derive(Debug, thiserror::Error)]
pub enum CustomError {
    #[error("{0}")]
    InputValueError(String),
    #[error("{0}")]
    ClientFetchError(String),
}

impl ErrorExtensions for CustomError {
    fn extend(&self) -> Error {
        Error::new(format!("{}", self)).extend_with(|_err, e| match self {
            CustomError::InputValueError(_) => {
                e.set("code", "INVALID_INPUT_VALUE");
            }
            CustomError::ClientFetchError(_) => {
                e.set("code", "CLIENT_FETCH_ERROR");
            }
        })
    }
}
