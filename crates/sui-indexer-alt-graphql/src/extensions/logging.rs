// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{net::SocketAddr, sync::Arc};

use async_graphql::{
    extensions::{
        Extension, ExtensionContext, ExtensionFactory, NextExecute, NextParseQuery, NextResolve,
        ResolveInfo,
    },
    parser::types::ExecutableDocument,
    Response, ServerResult, Value, Variables,
};
use serde_json::json;
use tracing::{debug, info};
use uuid::Uuid;

use crate::metrics::RpcMetrics;

/// Context data that tracks the session UUID and the client's address, to associate logs with a
/// particular request.
pub(crate) struct Session(Uuid, SocketAddr);

/// This extension is responsible for tracing and recording metrics for various GraphQL queries.
pub(crate) struct Logging(pub Arc<RpcMetrics>);

struct LoggingExt(pub Arc<RpcMetrics>);

impl Session {
    pub(crate) fn new(addr: SocketAddr) -> Self {
        Self(Uuid::new_v4(), addr)
    }
}

impl ExtensionFactory for Logging {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(LoggingExt(self.0.clone()))
    }
}

#[async_trait::async_trait]
impl Extension for LoggingExt {
    /// Track query-wide metrics
    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let Some(Session(uuid, addr)) = ctx.data_opt() else {
            return next.run(ctx, operation_name).await;
        };

        self.0.queries_received.inc();
        self.0.queries_in_flight.inc();

        let guard = self.0.query_latency.start_timer();
        let response = next.run(ctx, operation_name).await;
        let elapsed_ms = guard.stop_and_record() * 1000.0;

        debug!(%uuid, %addr, response = %json!(response), "Response");

        self.0.queries_in_flight.dec();
        if response.is_ok() {
            info!(%uuid, %addr, elapsed_ms, "Request succeeded");
            self.0.queries_succeeded.inc();
        } else {
            info!(%uuid, %addr, elapsed_ms, "Request failed");
            self.0.queries_failed.inc();
        }

        response
    }

    /// debug! trace the query.
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let Some(Session(uuid, addr)) = ctx.data_opt() else {
            return next.run(ctx, query, variables).await;
        };

        let doc = next.run(ctx, query, variables).await?;
        debug!(%uuid, %addr, query = ctx.stringify_execute_doc(&doc, variables), "Query");
        Ok(doc)
    }

    /// Track metrics per field
    async fn resolve(
        &self,
        ctx: &ExtensionContext<'_>,
        info: ResolveInfo<'_>,
        next: NextResolve<'_>,
    ) -> ServerResult<Option<Value>> {
        let labels = &[info.parent_type, info.name];
        self.0.fields_received.with_label_values(labels).inc();

        let result = next.run(ctx, info).await;
        if result.is_ok() {
            self.0.fields_succeeded.with_label_values(labels).inc();
        } else {
            self.0.fields_failed.with_label_values(labels).inc();
        }

        result
    }
}
