// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextExecute, NextParseQuery},
    parser::types::ExecutableDocument,
    Response, ServerError, ServerResult,
};
use async_graphql_value::Variables;
use std::sync::Mutex;
use std::time::Duration;
use std::{net::SocketAddr, sync::Arc};
use tokio::time::timeout;
use tracing::error;
use uuid::Uuid;

use crate::{config::ServiceConfig, error::code};

/// Extension factory for creating new `Timeout` instances, per query.
pub(crate) struct Timeout;

#[derive(Debug, Default)]
struct TimeoutExt {
    pub query: Mutex<Option<String>>,
}

impl ExtensionFactory for Timeout {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(TimeoutExt {
            query: Mutex::new(None),
        })
    }
}

#[async_trait::async_trait]
impl Extension for TimeoutExt {
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let document = next.run(ctx, query, variables).await?;
        *self.query.lock().unwrap() = Some(ctx.stringify_execute_doc(&document, variables));
        Ok(document)
    }

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
                let query_id: &Uuid = ctx.data_unchecked();
                let session_id: &SocketAddr = ctx.data_unchecked();
                let error_code = code::REQUEST_TIMEOUT;
                let guard = self.query.lock().unwrap();
                let query = match guard.as_ref() {
                    Some(s) => s.as_str(),
                    None => "",
                };

                error!(
                    %query_id,
                    %session_id,
                    %error_code,
                    %query
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
