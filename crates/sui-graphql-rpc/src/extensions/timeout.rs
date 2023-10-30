// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextRequest},
    Response, ServerError,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use crate::config::ServiceConfig;

#[derive(Clone, Debug, Default)]
pub(crate) struct Timeout;

impl ExtensionFactory for Timeout {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(Timeout)
    }
}

#[async_trait::async_trait]
impl Extension for Timeout {
    async fn request(&self, ctx: &ExtensionContext<'_>, next: NextRequest<'_>) -> Response {
        let cfg = ctx
            .data::<ServiceConfig>()
            .expect("No service config provided in schema data");
        let request_timeout = Duration::from_millis(cfg.limits.request_timeout_ms);

        timeout(request_timeout, next.run(ctx))
            .await
            .unwrap_or_else(|_| {
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
