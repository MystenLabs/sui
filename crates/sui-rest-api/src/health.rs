// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    openapi::{ApiEndpoint, OperationBuilder, ResponseBuilder, RouteHandler},
    reader::StateReader,
    RestService, Result,
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use std::time::{Duration, SystemTime};
use sui_types::storage::ReadStore;
use tap::Pipe;

pub struct HealthCheck;

impl ApiEndpoint<RestService> for HealthCheck {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/health"
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("General")
            .operation_id("HealthCheck")
            .query_parameters::<Threshold>(generator)
            .response(200, ResponseBuilder::new().text_content().build())
            .build()
    }

    fn handler(&self) -> crate::openapi::RouteHandler<RestService> {
        RouteHandler::new(self.method(), health)
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Threshold {
    pub threshold_seconds: Option<u32>,
}

async fn health(
    Query(Threshold { threshold_seconds }): Query<Threshold>,
    State(state): State<StateReader>,
) -> Result<impl IntoResponse> {
    let summary = state.inner().get_latest_checkpoint()?;

    // If we have a provided threshold, check that it's close to the current time
    if let Some(threshold_seconds) = threshold_seconds {
        let latest_chain_time = summary.timestamp();

        let threshold = SystemTime::now() - Duration::from_secs(threshold_seconds as u64);

        if latest_chain_time < threshold {
            return Err(anyhow::anyhow!(
                "The latest checkpoint timestamp is less than the provided threshold"
            )
            .into());
        }
    }

    StatusCode::OK.pipe(Ok)
}
