// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use axum::{Json, extract::State, response::IntoResponse};

use crate::api::types::{ApiResponse, ForkingStatus};

/// The shared state for the forking server.
pub(super) struct AppState {
    pub context: crate::context::Context,
}

impl AppState {
    pub async fn new(context: crate::context::Context) -> Self {
        Self { context }
    }
}

pub(super) async fn health() -> &'static str {
    "OK"
}

pub(super) async fn get_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let sim = state.context.simulacrum.read().await;
    let store = sim.store();

    let checkpoint = store
        .get_highest_checkpint()
        .map(|c| c.sequence_number)
        .unwrap_or(0);
    let epoch = store.get_highest_checkpint().map(|c| c.epoch()).unwrap_or(0);
    let clock_timestamp_ms = store.get_clock().timestamp_ms();

    Json(ApiResponse {
        success: true,
        data: Some(ForkingStatus {
            checkpoint,
            epoch,
            clock_timestamp_ms,
        }),
        error: None,
    })
}
