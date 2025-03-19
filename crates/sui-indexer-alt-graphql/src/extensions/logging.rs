// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use async_graphql::{
    extensions::{
        Extension, ExtensionContext, ExtensionFactory, NextExecute, NextParseQuery, NextResolve,
        ResolveInfo,
    },
    parser::types::ExecutableDocument,
    Response, ServerResult, Value, Variables,
};
use serde_json::json;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{error::code, metrics::RpcMetrics};

/// Context data that tracks the session UUID and the client's address, to associate logs with a
/// particular request.
pub(crate) struct Session(Uuid, SocketAddr);

/// This extension is responsible for tracing and recording metrics for various GraphQL queries.
pub(crate) struct Logging(pub Arc<RpcMetrics>);

struct LoggingExt {
    query: Mutex<Option<String>>,
    metrics: Arc<RpcMetrics>,
}

impl Session {
    pub(crate) fn new(addr: SocketAddr) -> Self {
        Self(Uuid::new_v4(), addr)
    }
}

impl ExtensionFactory for Logging {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(LoggingExt {
            query: Mutex::new(None),
            metrics: self.0.clone(),
        })
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
        let Session(uuid, addr) = ctx.data_unchecked();

        self.metrics.queries_received.inc();
        self.metrics.queries_in_flight.inc();

        let guard = self.metrics.query_latency.start_timer();
        let response = next.run(ctx, operation_name).await;
        let elapsed_ms = guard.stop_and_record() * 1000.0;

        self.metrics.queries_in_flight.dec();
        if response.is_ok() {
            info!(%uuid, %addr, elapsed_ms, "Request succeeded");
            self.metrics.queries_succeeded.inc();
        } else {
            let codes = error_codes(&response);

            // Log internal errors and timeouts at a higher log level than other errors.
            if is_loud_query(&codes) {
                warn!(%uuid, %addr, query = self.query.lock().unwrap().as_ref().unwrap(), "Query");
            } else {
                debug!(%uuid, %addr, query = self.query.lock().unwrap().as_ref().unwrap(), "Query");
            }

            info!(%uuid, %addr, elapsed_ms, ?codes, "Request failed");

            if codes.is_empty() {
                self.metrics
                    .queries_failed
                    .with_label_values(&["<UNKNOWN>"])
                    .inc();
            }

            for code in &codes {
                self.metrics.queries_failed.with_label_values(&[code]).inc();
            }
        }

        debug!(%uuid, %addr, response = %json!(response), "Response");
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
        let doc = next.run(ctx, query, variables).await?;
        let query = ctx.stringify_execute_doc(&doc, variables);
        *self.query.lock().unwrap() = Some(query);
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
        self.metrics.fields_received.with_label_values(labels).inc();

        let result = next.run(ctx, info).await;
        if result.is_ok() {
            self.metrics.fields_succeeded.with_label_values(labels)
        } else {
            self.metrics.fields_failed.with_label_values(labels)
        }
        .inc();

        result
    }
}

/// Get a list of error codes from a GraphQL response. We use these to figure out whether we should
/// log the query at the `debug` or `info` level.
fn error_codes(response: &Response) -> Vec<&str> {
    response
        .errors
        .iter()
        .flat_map(|err| &err.extensions)
        .flat_map(|ext| ext.get("code"))
        .filter_map(|code| {
            if let Value::String(code) = code {
                Some(code.as_str())
            } else {
                None
            }
        })
        .collect()
}

/// Whether the query should be logged at a "louder" level (e.g. `warn!` instead of `debug!`),
/// because it's related to some problem that we should probably investigate.
fn is_loud_query(codes: &[&str]) -> bool {
    codes
        .iter()
        .any(|c| matches!(*c, code::REQUEST_TIMEOUT | code::INTERNAL_SERVER_ERROR))
}
