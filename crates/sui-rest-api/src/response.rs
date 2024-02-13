// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use reqwest::StatusCode;

use crate::{APPLICATION_BCS, TEXT_PLAIN_UTF_8};

pub struct Bcs<T>(pub T);

pub enum ResponseContent<T, J = T> {
    Bcs(T),
    Json(J),
}

impl<T> axum::response::IntoResponse for Bcs<T>
where
    T: serde::Serialize,
{
    fn into_response(self) -> axum::response::Response {
        match bcs::to_bytes(&self.0) {
            Ok(buf) => (
                [(
                    axum::http::header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static(APPLICATION_BCS),
                )],
                buf,
            )
                .into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(
                    axum::http::header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static(TEXT_PLAIN_UTF_8),
                )],
                err.to_string(),
            )
                .into_response(),
        }
    }
}

impl<T, J> axum::response::IntoResponse for ResponseContent<T, J>
where
    T: serde::Serialize,
    J: serde::Serialize,
{
    fn into_response(self) -> axum::response::Response {
        match self {
            ResponseContent::Bcs(inner) => Bcs(inner).into_response(),
            ResponseContent::Json(inner) => axum::Json(inner).into_response(),
        }
    }
}
