// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    extensions::{
        Extension, ExtensionContext, ExtensionFactory, NextExecute, NextParseQuery, NextRequest,
        NextValidation,
    },
    parser::types::{ExecutableDocument, OperationType, Selection},
    PathSegment, Response, ServerError, ServerResult, ValidationResult, Variables,
};
use std::{fmt::Write, sync::Arc};
use tokio::sync::Mutex;
use tracing::{error, info};
use uuid::Uuid;

// TODO: mode in-depth logging to debug

#[derive(Clone, Debug)]
pub struct LoggerConfig {
    pub log_request_query: bool,
    pub log_response: bool,
    pub log_complexity: bool,
}

impl Default for LoggerConfig {
    fn default() -> Self {
        Self {
            log_request_query: true,
            log_response: true,
            log_complexity: true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Logger {
    config: LoggerConfig,
}

impl ExtensionFactory for Logger {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(LoggerExtension {
            session_id: None.into(),
            config: self.config.clone(),
        })
    }
}

struct LoggerExtension {
    session_id: Mutex<Option<Uuid>>,
    config: LoggerConfig,
}

impl LoggerExtension {
    async fn session_id(&self) -> Option<Uuid> {
        *self.session_id.lock().await
    }
}

#[async_trait::async_trait]
impl Extension for LoggerExtension {
    async fn request(&self, ctx: &ExtensionContext<'_>, next: NextRequest<'_>) -> Response {
        *self.session_id.lock().await = Some(Uuid::new_v4());
        next.run(ctx).await
    }

    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let document = next.run(ctx, query, variables).await?;
        let is_schema = document
            .operations
            .iter()
            .filter(|(_, operation)| operation.node.ty == OperationType::Query)
            .any(|(_, operation)| operation.node.selection_set.node.items.iter().any(|selection| matches!(&selection.node, Selection::Field(field) if field.node.name.node == "__schema")));
        if !is_schema && self.config.log_request_query {
            info!(
                target: "async-graphql",
                "[Query] {}: {}", self.session_id().await.unwrap(), ctx.stringify_execute_doc(&document, variables)
            );
        }
        Ok(document)
    }

    async fn validation(
        &self,
        ctx: &ExtensionContext<'_>,
        next: NextValidation<'_>,
    ) -> Result<ValidationResult, Vec<ServerError>> {
        let res = next.run(ctx).await?;
        if self.config.log_complexity {
            info!(
                target: "async-graphql",
                complexity = res.complexity,
                depth = res.depth,
                "[Validation] {}", self.session_id().await.unwrap());
        }
        Ok(res)
    }

    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let resp = next.run(ctx, operation_name).await;
        if resp.is_err() {
            for err in &resp.errors {
                if !err.path.is_empty() {
                    let mut path = String::new();
                    for (idx, s) in err.path.iter().enumerate() {
                        if idx > 0 {
                            path.push('.');
                        }
                        match s {
                            PathSegment::Index(idx) => {
                                let _ = write!(&mut path, "{}", idx);
                            }
                            PathSegment::Field(name) => {
                                let _ = write!(&mut path, "{}", name);
                            }
                        }
                    }

                    error!(
                        target: "async-graphql",
                        "[Response] path={} message={}", path, err.message,
                    );
                } else {
                    error!(
                        target: "async-graphql",
                        "[Response] message={}", err.message,
                    );
                }
            }
        } else if self.config.log_response {
            info!(
                target: "async-graphql",
                "[Response] {}: {}", self.session_id().await.unwrap(), resp.data
            );
        }
        resp
    }
}
