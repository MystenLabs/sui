// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextExecute, NextParseQuery},
    parser::types::{ExecutableDocument, OperationType},
    Response, ServerError, ServerResult,
};
use async_graphql_value::Variables;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
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
    pub is_mutation: AtomicBool,
}

impl ExtensionFactory for Timeout {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(TimeoutExt {
            query: Mutex::new(None),
            is_mutation: AtomicBool::new(false),
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

        let is_mutation = document
            .operations
            .iter()
            .any(|(_, operation)| operation.node.ty == OperationType::Mutation);
        self.is_mutation.store(is_mutation, Ordering::Relaxed);

        Ok(document)
    }

    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let cfg: &ServiceConfig = ctx
            .data()
            .expect("No service config provided in schema data");

        // increase the timeout if the request is a mutation
        let is_mutation = self.is_mutation.load(Ordering::Relaxed);
        let request_timeout = if is_mutation {
            Duration::from_millis(cfg.limits.mutation_timeout_ms.into())
        } else {
            Duration::from_millis(cfg.limits.request_timeout_ms.into())
        };

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
                let error_msg = if is_mutation {
                    format!(
                        "Mutation request timed out. Limit: {}s",
                        request_timeout.as_secs_f32()
                    )
                } else {
                    format!(
                        "Query request timed out. Limit: {}s",
                        request_timeout.as_secs_f32()
                    )
                };
                Response::from_errors(vec![ServerError::new(error_msg, None)])
            })
    }
}
