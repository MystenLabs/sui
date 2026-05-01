// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::OnceLock;
use std::task::Context;
use std::task::Poll;

use async_graphql::Request;
use async_graphql::Response;
use async_graphql::ServerError;
use async_graphql::ServerResult;
use async_graphql::ValidationResult;
use async_graphql::Value;
use async_graphql::Variables;
use async_graphql::extensions::Extension;
use async_graphql::extensions::ExtensionContext;
use async_graphql::extensions::ExtensionFactory;
use async_graphql::extensions::NextParseQuery;
use async_graphql::extensions::NextPrepareRequest;
use async_graphql::extensions::NextRequest;
use async_graphql::extensions::NextResolve;
use async_graphql::extensions::NextValidation;
use async_graphql::extensions::ResolveInfo;
use async_graphql::parser::types::ExecutableDocument;
use axum::http::HeaderMap;
use axum::http::HeaderName;
use pin_project::pin_project;
use pin_project::pinned_drop;
use prometheus::HistogramTimer;
use serde_json::json;
use tracing::debug;
use tracing::info;
use tracing::log::Level;
use tracing::log::log_enabled;
use tracing::warn;
use uuid::Uuid;

use crate::error::code;
use crate::error::error_codes;
use crate::error::fill_error_code;
use crate::metrics::RpcMetrics;

/// This custom response header contains a unique request-id used for debugging and appears in the logs.
pub const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-sui-rpc-request-id");

/// Header identifying the SDK that issued the request. The value flows into the
/// `client_sdk_type` Prometheus label, matched against `SDK_TYPE_WHITELIST`. Add a new entry to
/// the whitelist when a new first-party SDK adopts this header, otherwise its requests will be
/// bucketed into `CLIENT_LABEL_OTHER`.
const CLIENT_SDK_TYPE_HEADER: HeaderName = HeaderName::from_static("client-sdk-type");

/// Header identifying the SDK version that issued the request. The value flows into the
/// `client_sdk_version` Prometheus label, sanitized by `sanitize_sdk_version`. Values longer
/// than `MAX_SDK_VERSION_LEN` or containing characters outside `[A-Za-z0-9._-]` are bucketed
/// into `CLIENT_LABEL_OTHER`.
const CLIENT_SDK_VERSION_HEADER: HeaderName = HeaderName::from_static("client-sdk-version");

/// SDKs we accept verbatim as the `client_sdk_type` Prometheus label.
const SDK_TYPE_WHITELIST: &[&str] = &["rust", "typescript", "python"];

/// Maximum length of a `client-sdk-version` value we accept verbatim.
const MAX_SDK_VERSION_LEN: usize = 32;

/// Sentinel label value substituted when a client SDK header is present but does not match an
/// allowed value. Distinct from the empty string we use when the header is absent, so dashboards
/// can tell the two apart.
const CLIENT_LABEL_OTHER: &str = "other";

/// Context data that tracks the session UUID and the client's address, to associate logs with a
/// particular request.
#[derive(Clone)]
pub(crate) struct Session {
    pub uuid: Uuid,
    pub addr: SocketAddr,
    pub client: ClientInfo,
}

/// Information about the SDK that issued the request, extracted from request headers. Lives on
/// `Session` so it travels with the request through extensions, metrics, and logs.
#[derive(Clone, Default)]
pub(crate) struct ClientInfo {
    pub sdk_type: Option<String>,
    pub sdk_version: Option<String>,
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
            client: ClientInfo::default(),
        }
    }

    /// Builder used at the HTTP entry point to attach client identifiers extracted from request
    /// headers. All other code paths keep the default (empty) `ClientInfo`.
    pub(crate) fn with_client_info(mut self, client: ClientInfo) -> Self {
        self.client = client;
        self
    }
}

impl ClientInfo {
    pub(crate) fn from_headers(headers: &HeaderMap) -> Self {
        Self {
            sdk_type: headers
                .get(&CLIENT_SDK_TYPE_HEADER)
                .and_then(|v| v.to_str().ok())
                .map(sanitize_sdk_type),
            sdk_version: headers
                .get(&CLIENT_SDK_VERSION_HEADER)
                .and_then(|v| v.to_str().ok())
                .map(sanitize_sdk_version),
        }
    }
}

impl<F> MetricsFuture<F> {
    fn request(ext: &LoggingExt, inner: F) -> Self
    where
        F: Future<Output = Response>,
    {
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
    /// logging, once it has finished executing. The labeled `queries_received` counter is also
    /// incremented here because `request` is called before request-level data is merged into the
    /// context, so `Session` is not yet readable at that point.
    async fn prepare_request(
        &self,
        ctx: &ExtensionContext<'_>,
        request: Request,
        next: NextPrepareRequest<'_>,
    ) -> ServerResult<Request> {
        let session: &Session = ctx.data_unchecked();
        let client_sdk_type = session.client.sdk_type.as_deref().unwrap_or("");
        let client_sdk_version = session.client.sdk_version.as_deref().unwrap_or("");
        self.metrics
            .queries_received
            .with_label_values(&[client_sdk_type, client_sdk_version])
            .inc();
        let _ = self.session.set(session.clone());
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
        let Session { uuid, addr, client } = ext.session.get().unwrap();
        let client_sdk_type = client.sdk_type.as_deref().unwrap_or("");
        let client_sdk_version = client.sdk_version.as_deref().unwrap_or("");
        let request_id = uuid.to_string().try_into().unwrap();
        resp.http_headers.insert(REQUEST_ID_HEADER, request_id);

        if resp.is_ok() {
            if log_enabled!(Level::Debug) {
                debug!(request_id = %uuid, %addr, elapsed_ms, client_sdk_type, client_sdk_version, query = ext.query.get().unwrap(), response = %json!(resp), "Request succeeded");
            } else {
                info!(request_id = %uuid, %addr, elapsed_ms, client_sdk_type, client_sdk_version, "Request succeeded");
            }
            ext.metrics.queries_succeeded.inc();
        } else {
            let codes = error_codes(&resp);

            ext.metrics.queries_failed.inc();

            // Log internal errors, timeouts, and unknown errors at a higher log level than other errors.
            if is_loud_query(&codes) {
                warn!(request_id = %uuid, %addr, elapsed_ms, client_sdk_type, client_sdk_version, query = ext.query.get().unwrap(), response = %json!(resp), "Request failed");
            } else if log_enabled!(Level::Debug) {
                debug!(request_id = %uuid, %addr, elapsed_ms, client_sdk_type, client_sdk_version, query = ext.query.get().unwrap(), response = %json!(resp), "Request failed");
            } else {
                info!(request_id = %uuid, %addr, elapsed_ms, client_sdk_type, client_sdk_version, ?codes, "Request failed");
            }

            if codes.is_empty() {
                ext.metrics
                    .query_errors
                    .with_label_values(&["<UNKNOWN>"])
                    .inc();
            }

            for code in &codes {
                ext.metrics.query_errors.with_label_values(&[code]).inc();
            }
        }

        Poll::Ready(resp)
    }
}

#[pinned_drop]
impl<F> PinnedDrop for MetricsFuture<F> {
    fn drop(self: Pin<&mut Self>) {
        if let Some(RequestMetrics { timer, ext }) = self.project().metrics.take() {
            let Session { uuid, addr, client } = ext.session.get().unwrap();
            let client_sdk_type = client.sdk_type.as_deref().unwrap_or("");
            let client_sdk_version = client.sdk_version.as_deref().unwrap_or("");
            let elapsed_ms = timer.stop_and_record() * 1000.0;
            ext.metrics.queries_cancelled.inc();
            info!(%uuid, %addr, elapsed_ms, client_sdk_type, client_sdk_version, "Request cancelled");
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

/// Sanitize a `client-sdk-type` header value before it is used as a Prometheus label. Values
/// outside `SDK_TYPE_WHITELIST` are bucketed to `CLIENT_LABEL_OTHER` so the metric's cardinality
/// is bounded by the size of the whitelist.
fn sanitize_sdk_type(value: &str) -> String {
    if SDK_TYPE_WHITELIST.contains(&value) {
        value.to_string()
    } else {
        CLIENT_LABEL_OTHER.to_string()
    }
}

/// Sanitize a `client-sdk-version` header value before it is used as a Prometheus label. Values
/// longer than `MAX_SDK_VERSION_LEN` or containing characters outside `[A-Za-z0-9._-]` are
/// bucketed to `CLIENT_LABEL_OTHER`. Build metadata (`+...`) is rejected on purpose, because
/// per-build suffixes are exactly the cardinality vector we are trying to avoid.
fn sanitize_sdk_version(value: &str) -> String {
    let valid = !value.is_empty()
        && value.len() <= MAX_SDK_VERSION_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'));

    if valid {
        value.to_string()
    } else {
        CLIENT_LABEL_OTHER.to_string()
    }
}

#[cfg(test)]
mod tests {
    use async_graphql::EmptyMutation;
    use async_graphql::EmptySubscription;
    use async_graphql::Object;
    use async_graphql::Schema;
    use axum::http::HeaderValue;
    use prometheus::Registry;

    use super::*;

    struct Query;

    #[Object]
    impl Query {
        async fn op(&self) -> bool {
            true
        }
    }

    #[test]
    fn client_info_extracts_sdk_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(CLIENT_SDK_TYPE_HEADER, HeaderValue::from_static("rust"));
        headers.insert(
            CLIENT_SDK_VERSION_HEADER,
            HeaderValue::from_static("1.69.0"),
        );

        let info = ClientInfo::from_headers(&headers);

        assert_eq!(info.sdk_type.as_deref(), Some("rust"));
        assert_eq!(info.sdk_version.as_deref(), Some("1.69.0"));
    }

    #[test]
    fn client_info_missing_headers_yield_none() {
        let info = ClientInfo::from_headers(&HeaderMap::new());

        assert!(info.sdk_type.is_none());
        assert!(info.sdk_version.is_none());
    }

    #[test]
    fn client_info_unknown_sdk_type_bucketed_other() {
        let mut headers = HeaderMap::new();
        headers.insert(CLIENT_SDK_TYPE_HEADER, HeaderValue::from_static("haskell"));
        headers.insert(CLIENT_SDK_VERSION_HEADER, HeaderValue::from_static("0.1.0"));

        let info = ClientInfo::from_headers(&headers);

        assert_eq!(info.sdk_type.as_deref(), Some(CLIENT_LABEL_OTHER));
        assert_eq!(info.sdk_version.as_deref(), Some("0.1.0"));
    }

    #[test]
    fn client_info_oversized_version_bucketed_other() {
        let mut headers = HeaderMap::new();
        let too_long = "1.".to_string() + &"0".repeat(MAX_SDK_VERSION_LEN);
        headers.insert(CLIENT_SDK_TYPE_HEADER, HeaderValue::from_static("rust"));
        headers.insert(
            CLIENT_SDK_VERSION_HEADER,
            HeaderValue::from_str(&too_long).unwrap(),
        );

        let info = ClientInfo::from_headers(&headers);

        assert_eq!(info.sdk_type.as_deref(), Some("rust"));
        assert_eq!(info.sdk_version.as_deref(), Some(CLIENT_LABEL_OTHER));
    }

    #[test]
    fn client_info_version_with_build_metadata_bucketed_other() {
        let mut headers = HeaderMap::new();
        headers.insert(CLIENT_SDK_TYPE_HEADER, HeaderValue::from_static("rust"));
        headers.insert(
            CLIENT_SDK_VERSION_HEADER,
            HeaderValue::from_static("1.0.0+abc123"),
        );

        let info = ClientInfo::from_headers(&headers);

        assert_eq!(info.sdk_version.as_deref(), Some(CLIENT_LABEL_OTHER));
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
        assert_eq!(
            metrics.queries_received.with_label_values(&["", ""]).get(),
            1
        );
        assert_eq!(
            metrics
                .query_errors
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
        assert_eq!(
            metrics.queries_received.with_label_values(&["", ""]).get(),
            1
        );
        assert_eq!(
            metrics
                .query_errors
                .with_label_values(&[code::GRAPHQL_VALIDATION_FAILED])
                .get(),
            1
        );
    }

    #[tokio::test]
    async fn multiple_error_codes_single_request_failed() {
        let registry = Registry::new();
        let metrics = RpcMetrics::new(&registry);

        let request = Request::from("{ undefined1 undefined2 undefined3 }")
            .data(Session::new("0.0.0.0:0".parse().unwrap()));

        let response = Schema::build(Query, EmptyMutation, EmptySubscription)
            .extension(Logging(metrics.clone()))
            .finish()
            .execute(request)
            .await;

        assert!(response.is_err());

        // Should have multiple errors
        let codes = error_codes(&response);
        assert_eq!(codes.len(), 3);

        assert_eq!(
            metrics.queries_received.with_label_values(&["", ""]).get(),
            1
        );
        assert_eq!(metrics.queries_failed.get(), 1);
        assert_eq!(
            metrics
                .query_errors
                .with_label_values(&[code::GRAPHQL_VALIDATION_FAILED])
                .get(),
            3,
        );
    }
}
