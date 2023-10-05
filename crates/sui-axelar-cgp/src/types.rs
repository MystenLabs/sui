// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::json;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumProperty};
use thiserror::Error;
use tokio::task::JoinError;

use sui_sdk::types::digests::TransactionDigest;

#[derive(Debug, Error, EnumDiscriminants, EnumProperty)]
#[strum_discriminants(
    name(ErrorType),
    derive(Display, EnumIter),
    strum(serialize_all = "kebab-case")
)]
#[allow(clippy::enum_variant_names)]
pub enum Error {
    #[error(transparent)]
    UncategorizedError(#[from] anyhow::Error),

    #[error(transparent)]
    SuiError(#[from] sui_sdk::error::Error),

    #[error(transparent)]
    TokioJoinError(#[from] JoinError),
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let error = json!(self);
        error.serialize(serializer)
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProcessCommandsResponse {
    pub tx_hash: TransactionDigest,
}

impl IntoResponse for ProcessCommandsResponse {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}
