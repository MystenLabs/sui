// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    extensions::{
        Extension, ExtensionContext, ExtensionFactory, NextExecute, NextParseQuery,
        NextPrepareRequest, NextValidation,
    },
    parser::types::{ExecutableDocument, OperationType, Selection},
    PathSegment, Request, Response, ServerError, ServerResult, ValidationResult, Variables,
};
use std::{fmt::Write, net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

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
            session_id: "".to_string().into(),
            config: self.config.clone(),
        })
    }
}

struct LoggerExtension {
    session_id: Mutex<String>,
    config: LoggerConfig,
}

impl LoggerExtension {
    async fn set_session_id(&self, ip: Option<SocketAddr>) {
        *self.session_id.lock().await = ip.map(|ip| format!("{}-", ip)).unwrap_or_default();
    }

    async fn session_id(&self) -> String {
        self.session_id.lock().await.clone()
    }
}

#[async_trait::async_trait]
impl Extension for LoggerExtension {
    /// Called at prepare request.
    async fn prepare_request(
        &self,
        ctx: &ExtensionContext<'_>,
        request: Request,
        next: NextPrepareRequest<'_>,
    ) -> ServerResult<Request> {
        self.set_session_id(ctx.data_opt::<SocketAddr>().copied())
            .await;
        next.run(ctx, request).await
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
                query_id = ctx.data_unchecked::<Uuid>().to_string(),
                "{}: {}",
                self.session_id().await,
                ctx.stringify_execute_doc(&document, variables)
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
                query_id = ctx.data_unchecked::<Uuid>().to_string(),
                complexity = res.complexity,
                depth = res.depth,
                "{}",
                self.session_id().await
            );
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
                    if let Some(ext) = &err.extensions {
                        if let Some(error_code) = ext.get("code") {
                            if let async_graphql::Value::String(val) = error_code {
                                if val.as_str() == crate::error::code::INTERNAL_SERVER_ERROR {
                                    // TODO do we want/it's useful to log the whole problematic query?
                                    error!(
                                        query_id = ctx.data_unchecked::<Uuid>().to_string(),
                                        "path={} message={}", path, err.message,
                                    );
                                } else {
                                    info!(
                                        query_id = ctx.data_unchecked::<Uuid>().to_string(),
                                        "path={} message={}", path, err.message,
                                    );
                                }
                            }
                        }
                    } else {
                        warn!(
                            query_id = ctx.data_unchecked::<Uuid>().to_string(),
                            no_error_code = "true",
                            "path={} message={}",
                            path,
                            err.message,
                        );
                    }
                } else {
                    warn!(
                        query_id = ctx.data_unchecked::<Uuid>().to_string(),
                        "message={}", err.message,
                    );
                }
            }
        } else if self.config.log_response {
            match operation_name {
                Some("IntrospectionQuery") => {
                    debug!(
                        query_id = ctx.data_unchecked::<Uuid>().to_string(),
                        "{}: {}",
                        self.session_id().await,
                        resp.data
                    );
                }
                _ => info!(
                    query_id = ctx.data_unchecked::<Uuid>().to_string(),
                    "{}: {}",
                    self.session_id().await,
                    resp.data
                ),
            }
        }
        resp
    }
}
