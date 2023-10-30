// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextRequest},
    Response, ServerError,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

// 10s
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_millis(20_000);

#[derive(Clone, Debug, Copy)]
pub(crate) struct TimeoutConfig {
    pub request_timeout: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Timeout {
    pub config: TimeoutConfig,
}

impl ExtensionFactory for Timeout {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(TimeoutExtension {
            config: self.config,
        })
    }
}

struct TimeoutExtension {
    config: TimeoutConfig,
}

#[async_trait::async_trait]
impl Extension for TimeoutExtension {
    async fn request(&self, ctx: &ExtensionContext<'_>, next: NextRequest<'_>) -> Response {
        timeout(self.config.request_timeout, next.run(ctx))
            .await
            .unwrap_or_else(|_| {
                Response::from_errors(vec![ServerError::new(
                    format!(
                        "Request timed out. Limit: {}s",
                        self.config.request_timeout.as_secs_f32()
                    ),
                    None,
                )])
            })
    }
}
