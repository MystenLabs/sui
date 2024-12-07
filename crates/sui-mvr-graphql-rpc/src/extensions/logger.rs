// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{error::code, metrics::Metrics};
use async_graphql::{
    extensions::{
        Extension, ExtensionContext, ExtensionFactory, NextExecute, NextParseQuery, NextResolve,
        NextValidation, ResolveInfo,
    },
    parser::types::{ExecutableDocument, OperationType, Selection},
    PathSegment, Response, ServerError, ServerResult, ValidationResult, Variables,
};
use async_graphql_value::ConstValue;
use std::{fmt::Write, net::SocketAddr, sync::Arc};
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
            log_request_query: false,
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
            config: self.config.clone(),
        })
    }
}

struct LoggerExtension {
    config: LoggerConfig,
}

#[async_trait::async_trait]
impl Extension for LoggerExtension {
    // This hook is used to get the top level node name for recording in the metrics which top
    // level nodes are being called.
    async fn resolve(
        &self,
        ctx: &ExtensionContext<'_>,
        info: ResolveInfo<'_>,
        next: NextResolve<'_>,
    ) -> ServerResult<Option<ConstValue>> {
        if info.path_node.parent.is_none() {
            ctx.data_unchecked::<Metrics>()
                .request_metrics
                .num_queries_top_level
                .with_label_values(&[info.name])
                .inc();
        }
        next.run(ctx, info).await
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
        let query_id: &Uuid = ctx.data_unchecked();
        let session_id: &SocketAddr = ctx.data_unchecked();
        if !is_schema && self.config.log_request_query {
            info!(
                %query_id,
                %session_id,
                "[Query] {}",
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
        let query_id: &Uuid = ctx.data_unchecked();
        let session_id: &SocketAddr = ctx.data_unchecked();
        if self.config.log_complexity {
            info!(
                %query_id,
                %session_id,
                complexity = res.complexity,
                depth = res.depth,
                "[Validation]",
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
        let query_id: &Uuid = ctx.data_unchecked();
        let session_id: &SocketAddr = ctx.data_unchecked();
        if resp.is_err() {
            for err in &resp.errors {
                let error_code = &err.extensions.as_ref().and_then(|x| x.get("code"));
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

                    if let Some(async_graphql_value::ConstValue::String(error_code)) = error_code {
                        if error_code.as_str() == code::INTERNAL_SERVER_ERROR {
                            error!(
                                %query_id,
                                %session_id,
                                error_code,
                                "[Response] path={} message={}",
                                path,
                                err.message,
                            );
                        } else {
                            info!(
                                %query_id,
                                %session_id,
                                error_code,
                                "[Response] path={} message={}",
                                path,
                                err.message,
                            );
                        }
                    } else {
                        warn!(
                            %query_id,
                            %session_id,
                            error_code = code::UNKNOWN,
                            "[Response] path={} message={}",
                            path,
                            err.message,
                        );
                    }
                } else {
                    let error_code = if let Some(error_code) = error_code {
                        error_code.to_string()
                    } else {
                        code::UNKNOWN.to_string()
                    };
                    info!(
                        %query_id,
                        %session_id,
                        error_code,
                        "[Response] message={}", err.message
                    )
                }
            }
        } else if self.config.log_response {
            match operation_name {
                Some("IntrospectionQuery") => {
                    debug!(
                        %query_id,
                        %session_id,
                        "[Schema] {}", resp.data
                    );
                }
                _ => info!(
                        %query_id,
                        %session_id,
                        "[Response] {}", resp.data
                ),
            }
        }
        resp
    }
}
