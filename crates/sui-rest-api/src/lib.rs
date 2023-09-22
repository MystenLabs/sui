// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{http::StatusCode, routing::get, Router};

mod checkpoints;
mod client;
pub mod headers;
mod objects;

pub use checkpoints::{CheckpointData, CheckpointTransaction};
pub use client::Client;

async fn health_check() -> StatusCode {
    StatusCode::OK
}

pub struct Bcs<T>(pub T);

pub const TEXT_PLAIN_UTF_8: &str = "text/plain; charset=utf-8";
pub const APPLICATION_BCS: &str = "application/bcs";
pub const APPLICATION_JSON: &str = "application/json";

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

pub fn rest_router<S>(state: S) -> Router
where
    S: sui_types::storage::CheckpointStore
        + sui_types::storage::TransactionStore
        + sui_types::storage::ObjectStore2
        + sui_types::storage::EventStore
        + Clone
        + Send
        + Sync
        + 'static,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(health_check))
        .merge(checkpoints::router(state.clone()))
        .merge(objects::router(state))
}

pub async fn start_service<S>(socket_address: std::net::SocketAddr, state: S)
where
    S: sui_types::storage::CheckpointStore
        + sui_types::storage::TransactionStore
        + sui_types::storage::ObjectStore2
        + sui_types::storage::EventStore
        + Clone
        + Send
        + Sync
        + 'static,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let app = rest_router(state);

    axum::Server::bind(&socket_address)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// Make our own error that wraps `anyhow::Error`.
pub struct AppError(anyhow::Error);

// Tell axum how to convert `AppError` into a response.
impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
// `Result<_, AppError>`. That way you don't need to do that manually.
impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
