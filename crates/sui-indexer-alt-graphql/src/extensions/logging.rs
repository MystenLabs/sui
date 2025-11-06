// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::{Arc, OnceLock},
    task::{Context, Poll},
};

use async_graphql::{
    Request, Response, ServerError, ServerResult, ValidationResult, Value, Variables,
    extensions::{
        Extension, ExtensionContext, ExtensionFactory, NextParseQuery, NextPrepareRequest,
        NextRequest, NextResolve, NextValidation, ResolveInfo,
    },
    parser::types::ExecutableDocument,
};
use axum::http::HeaderName;
use pin_project::{pin_project, pinned_drop};
use prometheus::HistogramTimer;
use serde_json::json;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    error::{code, error_codes, fill_error_code},
    metrics::RpcMetrics,
};

/// This custom response header contains a unique request-id used for debugging and appears in the logs.
pub const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-sui-rpc-request-id");

/// Context data that tracks the session UUID and the client's address, to associate logs with a
/// particular request.
#[derive(Copy, Clone)]
pub(crate) struct Session {
    pub uuid: Uuid,
    pub addr: SocketAddr,
}

/// This extension is responsible for tracing and recording metrics for various GraphQL queries.
pub(crate) struct Logging(pub Arc<RpcMetrics>);

#[derive(Clone)]
struct LoggingExt {
    session: Arc<OnceLock<Session>>,
    query: Arc<OnceLock<String>>,
    metrics: Arc<RpcMetrics>,
}

struct RequestMetrics {
    timer: HistogramTimer,
    ext: LoggingExt,
}

#[pin_project(PinnedDrop)]
struct MetricsFuture<F> {
    metrics: Option<RequestMetrics>,
    #[pin]
    inner: F,
}

impl Session {
    pub(crate) fn new(addr: SocketAddr) -> Self {
        Self {
            uuid: Uuid::new_v4(),
            addr,
        }
    }
}

impl<F> MetricsFuture<F> {
    fn request(ext: &LoggingExt, inner: F) -> Self
    where
        F: Future<Output = Response>,
    {
        ext.metrics.queries_received.inc();
        ext.metrics.queries_in_flight.inc();
        let guard = ext.metrics.query_latency.start_timer();

        MetricsFuture {
            metrics: Some(RequestMetrics {
                timer: guard,
                ext: ext.clone(),
            }),
            inner,
        }
    }
}

impl ExtensionFactory for Logging {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(LoggingExt {
            session: Arc::new(OnceLock::new()),
            query: Arc::new(OnceLock::new()),
            metrics: self.0.clone(),
        })
    }
}

#[async_trait::async_trait]
impl Extension for LoggingExt {
    async fn request(&self, ctx: &ExtensionContext<'_>, next: NextRequest<'_>) -> Response {
        MetricsFuture::request(self, next.run(ctx)).await
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
        let _ = self.session.set(*session);
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
        let _ = self.query.set(query);
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

impl<F> Future for MetricsFuture<F>
where
    F: Future<Output = Response>,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        let Poll::Ready(mut resp) = this.inner.poll(cx) else {
            return Poll::Pending;
        };

        let Some(RequestMetrics { timer, ext }) = this.metrics.take() else {
            return Poll::Ready(resp);
        };

        let elapsed_ms = timer.stop_and_record() * 1000.0;
        ext.metrics.queries_in_flight.dec();

        // SAFETY: This is set by `prepare_request`.
        let Session { uuid, addr } = ext.session.get().unwrap();
        let request_id = uuid.to_string().try_into().unwrap();
        resp.http_headers.insert(REQUEST_ID_HEADER, request_id);

        if resp.is_ok() {
            info!(%uuid, %addr, elapsed_ms, "Request succeeded");
            ext.metrics.queries_succeeded.inc();
        } else {
            let codes = error_codes(&resp);

            // Log internal errors, timeouts, and unknown errors at a higher log level than other errors.
            if is_loud_query(&codes) {
                warn!(%uuid, %addr, query = ext.query.get().unwrap(), "Query");
            } else {
                debug!(%uuid, %addr, query = ext.query.get().unwrap(), "Query");
            }

            info!(%uuid, %addr, elapsed_ms, ?codes, "Request failed");

            if codes.is_empty() {
                ext.metrics
                    .queries_failed
                    .with_label_values(&["<UNKNOWN>"])
                    .inc();
            }

            for code in &codes {
                ext.metrics.queries_failed.with_label_values(&[code]).inc();
            }
        }

        debug!(%uuid, %addr, response = %json!(resp), "Response");
        Poll::Ready(resp)
    }
}

#[pinned_drop]
impl<F> PinnedDrop for MetricsFuture<F> {
    fn drop(self: Pin<&mut Self>) {
        if let Some(RequestMetrics { timer, ext }) = self.project().metrics.take() {
            let elapsed_ms = timer.stop_and_record() * 1000.0;
            ext.metrics.queries_cancelled.inc();
            info!(elapsed_ms, "Request cancelled");
        }
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
