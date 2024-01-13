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
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::server::builder::QueryUuid;

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
            query_id: "".to_string().into(),
            session_id: "".to_string().into(),
            config: self.config.clone(),
        })
    }
}

struct LoggerExtension {
    session_id: Mutex<String>,
    query_id: Mutex<String>,
    config: LoggerConfig,
}

impl LoggerExtension {
    /// Sets a unique id for each query that comes through
    async fn set_query_id(&self, query_id: Option<QueryUuid>) {
        let id = query_id.map(|id| id.uuid).unwrap_or_default();
        *self.query_id.lock().await = id;
    }

    /// Get the query uuid
    async fn query_id(&self) -> String {
        self.query_id.lock().await.clone()
    }

    async fn set_session_id(&self, ip: Option<SocketAddr>) {
        let ip_component = ip.map(|ip| format!("{}-", ip)).unwrap_or_default();
        let uuid_component = format!("{}", Uuid::new_v4());
        *self.session_id.lock().await = format!("{}{}", ip_component, uuid_component);
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
        self.set_query_id(ctx.data_opt::<QueryUuid>().cloned())
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
        // TODO figure out if we can use the query_id call directly in the logging macro
        let query_uuid = self.query_id().await;
        if !is_schema && self.config.log_request_query {
            info!(
                query_id = query_uuid,
                "[Query] {}: {}",
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
            let query_uuid = self.query_id().await;
            info!(
                query_id = query_uuid,
                complexity = res.complexity,
                depth = res.depth,
                "[Validation] {}",
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
        let query_uuid = self.query_id().await;
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
                        query_id = query_uuid,
                        "[Response] path={} message={}", path, err.message,
                    );
                } else {
                    error!(query_id = query_uuid, "[Response] message={}", err.message,);
                }
            }
        } else if self.config.log_response {
            match operation_name {
                Some("IntrospectionQuery") => {
                    debug!(
                        query_id = query_uuid,
                        "[Schema] {}: {}",
                        self.session_id().await,
                        resp.data
                    );
                }
                _ => info!(
                    query_id = query_uuid,
                    "[Response] {}: {}",
                    self.session_id().await,
                    resp.data
                ),
            }
        }
        resp
    }
}
