// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use async_graphql::{
    extensions::{
        Extension, ExtensionContext, ExtensionFactory, NextParseQuery, NextPrepareRequest,
        NextRequest, NextResolve, NextValidation, ResolveInfo,
    },
    parser::types::ExecutableDocument,
    Request, Response, ServerError, ServerResult, ValidationResult, Value, Variables,
};
use serde_json::json;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    error::{code, error_codes, fill_error_code},
    metrics::RpcMetrics,
};

/// Context data that tracks the session UUID and the client's address, to associate logs with a
/// particular request.
#[derive(Copy, Clone)]
pub(crate) struct Session(Uuid, SocketAddr);

/// This extension is responsible for tracing and recording metrics for various GraphQL queries.
pub(crate) struct Logging(pub Arc<RpcMetrics>);

struct LoggingExt {
    session: Mutex<Option<Session>>,
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
            session: Mutex::new(None),
            query: Mutex::new(None),
            metrics: self.0.clone(),
        })
    }
}

#[async_trait::async_trait]
impl Extension for LoggingExt {
    async fn request(&self, ctx: &ExtensionContext<'_>, next: NextRequest<'_>) -> Response {
        self.metrics.queries_received.inc();
        self.metrics.queries_in_flight.inc();
        let guard = self.metrics.query_latency.start_timer();
        let response = next.run(ctx).await;
        let elapsed_ms = guard.stop_and_record() * 1000.0;
        self.metrics.queries_in_flight.dec();

        // SAFETY: This is set by `prepare_request`.
        let Session(uuid, addr) = self.session.lock().unwrap().unwrap();

        if response.is_ok() {
            info!(%uuid, %addr, elapsed_ms, "Request succeeded");
            self.metrics.queries_succeeded.inc();
        } else {
            let codes = error_codes(&response);

            // Log internal errors, timeouts, and unknown errors at a higher log level than other errors.
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

    /// Capture Session information from the Context so that the `request` handler can use it for
    /// logging, once it has finished executing.
    async fn prepare_request(
        &self,
        ctx: &ExtensionContext<'_>,
        request: Request,
        next: NextPrepareRequest<'_>,
    ) -> ServerResult<Request> {
        let session = ctx.data_unchecked();
        *self.session.lock().unwrap() = Some(*session);
        next.run(ctx, request).await
    }

    /// Check for parse errors and capture the query in case we need to log it.
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let doc = next.run(ctx, query, variables).await.map_err(|mut err| {
            fill_error_code(&mut err.extensions, code::GRAPHQL_PARSE_FAILED);
            err
        })?;

        let query = ctx.stringify_execute_doc(&doc, variables);
        *self.query.lock().unwrap() = Some(query);
        Ok(doc)
    }

    async fn validation(
        &self,
        ctx: &ExtensionContext<'_>,
        next: NextValidation<'_>,
    ) -> Result<ValidationResult, Vec<ServerError>> {
        next.run(ctx).await.map_err(|mut errs| {
            for err in &mut errs {
                fill_error_code(&mut err.extensions, code::GRAPHQL_VALIDATION_FAILED);
            }
            errs
        })
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

/// Whether the query should be logged at a "louder" level (e.g. `warn!` instead of `debug!`),
/// because it's related to some problem that we should probably investigate.
fn is_loud_query(codes: &[&str]) -> bool {
    codes.is_empty()
        || codes
            .iter()
            .any(|c| matches!(*c, code::REQUEST_TIMEOUT | code::INTERNAL_SERVER_ERROR))
}

#[cfg(test)]
mod tests {
    use async_graphql::{EmptyMutation, EmptySubscription, Object, Schema};
    use prometheus::Registry;

    use super::*;

    struct Query;

    #[Object]
    impl Query {
        async fn op(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn parsing_error_code() {
        let registry = Registry::new();
        let metrics = RpcMetrics::new(&registry);

        let request = Request::from("{ op").data(Session::new("0.0.0.0:0".parse().unwrap()));
        let response = Schema::build(Query, EmptyMutation, EmptySubscription)
            .extension(Logging(metrics.clone()))
            .finish()
            .execute(request)
            .await;

        assert!(response.is_err());
        assert_eq!(error_codes(&response), vec![code::GRAPHQL_PARSE_FAILED]);
        assert_eq!(metrics.queries_received.get(), 1);
        assert_eq!(
            metrics
                .queries_failed
                .with_label_values(&[code::GRAPHQL_PARSE_FAILED])
                .get(),
            1
        );
    }

    #[tokio::test]
    async fn validation_error_code() {
        let registry = Registry::new();
        let metrics = RpcMetrics::new(&registry);

        let request = Request::from("query ($foo: String) { op }")
            .data(Session::new("0.0.0.0:0".parse().unwrap()));

        let response = Schema::build(Query, EmptyMutation, EmptySubscription)
            .extension(Logging(metrics.clone()))
            .finish()
            .execute(request)
            .await;

        assert!(response.is_err());
        assert_eq!(
            error_codes(&response),
            vec![code::GRAPHQL_VALIDATION_FAILED]
        );
        assert_eq!(metrics.queries_received.get(), 1);
        assert_eq!(
            metrics
                .queries_failed
                .with_label_values(&[code::GRAPHQL_VALIDATION_FAILED])
                .get(),
            1
        );
    }
}
