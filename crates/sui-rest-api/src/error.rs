// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::http::StatusCode;

pub type Result<T, E = RestError> = std::result::Result<T, E>;

pub struct RestError {
    status: StatusCode,
    message: Option<String>,
}

impl RestError {
    pub fn new(status: StatusCode, message: String) -> Self {
        Self {
            status,
            message: Some(message),
        }
    }
}

// Tell axum how to convert `AppError` into a response.
impl axum::response::IntoResponse for RestError {
    fn into_response(self) -> axum::response::Response {
        match self.message {
            Some(message) => (self.status, message).into_response(),
            None => self.status.into_response(),
        }
    }
}

impl From<sui_types::storage::error::Error> for RestError {
    fn from(value: sui_types::storage::error::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: Some(value.to_string()),
        }
    }
}

impl From<anyhow::Error> for RestError {
    fn from(value: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: Some(value.to_string()),
        }
    }
}
