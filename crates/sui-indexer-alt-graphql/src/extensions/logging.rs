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
use async_graphql::parser::types::DocumentOperations;
use async_graphql::parser::types::ExecutableDocument;
use async_graphql::parser::types::OperationType;
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
use tracing::trace;
use tracing::trace_span;
use tracing::warn;
use uuid::Uuid;

use crate::config::LoggingConfig;
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

/// Header identifying the SDK version that issued the request. The value is matched against
/// `LoggingConfig::sdk_version_allowlist` before being used as the `client_sdk_version`
/// Prometheus label or recorded in request log lines; versions outside the allowlist appear
/// as `CLIENT_LABEL_OTHER`.
const CLIENT_SDK_VERSION_HEADER: HeaderName = HeaderName::from_static("client-sdk-version");

/// SDKs we accept verbatim as the `client_sdk_type` Prometheus label.
const SDK_TYPE_WHITELIST: &[&str] = &["rust", "typescript", "python"];

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
    /// The client-selected operation name, stashed by `prepare_request` so `parse_query` can
    /// classify the operation that will execute (it is not reachable from `parse_query` otherwise).
    operation_name: Arc<OnceLock<Option<String>>>,
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
    pub(crate) fn from_headers(headers: &HeaderMap, config: &LoggingConfig) -> Self {
        let sdk_type = headers
            .get(&CLIENT_SDK_TYPE_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(sanitize_sdk_type);
        let sdk_version = headers
            .get(&CLIENT_SDK_VERSION_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|v| sanitize_sdk_version(sdk_type.as_deref().unwrap_or(""), v, config));
        Self {
            sdk_type,
            sdk_version,
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
            operation_name: Arc::new(OnceLock::new()),
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
    /// context, so `Session` is not yet readable at that point. The request's operation name is
    /// also stashed here so `parse_query` can classify the operation without re-parsing the query.
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
        let _ = self.operation_name.set(request.operation_name.clone());
        next.run(ctx, request).await
    }

    /// Check for parse errors and capture the request verbatim for replay. The framework parses the
    /// query exactly once here; we reuse the resulting document to classify the operation rather than
    /// re-parsing it.
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        // SAFETY: both are set by `prepare_request`, which the framework runs before `parse_query`
        // (and `Logging` is the outermost extension, so its hooks fire first).
        let uuid = self.session.get().unwrap().uuid;
        let operation_name = self.operation_name.get().unwrap().as_deref();

        let doc = match next.run(ctx, query, variables).await {
            Ok(doc) => doc,
            Err(mut err) => {
                // Capture verbatim even when the query fails to parse, so it can still be replayed.
                capture(uuid, query, variables, operation_name, None);
                fill_error_code(&mut err.extensions, code::GRAPHQL_PARSE_FAILED);
                return Err(err);
            }
        };

        capture(uuid, query, variables, operation_name, Some(&doc));

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

/// Sanitize a `client-sdk-version` header value before it is used as a Prometheus label.
/// Returns the value verbatim if `(sdk_type, value)` is listed in
/// `LoggingConfig::sdk_version_allowlist`, otherwise `CLIENT_LABEL_OTHER`. Bounding label
/// cardinality this way is what protects the metric against an adversarially-chosen version
/// string.
fn sanitize_sdk_version(sdk_type: &str, value: &str, config: &LoggingConfig) -> String {
    if config
        .sdk_version_allowlist
        .get(sdk_type)
        .is_some_and(|vs| vs.contains(value))
    {
        value.to_string()
    } else {
        CLIENT_LABEL_OTHER.to_string()
    }
}

/// Log this request's original payload (query, variables, operationName) as a single JSON object on
/// the `graphql_request` tracing target so it can be replayed later. Unlike the per-request logging
/// in `poll` (which records a reconstructed, variables-inlined query), this captures the request
/// verbatim. `doc` is the document the framework already parsed (`None` when the query failed to
/// parse), so the kind is derived without re-parsing.
///
/// Emitted at `trace` level, so it is a no-op unless an operator opts in by enabling the target.
/// The operation kind is recorded on the enclosing span, so an operator can capture every kind with
/// `RUST_LOG=graphql_request=trace`, or a single kind with an `EnvFilter` span-field directive, e.g.
/// `RUST_LOG="graphql_request[{kind=mutation}]=trace"`. Combine with `RUST_LOG_JSON=1` (and
/// optionally `RUST_LOG_FILE`) to emit newline-delimited JSON. Payloads may contain sensitive
/// arguments.
fn capture(
    uuid: Uuid,
    query: &str,
    variables: &Variables,
    operation_name: Option<&str>,
    doc: Option<&ExecutableDocument>,
) {
    // `kind` lives on the span (not the event) because `EnvFilter` can filter by span fields but not
    // by an event's own fields. It is a span-macro field expression, so it is computed only when the
    // span callsite is enabled (some `graphql_request` trace directive is active), never on the
    // default path where trace is statically disabled.
    let _span = trace_span!(
        target: "graphql_request",
        "capture",
        kind = doc
            .and_then(|doc| operation_kind(doc, operation_name))
            .unwrap_or("unknown"),
    )
    .entered();

    trace!(
        target: "graphql_request",
        request_id = %uuid,
        payload = %json!({
            "query": query,
            "variables": variables,
            "operationName": operation_name,
        }),
        "Captured request",
    );
}

/// Return the kind of the operation that will execute (`query`, `mutation`, or `subscription`) for
/// an already-parsed `doc`, selecting by `operation_name` for multi-operation documents. Returns
/// `None` if the operation cannot be resolved. Reads the type straight off the parsed AST, so it
/// performs no re-parsing.
fn operation_kind(doc: &ExecutableDocument, operation_name: Option<&str>) -> Option<&'static str> {
    let op = match (&doc.operations, operation_name) {
        (DocumentOperations::Single(op), _) => op,
        (DocumentOperations::Multiple(ops), Some(name)) => ops.get(name)?,
        // A single named operation executes without an explicit operation name; only genuinely
        // ambiguous documents (multiple operations, none selected) cannot be classified.
        (DocumentOperations::Multiple(ops), None) if ops.len() == 1 => ops.values().next()?,
        (DocumentOperations::Multiple(_), None) => return None,
    };
    Some(match op.node.ty {
        OperationType::Query => "query",
        OperationType::Mutation => "mutation",
        OperationType::Subscription => "subscription",
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    use async_graphql::EmptyMutation;
    use async_graphql::EmptySubscription;
    use async_graphql::Object;
    use async_graphql::Schema;
    use async_graphql::parser::parse_query;
    use axum::http::HeaderValue;
    use prometheus::Registry;
    use tracing::field::Field;
    use tracing::field::Visit;
    use tracing::subscriber::with_default;
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::Context;
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

    struct Query;

    #[Object]
    impl Query {
        async fn op(&self) -> bool {
            true
        }
    }

    /// A `tracing` layer that records the `payload` field of every event it receives, so tests can
    /// assert which `capture` calls survived an `EnvFilter`.
    #[derive(Clone, Default)]
    struct CaptureLayer {
        payloads: Arc<Mutex<Vec<String>>>,
    }

    impl<S: tracing::Subscriber> Layer<S> for CaptureLayer {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            let mut visitor = PayloadVisitor(None);
            event.record(&mut visitor);
            if let Some(payload) = visitor.0 {
                self.payloads.lock().unwrap().push(payload);
            }
        }
    }

    struct PayloadVisitor(Option<String>);

    impl Visit for PayloadVisitor {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            if field.name() == "payload" {
                self.0 = Some(format!("{value:?}"));
            }
        }
    }

    /// Run `capture` once per operation kind under a subscriber configured with `directive`, and
    /// return the payloads that were actually logged (i.e. passed the filter).
    fn capture_with_filter(directive: &str) -> Vec<String> {
        let layer = CaptureLayer::default();
        let subscriber = tracing_subscriber::registry()
            .with(EnvFilter::new(directive))
            .with(layer.clone());

        with_default(subscriber, || {
            let variables = Variables::default();
            for query in [
                "query Q { op }",
                "mutation M { op }",
                "subscription S { op }",
            ] {
                let doc = parse_query(query).unwrap();
                capture(Uuid::nil(), query, &variables, None, Some(&doc));
            }
        });

        let payloads = layer.payloads.lock().unwrap();
        payloads.clone()
    }

    #[test]
    fn client_info_missing_headers_yield_none() {
        let info = ClientInfo::from_headers(&HeaderMap::new(), &LoggingConfig::default());

        assert!(info.sdk_type.is_none());
        assert!(info.sdk_version.is_none());
    }

    #[test]
    fn client_info_non_allowlisted_version_bucketed_other() {
        let mut headers = HeaderMap::new();
        headers.insert(CLIENT_SDK_TYPE_HEADER, HeaderValue::from_static("rust"));
        headers.insert(
            CLIENT_SDK_VERSION_HEADER,
            HeaderValue::from_static("1.69.0"),
        );

        let info = ClientInfo::from_headers(&headers, &LoggingConfig::default());

        assert_eq!(info.sdk_type.as_deref(), Some("rust"));
        assert_eq!(info.sdk_version.as_deref(), Some(CLIENT_LABEL_OTHER));
    }

    #[test]
    fn client_info_unknown_sdk_type_bucketed_other() {
        let mut headers = HeaderMap::new();
        headers.insert(CLIENT_SDK_TYPE_HEADER, HeaderValue::from_static("haskell"));
        headers.insert(CLIENT_SDK_VERSION_HEADER, HeaderValue::from_static("0.1.0"));

        let info = ClientInfo::from_headers(&headers, &LoggingConfig::default());

        assert_eq!(info.sdk_type.as_deref(), Some(CLIENT_LABEL_OTHER));
        assert_eq!(info.sdk_version.as_deref(), Some(CLIENT_LABEL_OTHER));
    }

    #[test]
    fn client_info_version_header_missing_yields_none() {
        let mut headers = HeaderMap::new();
        headers.insert(CLIENT_SDK_TYPE_HEADER, HeaderValue::from_static("rust"));

        let info = ClientInfo::from_headers(&headers, &LoggingConfig::default());

        assert_eq!(info.sdk_type.as_deref(), Some("rust"));
        assert!(info.sdk_version.is_none());
    }

    #[test]
    fn client_info_allowlisted_version_kept_verbatim() {
        let mut headers = HeaderMap::new();
        headers.insert(CLIENT_SDK_TYPE_HEADER, HeaderValue::from_static("rust"));
        headers.insert(
            CLIENT_SDK_VERSION_HEADER,
            HeaderValue::from_static("1.69.0"),
        );

        let config = LoggingConfig {
            sdk_version_allowlist: BTreeMap::from([(
                "rust".to_string(),
                BTreeSet::from(["1.69.0".to_string()]),
            )]),
        };

        let info = ClientInfo::from_headers(&headers, &config);

        assert_eq!(info.sdk_type.as_deref(), Some("rust"));
        assert_eq!(info.sdk_version.as_deref(), Some("1.69.0"));
    }

    #[test]
    fn operation_kind_classifies_operations() {
        assert_eq!(
            operation_kind(&parse_query("{ op }").unwrap(), None),
            Some("query"),
        );
        assert_eq!(
            operation_kind(&parse_query("mutation M { op }").unwrap(), None),
            Some("mutation"),
        );
        assert_eq!(
            operation_kind(&parse_query("subscription S { op }").unwrap(), None),
            Some("subscription"),
        );
    }

    #[test]
    fn operation_kind_selects_by_operation_name() {
        let doc = parse_query("query Q { op } mutation M { op }").unwrap();
        assert_eq!(operation_kind(&doc, Some("M")), Some("mutation"));
        assert_eq!(operation_kind(&doc, Some("Q")), Some("query"));

        // Ambiguous (multiple operations, no name) cannot be classified.
        assert_eq!(operation_kind(&doc, None), None);

        // Unparseable queries never reach `operation_kind`; they fail earlier, at `parse_query`.
        assert!(parse_query("{ op").is_err());
    }

    #[test]
    fn rust_log_captures_only_configured_operation_kind() {
        let payloads = capture_with_filter("graphql_request[{kind=mutation}]=trace");
        assert_eq!(payloads.len(), 1, "only the mutation should be captured");
        assert!(
            payloads[0].contains("mutation M"),
            "captured the wrong operation: {:?}",
            payloads[0],
        );
    }

    #[test]
    fn rust_log_captures_all_operation_kinds() {
        let payloads = capture_with_filter("graphql_request=trace");
        assert_eq!(payloads.len(), 3);
    }

    #[test]
    fn rust_log_disabled_captures_nothing() {
        let payloads = capture_with_filter("info");
        assert!(payloads.is_empty());
    }

    #[test]
    fn parse_failure_captured_as_unknown() {
        // A request that fails to parse has no document, so it is captured verbatim and classified
        // as `unknown`. Filtering on `kind=unknown` confirms both that capture fired and the kind.
        let layer = CaptureLayer::default();
        let subscriber = tracing_subscriber::registry()
            .with(EnvFilter::new("graphql_request[{kind=unknown}]=trace"))
            .with(layer.clone());

        with_default(subscriber, || {
            let variables = Variables::default();
            capture(Uuid::nil(), "{ op", &variables, None, None);
        });

        let payloads = layer.payloads.lock().unwrap();
        assert_eq!(
            payloads.len(),
            1,
            "the unparseable request should be captured"
        );
        assert!(payloads[0].contains("{ op"));
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
