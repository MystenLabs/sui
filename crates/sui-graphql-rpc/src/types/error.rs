// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Error, ErrorExtensions};

#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum CustomError {
    #[error("{0}")]
    Input(String),
    #[error("{0}")]
    ClientFetch(String),
    #[error("Internal error occurred")]
    Internal,
}

impl ErrorExtensions for CustomError {
    fn extend(&self) -> Error {
        Error::new(format!("{}", self)).extend_with(|_err, e| match self {
            CustomError::Input(_) => {
                e.set("code", "INVALID_INPUT");
            }
            CustomError::ClientFetch(_) => {
                e.set("code", "CLIENT_FETCH_ERROR");
            }
            CustomError::Internal => {
                e.set("code", "INTERNAL_ERROR");
            }
        })
    }
}
