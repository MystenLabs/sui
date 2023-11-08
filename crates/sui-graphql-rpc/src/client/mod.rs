// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Value;
use hyper::header::ToStrError;
use serde_json::Number;

pub mod response;
pub mod simple_client;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Service version header not found")]
    ServiceVersionHeaderNotFound,
    #[error("Service version header value invalid string: {error}")]
    ServiceVersionHeaderValueInvalidString { error: ToStrError },
    #[error("Invalid usage number for {usage_name}: {usage_number}")]
    InvalidUsageNumber {
        usage_name: String,
        usage_number: Number,
    },
    #[error("Invalid usage field for {usage_name}: {usage_value}")]
    InvalidUsageValue {
        usage_name: String,
        usage_value: Value,
    },
    #[error(transparent)]
    InnerClientError(#[from] reqwest::Error),
}
