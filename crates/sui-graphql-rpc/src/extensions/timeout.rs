// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextExecute},
    Response, ServerError,
};
use std::time::Duration;
use std::{net::SocketAddr, sync::Arc};
use tokio::time::timeout;
use tracing::error;
use uuid::Uuid;

use crate::{config::ServiceConfig, error::code, server::builder::QueryString};

#[derive(Clone, Debug, Default)]
pub(crate) struct Timeout;

impl ExtensionFactory for Timeout {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(Timeout)
    }
}

#[async_trait::async_trait]
impl Extension for Timeout {
    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let cfg = ctx
            .data::<ServiceConfig>()
            .expect("No service config provided in schema data");
        let request_timeout = Duration::from_millis(cfg.limits.request_timeout_ms);
        timeout(request_timeout, next.run(ctx, operation_name))
            .await
            .unwrap_or_else(|_| {
                let query: &QueryString = ctx.data_unchecked();
                let query = query.0.clone();
                let query_id: &Uuid = ctx.data_unchecked();
                let session_id: &SocketAddr = ctx.data_unchecked();
                let error_code = code::REQUEST_TIMEOUT.to_string();
                error!(
                    %query_id,
                    %session_id,
                    error_code,
                    "Query: {}", query
                );
                Response::from_errors(vec![ServerError::new(
                    format!(
                        "Request timed out. Limit: {}s",
                        request_timeout.as_secs_f32()
                    ),
                    None,
                )])
            })
    }
}
