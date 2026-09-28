// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::http;
use std::{
    borrow::Cow,
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use prometheus::{
    Histogram, HistogramTimer, HistogramVec, IntCounter, IntCounterVec, IntGauge, IntGaugeVec,
    Registry, register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry,
};
use prost::Message;
use sui_http::middleware::callback::{MakeCallbackHandler, RequestHandler, ResponseHandler};

#[derive(Clone)]
pub struct RpcMetrics {
    inflight_requests: IntGaugeVec,
    num_requests: IntCounterVec,
    request_latency: HistogramVec,
    request_handler_latency: HistogramVec,
    first_chunk_latency: HistogramVec,
    simulate_client_protocol_versions: IntCounterVec,
}

const GRPC_STATUS: http::HeaderName = http::HeaderName::from_static("grpc-status");
const GRPC_WEB_CONTENT_TYPE: &str = "application/grpc-web";

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
];

impl RpcMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            inflight_requests: register_int_gauge_vec_with_registry!(
                "rpc_inflight_requests",
                "Total in-flight RPC requests per route",
                &["path"],
                registry,
            )
            .unwrap(),
            num_requests: register_int_counter_vec_with_registry!(
                "rpc_requests",
                "Total RPC requests per route and their http status",
                &["path", "status"],
                registry,
            )
            .unwrap(),
            request_latency: register_histogram_vec_with_registry!(
                "rpc_request_latency",
                "Latency of RPC requests per route, measured from receipt of the request \
                 until the response body finished streaming back to the client",
                &["path"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            request_handler_latency: register_histogram_vec_with_registry!(
                "rpc_request_handler_latency",
                "Latency of RPC requests per route, measured from receipt of the request \
                 until the request handler produced a response, excluding the time spent \
                 streaming the response body back to the client",
                &["path"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            first_chunk_latency: register_histogram_vec_with_registry!(
                "rpc_first_chunk_latency",
                "Latency of RPC requests per route, measured from receipt of the request \
                 until the first response body data chunk is produced. For streaming responses \
                 this is when the first chunk is handed to the transport, which for gRPC \
                 typically carries the first encoded message; response headers are excluded. \
                 Responses whose body never yields a data chunk are not observed.",
                &["path"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            simulate_client_protocol_versions: register_int_counter_vec_with_registry!(
                "rpc_simulate_transaction_client_protocol_version",
                "SimulateTransaction requests by the client's reported x-sui-client-protocol-version; \
                 0 when the header is missing, malformed, or more than 20 above this binary's max",
                &["client_protocol_version"],
                registry,
            )
            .unwrap(),
        }
    }

    pub(crate) fn observe_simulate_client_protocol_version(&self, version: u64) {
        self.simulate_client_protocol_versions
            .with_label_values(&[&version.to_string()])
            .inc();
    }
}

#[derive(Clone)]
pub(crate) struct ListApiMetrics {
    list_first_frame_seconds: HistogramVec,
    list_response_page_bytes: HistogramVec,
    list_watermark_frames_total: IntCounterVec,
    list_stream_yield_wait_seconds: HistogramVec,
    list_render_seconds: HistogramVec,
    list_chunk_seconds: HistogramVec,
    list_query_ends_total: IntCounterVec,
    list_bitmap_buckets_evaluated: HistogramVec,
}

impl ListApiMetrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        Self {
            list_first_frame_seconds: register_histogram_vec_with_registry!(
                "list_first_frame_seconds",
                "Time in seconds from List handler entry to the first response frame of any kind — data, watermark-only, or terminal — the client's first actionable signal; resolution label derived only from the validated read mask.",
                &["method", "resolution"],
                prometheus::exponential_buckets(0.001, 2.0, 17).unwrap(),
                registry,
            )
            .unwrap(),
            list_response_page_bytes: register_histogram_vec_with_registry!(
                "list_response_page_bytes",
                "Protobuf encoded size in bytes of data-bearing List response frames, measured with encoded_len without serializing or copying; watermark-only and terminal-only frames are excluded and counted by list_watermark_frames_total.",
                &["method", "resolution"],
                prometheus::exponential_buckets(1024.0, 2.0, 17).unwrap(),
                registry,
            )
            .unwrap(),
            list_watermark_frames_total: register_int_counter_vec_with_registry!(
                "list_watermark_frames_total",
                "Total watermark-only and terminal-only response frames emitted by List handlers; data-bearing frames are excluded.",
                &["method"],
                registry,
            )
            .unwrap(),
            list_stream_yield_wait_seconds: register_histogram_vec_with_registry!(
                "list_stream_yield_wait_seconds",
                "Time from yielding one List response frame (any kind) until the handler stream is polled again; downstream transport consumption and backpressure signal.",
                &["method", "resolution"],
                prometheus::exponential_buckets(0.00001, 2.0, 24).unwrap(),
                registry,
            )
            .unwrap(),
            list_render_seconds: register_histogram_vec_with_registry!(
                "list_render_seconds",
                "Time in seconds spent rendering one data item into a List response frame. Request setup, scan-only watermark rendering, standalone terminal rendering, and chunk-level batch reads are excluded. The resolution label is derived only from the validated read mask.",
                &["method", "resolution"],
                prometheus::exponential_buckets(0.00001, 2.0, 24).unwrap(),
                registry,
            )
            .unwrap(),
            list_chunk_seconds: register_histogram_vec_with_registry!(
                "list_chunk_seconds",
                "Time in seconds for one blocking List chunk phase. queue spans immediately before spawn_blocking through entry into its closure; work spans execution of the blocking chunk and includes chunks that return an error.",
                &["method", "phase"],
                prometheus::exponential_buckets(0.0001, 2.0, 20).unwrap(),
                registry,
            )
            .unwrap(),
            list_query_ends_total: register_int_counter_vec_with_registry!(
                "list_query_ends_total",
                "Successful List streams by effective protocol QueryEndReason. Errors, cancellation, and dropped streams are excluded.",
                &["method", "reason"],
                registry,
            )
            .unwrap(),
            list_bitmap_buckets_evaluated: register_histogram_vec_with_registry!(
                "list_bitmap_buckets_evaluated",
                "Total bitmap buckets evaluated across all blocking chunks of one successfully completed filtered List request. Unfiltered requests are not observed.",
                &["method"],
                prometheus::exponential_buckets(1.0, 2.0, 12).unwrap(),
                registry,
            )
            .unwrap(),
        }
    }

    pub(crate) fn stream_metrics(
        &self,
        method: &'static str,
        resolution: &'static str,
    ) -> ListStreamMetrics {
        ListStreamMetrics {
            method,
            first_frame: self
                .list_first_frame_seconds
                .with_label_values(&[method, resolution]),
            page_bytes: self
                .list_response_page_bytes
                .with_label_values(&[method, resolution]),
            watermark_frames: self
                .list_watermark_frames_total
                .with_label_values(&[method]),
            yield_wait: self
                .list_stream_yield_wait_seconds
                .with_label_values(&[method, resolution]),
            render: self
                .list_render_seconds
                .with_label_values(&[method, resolution]),
            chunk_queue: self
                .list_chunk_seconds
                .with_label_values(&[method, "queue"]),
            chunk_work: self.list_chunk_seconds.with_label_values(&[method, "work"]),
            query_ends: self.list_query_ends_total.clone(),
            bitmap_buckets_evaluated: self
                .list_bitmap_buckets_evaluated
                .with_label_values(&[method]),
        }
    }
}

/// Set of `/package.Service/Method` paths that are safe to use as metric
/// labels.
///
/// Services are mounted with the wildcard route `/{ServiceName}/{*rest}`, so
/// any path under a registered prefix matches a route and would otherwise be
/// taken verbatim as a `path` label. Bounding the labels to known methods
/// prevents an unauthenticated attacker from inflating Prometheus label maps
/// (which the prometheus crate retains for the lifetime of the process) by
/// streaming requests with random method suffixes.
pub type GrpcMethodAllowlist = Arc<HashSet<String>>;

/// Decode one or more encoded `FileDescriptorSet` byte slices and return the
/// set of `/package.Service/Method` paths they declare.
///
/// Intended to be called once at server startup with the same bytes that are
/// registered with `tonic_reflection`, so the metrics allowlist stays in sync
/// with the services actually exposed over gRPC.
pub fn grpc_method_paths_from_file_descriptor_sets(
    encoded_sets: &[&[u8]],
) -> Result<HashSet<String>, prost::DecodeError> {
    let mut paths = HashSet::new();
    for bytes in encoded_sets {
        let fds = prost_types::FileDescriptorSet::decode(*bytes)?;
        for file in fds.file {
            let package = file.package.unwrap_or_default();
            for service in file.service {
                let Some(service_name) = service.name else {
                    continue;
                };
                let qualified_service = if package.is_empty() {
                    service_name
                } else {
                    format!("{}.{}", package, service_name)
                };
                for method in service.method {
                    let Some(method_name) = method.name else {
                        continue;
                    };
                    paths.insert(format!("/{}/{}", qualified_service, method_name));
                }
            }
        }
    }
    Ok(paths)
}

/// Builds the per-request metrics handlers for [`sui_http::middleware::callback::CallbackLayer`].
///
/// The layer must be installed outside any layer that wraps the request body
/// with its own failure modes (body-size limits, request decompression,
/// body-read timeouts). A request whose body errors is counted `canceled`,
/// which is only correct while the body this layer observes is the
/// transport's: then an error means the client reset the stream, the
/// connection failed, or the client broke the protocol. Errors from an outer
/// body-wrapping layer would be counted `canceled` and hide real rejections.
#[derive(Clone)]
pub struct RpcMetricsMakeCallbackHandler {
    metrics: Arc<RpcMetrics>,
    grpc_method_allowlist: GrpcMethodAllowlist,
}

impl RpcMetricsMakeCallbackHandler {
    /// Construct a handler with no gRPC method allowlist. All gRPC requests
    /// will be labelled with their matched route pattern (e.g.
    /// `/sui.rpc.v2.LedgerService/{*rest}`) rather than the per-method path,
    /// which is safe but loses per-method granularity.
    pub fn new(metrics: Arc<RpcMetrics>) -> Self {
        Self::with_grpc_method_allowlist(metrics, Arc::new(HashSet::new()))
    }

    /// Construct a handler that uses `allowlist` to decide which gRPC request
    /// paths are safe to emit as Prometheus labels.
    pub fn with_grpc_method_allowlist(
        metrics: Arc<RpcMetrics>,
        allowlist: GrpcMethodAllowlist,
    ) -> Self {
        Self {
            metrics,
            grpc_method_allowlist: allowlist,
        }
    }
}

impl MakeCallbackHandler for RpcMetricsMakeCallbackHandler {
    type RequestHandler = RpcMetricsRequestHandler;
    type ResponseHandler = RpcMetricsCallbackHandler;

    fn make_handler(
        &self,
        request: &http::request::Parts,
    ) -> (Self::RequestHandler, Self::ResponseHandler) {
        let start = Instant::now();
        let metrics = self.metrics.clone();

        let matched_path = request
            .extensions
            .get::<axum::extract::MatchedPath>()
            .map(|m| m.as_str());
        let is_grpc = request
            .headers
            .get(&http::header::CONTENT_TYPE)
            .is_some_and(is_grpc_content_type);

        let path = compute_metric_label(
            is_grpc,
            request.uri.path(),
            matched_path,
            &self.grpc_method_allowlist,
        );

        metrics
            .inflight_requests
            .with_label_values(&[path.as_ref()])
            .inc();

        let request_body_failed = Arc::new(AtomicBool::new(false));

        (
            RpcMetricsRequestHandler {
                request_body_failed: request_body_failed.clone(),
            },
            RpcMetricsCallbackHandler {
                metrics,
                path,
                start,
                request_body_failed,
                counting: CountingState::AwaitingResponse,
                counted_first_chunk: false,
            },
        )
    }
}

/// Decide which string to use as the `path` Prometheus label for a request.
///
/// For gRPC traffic, prefer the per-method URI path when it is in the
/// allowlist; otherwise fall back to the matched route pattern so unknown
/// methods collapse into a single bounded series per service. For non-gRPC
/// traffic the matched path is already bounded by the routes registered on
/// the router, so it is used directly.
fn compute_metric_label(
    is_grpc: bool,
    uri_path: &str,
    matched_path: Option<&str>,
    grpc_method_allowlist: &HashSet<String>,
) -> Cow<'static, str> {
    match (is_grpc, matched_path) {
        (true, _) if grpc_method_allowlist.contains(uri_path) => Cow::Owned(uri_path.to_owned()),
        (true, Some(matched)) => Cow::Owned(matched.to_owned()),
        (false, Some(matched)) => Cow::Owned(matched.to_owned()),
        (_, None) => Cow::Borrowed("unknown"),
    }
}

fn is_grpc_content_type(content_type: &http::HeaderValue) -> bool {
    content_type
        .as_bytes()
        .starts_with(tonic::metadata::GRPC_CONTENT_TYPE.as_bytes())
}

fn is_grpc_web_content_type(content_type: &http::HeaderValue) -> bool {
    content_type
        .as_bytes()
        .starts_with(GRPC_WEB_CONTENT_TYPE.as_bytes())
}

/// Observes the request body so that a request the client abandoned is
/// counted as `canceled` rather than with whatever status the service
/// produced for the truncated request. This relies on the placement described
/// on [`RpcMetricsMakeCallbackHandler`].
pub struct RpcMetricsRequestHandler {
    request_body_failed: Arc<AtomicBool>,
}

impl RequestHandler for RpcMetricsRequestHandler {
    fn on_body_error<E>(&mut self, _error: &E)
    where
        E: std::fmt::Display + 'static,
    {
        self.request_body_failed.store(true, Ordering::Release);
    }
}

/// Progress of a request towards its single `rpc_requests` increment.
enum CountingState {
    /// The service has not produced a response yet.
    AwaitingResponse,
    /// The response is a native gRPC stream whose status arrives in the
    /// trailers that end the response body.
    AwaitingTrailers,
    Counted,
}

pub struct RpcMetricsCallbackHandler {
    metrics: Arc<RpcMetrics>,
    path: Cow<'static, str>,
    start: Instant,
    request_body_failed: Arc<AtomicBool>,
    // Requests that end before reaching `Counted` (the service future or the
    // response body is dropped first) are counted as `canceled` on drop.
    counting: CountingState,
    counted_first_chunk: bool,
}

impl RpcMetricsCallbackHandler {
    fn count(&mut self, status: &str) {
        // A request body that errors means the client reset the stream or the
        // connection failed while the request was arriving, so no response
        // reaches the client. The service's answer to the truncated request is
        // not the outcome: tonic, for example, answers a request whose message
        // never arrived with `internal`.
        let status = if self.request_body_failed.load(Ordering::Acquire) {
            "canceled"
        } else {
            status
        };

        self.metrics
            .num_requests
            .with_label_values(&[self.path.as_ref(), status])
            .inc();

        self.counting = CountingState::Counted;
    }
}

impl ResponseHandler for RpcMetricsCallbackHandler {
    fn on_response(&mut self, response: &http::response::Parts) {
        // Unlike `request_latency` (observed in `Drop`, after the response
        // body finished streaming), this fires as soon as the handler
        // produced a response, so it excludes client-side network latency.
        self.metrics
            .request_handler_latency
            .with_label_values(&[self.path.as_ref()])
            .observe(self.start.elapsed().as_secs_f64());

        let content_type = response.headers.get(&http::header::CONTENT_TYPE);
        let status = if content_type.is_some_and(is_grpc_content_type) {
            match response.headers.get(&GRPC_STATUS) {
                // Trailers-only response: the status is final, and the empty
                // body may never be polled.
                Some(grpc_status) => code_as_str(tonic::Code::from_bytes(grpc_status.as_bytes())),
                // grpc-web encodes the trailers into the response body, which
                // is not parsed here.
                None if content_type.is_some_and(is_grpc_web_content_type) => {
                    code_as_str(tonic::Code::Ok)
                }
                None => {
                    self.counting = CountingState::AwaitingTrailers;
                    return;
                }
            }
        } else {
            response.status.as_str()
        };

        self.count(status);
    }

    fn on_body_chunk<B>(&mut self, _chunk: &B)
    where
        B: bytes::Buf,
    {
        if !self.counted_first_chunk {
            self.metrics
                .first_chunk_latency
                .with_label_values(&[self.path.as_ref()])
                .observe(self.start.elapsed().as_secs_f64());
            self.counted_first_chunk = true;
        }
    }

    fn on_end_of_stream(&mut self, trailers: Option<&http::HeaderMap>) {
        if let CountingState::AwaitingTrailers = self.counting {
            // gRPC requires `grpc-status` in the trailers; clients report a
            // stream that ends without it as `unknown`.
            let code = trailers
                .and_then(|trailers| trailers.get(&GRPC_STATUS))
                .map(|grpc_status| tonic::Code::from_bytes(grpc_status.as_bytes()))
                .unwrap_or(tonic::Code::Unknown);

            self.count(code_as_str(code));
        }
    }

    fn on_body_error<E>(&mut self, _error: &E)
    where
        E: std::fmt::Display + 'static,
    {
        // The stream ends without trailers: hyper resets it (with
        // `INTERNAL_ERROR` unless the error carries an HTTP/2 reason), which
        // gRPC clients report as `internal`.
        if let CountingState::AwaitingTrailers = self.counting {
            self.count(code_as_str(tonic::Code::Internal));
        }
    }

    fn on_service_error<E>(&mut self, _error: &E)
    where
        E: std::fmt::Display + 'static,
    {
        // Do nothing if the whole service errored
        //
        // in Axum this isn't possible since all services are required to have an error type of
        // Infallible
    }
}

impl Drop for RpcMetricsCallbackHandler {
    fn drop(&mut self) {
        self.metrics
            .inflight_requests
            .with_label_values(&[self.path.as_ref()])
            .dec();

        let latency = self.start.elapsed().as_secs_f64();
        self.metrics
            .request_latency
            .with_label_values(&[self.path.as_ref()])
            .observe(latency);

        if !matches!(self.counting, CountingState::Counted) {
            self.metrics
                .num_requests
                .with_label_values(&[self.path.as_ref(), "canceled"])
                .inc();
        }
    }
}

fn code_as_str(code: tonic::Code) -> &'static str {
    match code {
        tonic::Code::Ok => "ok",
        tonic::Code::Cancelled => "canceled",
        tonic::Code::Unknown => "unknown",
        tonic::Code::InvalidArgument => "invalid-argument",
        tonic::Code::DeadlineExceeded => "deadline-exceeded",
        tonic::Code::NotFound => "not-found",
        tonic::Code::AlreadyExists => "already-exists",
        tonic::Code::PermissionDenied => "permission-denied",
        tonic::Code::ResourceExhausted => "resource-exhausted",
        tonic::Code::FailedPrecondition => "failed-precondition",
        tonic::Code::Aborted => "aborted",
        tonic::Code::OutOfRange => "out-of-range",
        tonic::Code::Unimplemented => "unimplemented",
        tonic::Code::Internal => "internal",
        tonic::Code::Unavailable => "unavailable",
        tonic::Code::DataLoss => "data-loss",
        tonic::Code::Unauthenticated => "unauthenticated",
    }
}

#[derive(Clone)]
pub(crate) struct ListStreamMetrics {
    method: &'static str,
    first_frame: Histogram,
    page_bytes: Histogram,
    watermark_frames: IntCounter,
    yield_wait: Histogram,
    render: Histogram,
    chunk_queue: Histogram,
    chunk_work: Histogram,
    query_ends: IntCounterVec,
    bitmap_buckets_evaluated: Histogram,
}

impl ListStreamMetrics {
    pub(crate) fn observe_render(&self, elapsed: Duration) {
        self.render.observe(elapsed.as_secs_f64());
    }

    pub(crate) fn start_queue_timer(&self) -> HistogramTimer {
        self.chunk_queue.start_timer()
    }

    pub(crate) fn start_work_timer(&self) -> HistogramTimer {
        self.chunk_work.start_timer()
    }
}

pub(crate) struct ListRequestMetrics {
    inner: Option<ListRequestMetricsInner>,
}

struct ListRequestMetricsInner {
    handles: ListStreamMetrics,
    started: Instant,
    first_frame_observed: bool,
    success_finished: bool,
}

impl ListRequestMetrics {
    pub(crate) fn new(handles: Option<ListStreamMetrics>, started: Instant) -> Self {
        Self {
            inner: handles.map(|handles| ListRequestMetricsInner {
                handles,
                started,
                first_frame_observed: false,
                success_finished: false,
            }),
        }
    }

    pub(crate) fn chunk_metrics(&self) -> Option<ListStreamMetrics> {
        self.inner.as_ref().map(|inner| inner.handles.clone())
    }

    pub(crate) fn observe_frame<M: prost::Message>(&mut self, response: &M, is_data: bool) {
        let Some(inner) = &mut self.inner else {
            return;
        };
        if is_data {
            inner
                .handles
                .page_bytes
                .observe(response.encoded_len() as f64);
        } else {
            inner.handles.watermark_frames.inc();
        }
        if !inner.first_frame_observed {
            inner
                .handles
                .first_frame
                .observe(inner.started.elapsed().as_secs_f64());
            inner.first_frame_observed = true;
        }
    }

    pub(crate) fn yield_clock(&self) -> Option<Instant> {
        self.inner.as_ref().map(|_| Instant::now())
    }

    /// Pair with `yield_clock`: capture immediately before `yield`, then observe as the first
    /// statement after resumption. A stream dropped while suspended records no sample.
    pub(crate) fn observe_yield_wait(&self, yield_started: Option<Instant>) {
        if let (Some(inner), Some(yield_started)) = (&self.inner, yield_started) {
            inner
                .handles
                .yield_wait
                .observe(yield_started.elapsed().as_secs_f64());
        }
    }

    pub(crate) fn finish_success(
        &mut self,
        reason: sui_rpc::proto::sui::rpc::v2::QueryEndReason,
        bitmap_buckets_evaluated: Option<usize>,
    ) {
        let Some(inner) = &mut self.inner else {
            return;
        };
        if inner.success_finished {
            return;
        }
        inner.success_finished = true;
        let reason = match reason {
            sui_rpc::proto::sui::rpc::v2::QueryEndReason::ItemLimit => "item_limit",
            sui_rpc::proto::sui::rpc::v2::QueryEndReason::ScanLimit => "scan_limit",
            sui_rpc::proto::sui::rpc::v2::QueryEndReason::LedgerTip => "ledger_tip",
            sui_rpc::proto::sui::rpc::v2::QueryEndReason::CheckpointBound => "checkpoint_bound",
            sui_rpc::proto::sui::rpc::v2::QueryEndReason::CursorBound => "cursor_bound",
            // Validation guarantees successful List streams always have a concrete end reason.
            sui_rpc::proto::sui::rpc::v2::QueryEndReason::Unknown => {
                unreachable!("validated successful List stream has an unspecified end reason")
            }
            _ => unreachable!("validated successful List stream has an unsupported end reason"),
        };
        inner
            .handles
            .query_ends
            .with_label_values(&[inner.handles.method, reason])
            .inc();
        if let Some(bitmap_buckets_evaluated) = bitmap_buckets_evaluated {
            inner
                .handles
                .bitmap_buckets_evaluated
                .observe(bitmap_buckets_evaluated as f64);
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum SubscriptionFrameKind {
    Payload,
    Watermark,
}

#[derive(Clone)]
pub(crate) struct SubscriptionStreamMetrics {
    pub(crate) payload_messages: IntCounter,
    watermark_messages: IntCounter,
    payload_bytes: Histogram,
    yield_wait: Histogram,
}

impl SubscriptionStreamMetrics {
    pub(crate) fn observe_frame<M: prost::Message>(
        &self,
        response: &M,
        kind: SubscriptionFrameKind,
    ) {
        match kind {
            SubscriptionFrameKind::Payload => {
                self.payload_messages.inc();
                self.payload_bytes.observe(response.encoded_len() as f64);
            }
            SubscriptionFrameKind::Watermark => {
                self.watermark_messages.inc();
            }
        }
    }

    pub(crate) fn observe_yield_wait(&self, elapsed: Duration) {
        self.yield_wait.observe(elapsed.as_secs_f64());
    }
}

#[derive(Clone)]
pub(crate) struct SubscriptionMetrics {
    pub(crate) inflight_subscribers: IntGaugeVec,
    pub(crate) last_recieved_checkpoint: IntGauge,
    pub payload_messages: IntCounterVec,
    pub(crate) watermark_messages: IntCounterVec,
    pub(crate) payload_bytes: HistogramVec,
    pub(crate) stream_yield_wait_seconds: HistogramVec,
    pub(crate) terminations_total: IntCounterVec,
    pub(crate) index_wait_seconds: Histogram,
    pub(crate) index_wait_timeouts_total: IntCounter,
}

impl SubscriptionMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            inflight_subscribers: register_int_gauge_vec_with_registry!(
                "subscription_inflight_subscribers",
                "Current admitted gRPC subscriptions by type and whether a filter is present.",
                &["type", "filtered"],
                registry,
            )
            .unwrap(),
            last_recieved_checkpoint: register_int_gauge_with_registry!(
                "subscription_last_recieved_checkpoint",
                "Last recieved checkpoint by the subscription service",
                registry,
            )
            .unwrap(),
            payload_messages: register_int_counter_vec_with_registry!(
                "subscription_payload_messages",
                "Total number of payload messages emitted by gRPC subscriptions, by type",
                &["type"],
                registry,
            )
            .unwrap(),
            watermark_messages: register_int_counter_vec_with_registry!(
                "subscription_watermark_messages_total",
                "Total progress-only response frames emitted by gRPC subscriptions, including initial recovery-boundary frames, by type.",
                &["type"],
                registry,
            )
            .unwrap(),
            payload_bytes: register_histogram_vec_with_registry!(
                "subscription_payload_bytes",
                "Protobuf encoded size in bytes of payload response frames yielded by a gRPC subscription, measured with encoded_len without serializing or copying the response. Progress-only frames are excluded and counted by subscription_watermark_messages_total.",
                &["type"],
                prometheus::exponential_buckets(1024.0, 2.0, 17).unwrap(),
                registry,
            )
            .unwrap(),
            stream_yield_wait_seconds: register_histogram_vec_with_registry!(
                "subscription_stream_yield_wait_seconds",
                "Time in seconds from yielding any gRPC subscription response until the stream is polled again; this is a downstream transport consumption and backpressure signal.",
                &["type"],
                prometheus::exponential_buckets(0.00001, 2.0, 24).unwrap(),
                registry,
            )
            .unwrap(),
            terminations_total: register_int_counter_vec_with_registry!(
                "subscription_terminations_total",
                "Admitted gRPC subscriptions terminated by bounded lifecycle reason. Admission rejections are excluded.",
                &["type", "reason"],
                registry,
            )
            .unwrap(),
            index_wait_seconds: register_histogram_with_registry!(
                "subscription_index_wait_seconds",
                "Time in seconds spent waiting for the subscription index to catch up before dispatching a checkpoint. Checkpoints that do not wait are excluded.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            index_wait_timeouts_total: register_int_counter_with_registry!(
                "subscription_index_wait_timeouts_total",
                "Total subscription index waits that reached the 10-second timeout and dispatched the checkpoint before the index caught up.",
                registry,
            )
            .unwrap(),
        }
    }
}
impl SubscriptionMetrics {
    pub(crate) fn stream_metrics(&self, type_label: &'static str) -> SubscriptionStreamMetrics {
        SubscriptionStreamMetrics {
            payload_messages: self.payload_messages.with_label_values(&[type_label]),
            watermark_messages: self.watermark_messages.with_label_values(&[type_label]),
            payload_bytes: self.payload_bytes.with_label_values(&[type_label]),
            yield_wait: self
                .stream_yield_wait_seconds
                .with_label_values(&[type_label]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    use prost_types::{
        FileDescriptorProto, FileDescriptorSet, MethodDescriptorProto, ServiceDescriptorProto,
    };
    use sui_rpc::proto::sui::rpc::v2::{
        ListTransactionsResponse, QueryEnd, SubscribeCheckpointsResponse, SubscribeEventsResponse,
        SubscribeTransactionsResponse, Watermark,
    };

    fn encode(set: FileDescriptorSet) -> Vec<u8> {
        let mut buf = Vec::with_capacity(set.encoded_len());
        set.encode(&mut buf).unwrap();
        buf
    }

    fn fds(package: &str, services: &[(&str, &[&str])]) -> Vec<u8> {
        encode(FileDescriptorSet {
            file: vec![FileDescriptorProto {
                package: Some(package.to_owned()),
                service: services
                    .iter()
                    .map(|(name, methods)| ServiceDescriptorProto {
                        name: Some((*name).to_owned()),
                        method: methods
                            .iter()
                            .map(|m| MethodDescriptorProto {
                                name: Some((*m).to_owned()),
                                ..Default::default()
                            })
                            .collect(),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            }],
        })
    }

    #[test]
    fn parses_method_paths_from_file_descriptor_sets() {
        let v2 = fds(
            "sui.rpc.v2",
            &[("LedgerService", &["GetCheckpoint", "GetTransaction"])],
        );
        let v2alpha = fds(
            "sui.rpc.v2alpha",
            &[("ProofService", &["GetCheckpointObjectProof"])],
        );

        let paths = grpc_method_paths_from_file_descriptor_sets(&[&v2, &v2alpha]).unwrap();

        assert_eq!(paths.len(), 3);
        assert!(paths.contains("/sui.rpc.v2.LedgerService/GetCheckpoint"));
        assert!(paths.contains("/sui.rpc.v2.LedgerService/GetTransaction"));
        assert!(paths.contains("/sui.rpc.v2alpha.ProofService/GetCheckpointObjectProof"));
    }

    #[test]
    fn parser_handles_files_without_a_package() {
        let bare = fds("", &[("BareService", &["Ping"])]);
        let paths = grpc_method_paths_from_file_descriptor_sets(&[&bare]).unwrap();
        assert!(paths.contains("/BareService/Ping"));
    }

    #[test]
    fn known_grpc_method_uses_uri_path_label() {
        let mut allowlist = HashSet::new();
        allowlist.insert("/sui.rpc.v2.LedgerService/GetCheckpoint".to_owned());

        let label = compute_metric_label(
            true,
            "/sui.rpc.v2.LedgerService/GetCheckpoint",
            Some("/sui.rpc.v2.LedgerService/{*rest}"),
            &allowlist,
        );
        assert_eq!(label, "/sui.rpc.v2.LedgerService/GetCheckpoint");
    }

    #[test]
    fn known_grpc_method_without_matched_path_uses_uri_path_label() {
        let mut allowlist = HashSet::new();
        allowlist.insert("/sui.rpc.v2.LedgerService/ListTransactions".to_owned());

        let label = compute_metric_label(
            true,
            "/sui.rpc.v2.LedgerService/ListTransactions",
            None,
            &allowlist,
        );
        assert_eq!(label, "/sui.rpc.v2.LedgerService/ListTransactions");
    }

    #[test]
    fn unknown_grpc_method_falls_back_to_route_pattern() {
        // Empty allowlist simulates an attacker hitting an unknown method
        // under a registered service. The label must collapse onto the
        // route pattern instead of the attacker-controlled URI path,
        // otherwise the prometheus label map can be inflated without bound.
        let allowlist = HashSet::new();
        let label = compute_metric_label(
            true,
            "/sui.rpc.v2.LedgerService/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            Some("/sui.rpc.v2.LedgerService/{*rest}"),
            &allowlist,
        );
        assert_eq!(label, "/sui.rpc.v2.LedgerService/{*rest}");
    }

    #[test]
    fn non_grpc_request_uses_matched_path() {
        let allowlist = HashSet::new();
        let label = compute_metric_label(false, "/health", Some("/health"), &allowlist);
        assert_eq!(label, "/health");
    }

    #[test]
    fn request_without_matched_path_is_labelled_unknown() {
        let allowlist = HashSet::new();
        let label = compute_metric_label(true, "/no/match", None, &allowlist);
        assert_eq!(label, "unknown");
    }

    #[test]
    fn grpc_content_type_accepts_codec_suffixes() {
        assert!(is_grpc_content_type(&http::HeaderValue::from_static(
            "application/grpc"
        )));
        assert!(is_grpc_content_type(&http::HeaderValue::from_static(
            "application/grpc+proto"
        )));
        assert!(!is_grpc_content_type(&http::HeaderValue::from_static(
            "application/json"
        )));
    }

    /// Builds a handler for a request with no matched path, so all metric
    /// observations land on the "unknown" label.
    fn make_test_handler(metrics: &Arc<RpcMetrics>) -> RpcMetricsCallbackHandler {
        let make = RpcMetricsMakeCallbackHandler::new(metrics.clone());
        let (parts, _) = http::Request::new(()).into_parts();
        let (_, handler) = make.make_handler(&parts);
        handler
    }

    const TEST_GRPC_PATH: &str = "/test.Service/Method";

    /// Builds a handler for a native gRPC request to `TEST_GRPC_PATH`.
    fn make_grpc_test_handler(metrics: &Arc<RpcMetrics>) -> RpcMetricsCallbackHandler {
        let make = RpcMetricsMakeCallbackHandler::with_grpc_method_allowlist(
            metrics.clone(),
            Arc::new(HashSet::from([TEST_GRPC_PATH.to_owned()])),
        );
        let (parts, _) = http::Request::builder()
            .method(http::Method::POST)
            .uri(TEST_GRPC_PATH)
            .header(http::header::CONTENT_TYPE, "application/grpc")
            .body(())
            .unwrap()
            .into_parts();
        let (_, handler) = make.make_handler(&parts);
        handler
    }

    /// Headers of a native gRPC response that streams its body and reports its
    /// status in the trailers.
    fn grpc_streaming_response_parts() -> http::response::Parts {
        let (parts, _) = http::Response::builder()
            .header(http::header::CONTENT_TYPE, "application/grpc")
            .body(())
            .unwrap()
            .into_parts();
        parts
    }

    /// The non-zero `rpc_requests` counts for `path`, keyed by gRPC status.
    fn grpc_status_counts(metrics: &RpcMetrics, path: &str) -> BTreeMap<&'static str, u64> {
        (0..=16)
            .map(|code| code_as_str(tonic::Code::from_i32(code)))
            .map(|status| {
                let count = metrics
                    .num_requests
                    .with_label_values(&[path, status])
                    .get();
                (status, count)
            })
            .filter(|(_, count)| *count > 0)
            .collect()
    }

    // The handler latency is observed as soon as the handler produces a
    // response, while the total request latency is only observed once the
    // handler is dropped (i.e. the response body finished streaming).
    #[test]
    fn handler_latency_observed_on_response_and_total_latency_on_drop() {
        let metrics = Arc::new(RpcMetrics::new(&Registry::new()));
        let mut handler = make_test_handler(&metrics);

        let handler_latency = metrics
            .request_handler_latency
            .with_label_values(&["unknown"]);
        let total_latency = metrics.request_latency.with_label_values(&["unknown"]);

        assert_eq!(handler_latency.get_sample_count(), 0);

        let (parts, _) = http::Response::new(()).into_parts();
        handler.on_response(&parts);

        assert_eq!(handler_latency.get_sample_count(), 1);
        assert_eq!(total_latency.get_sample_count(), 0);

        drop(handler);

        assert_eq!(handler_latency.get_sample_count(), 1);
        assert_eq!(total_latency.get_sample_count(), 1);
    }

    #[test]
    fn first_chunk_latency_observed_once_on_first_body_chunk() {
        let metrics = Arc::new(RpcMetrics::new(&Registry::new()));
        let mut handler = make_test_handler(&metrics);
        let first_chunk_latency = metrics.first_chunk_latency.with_label_values(&["unknown"]);

        let (parts, _) = http::Response::new(()).into_parts();
        handler.on_response(&parts);
        handler.on_body_chunk(&bytes::Bytes::from_static(b"first"));
        handler.on_body_chunk(&bytes::Bytes::from_static(b"second"));

        assert_eq!(first_chunk_latency.get_sample_count(), 1);

        drop(handler);

        assert_eq!(first_chunk_latency.get_sample_count(), 1);
    }

    // A request canceled before the handler produces a response records the
    // total latency and the canceled count, but no handler latency.
    #[test]
    fn handler_latency_not_observed_for_canceled_requests() {
        let metrics = Arc::new(RpcMetrics::new(&Registry::new()));
        let handler = make_test_handler(&metrics);

        drop(handler);

        assert_eq!(
            metrics
                .request_handler_latency
                .with_label_values(&["unknown"])
                .get_sample_count(),
            0
        );
        assert_eq!(
            metrics
                .first_chunk_latency
                .with_label_values(&["unknown"])
                .get_sample_count(),
            0
        );
        assert_eq!(
            metrics
                .request_latency
                .with_label_values(&["unknown"])
                .get_sample_count(),
            1
        );
        assert_eq!(
            metrics
                .num_requests
                .with_label_values(&["unknown", "canceled"])
                .get(),
            1
        );
    }

    // gRPC requires a stream to end with `grpc-status` in its trailers. A
    // stream that ends without one was truncated, which clients report as
    // `unknown`.
    #[test]
    fn grpc_stream_ending_without_grpc_status_is_counted_unknown() {
        let metrics = Arc::new(RpcMetrics::new(&Registry::new()));
        let mut handler = make_grpc_test_handler(&metrics);

        handler.on_response(&grpc_streaming_response_parts());
        handler.on_body_chunk(&bytes::Bytes::from_static(b"message"));
        handler.on_end_of_stream(Some(&http::HeaderMap::new()));
        drop(handler);

        assert_eq!(
            grpc_status_counts(&metrics, TEST_GRPC_PATH),
            BTreeMap::from([("unknown", 1)])
        );
    }

    // A response body that fails ends the stream without trailers; the client
    // sees the stream reset and reports `internal`.
    #[test]
    fn grpc_stream_whose_body_fails_is_counted_internal() {
        let metrics = Arc::new(RpcMetrics::new(&Registry::new()));
        let mut handler = make_grpc_test_handler(&metrics);

        handler.on_response(&grpc_streaming_response_parts());
        handler.on_body_chunk(&bytes::Bytes::from_static(b"message"));
        handler.on_body_error(&"response encoding failed");
        drop(handler);

        assert_eq!(
            grpc_status_counts(&metrics, TEST_GRPC_PATH),
            BTreeMap::from([("internal", 1)])
        );
    }

    fn metric_label_sets(
        family: &prometheus::proto::MetricFamily,
    ) -> BTreeSet<Vec<(String, String)>> {
        family
            .get_metric()
            .iter()
            .map(|metric| {
                let mut labels = metric
                    .get_label()
                    .iter()
                    .map(|label| (label.name().to_owned(), label.value().to_owned()))
                    .collect::<Vec<_>>();
                labels.sort();
                labels
            })
            .collect()
    }

    fn expected_label_sets(rows: Vec<Vec<(&str, &str)>>) -> BTreeSet<Vec<(String, String)>> {
        rows.into_iter()
            .map(|row| {
                let mut labels = row
                    .into_iter()
                    .map(|(name, value)| (name.to_owned(), value.to_owned()))
                    .collect::<Vec<_>>();
                labels.sort();
                labels
            })
            .collect()
    }

    fn assert_metric_family(
        families: &[prometheus::proto::MetricFamily],
        name: &str,
        expected_labels: BTreeSet<Vec<(String, String)>>,
    ) {
        let family = families
            .iter()
            .find(|family| family.name() == name)
            .unwrap_or_else(|| panic!("missing metric family {name}"));
        assert_eq!(metric_label_sets(family), expected_labels, "{name}");
    }

    #[test]
    fn focused_metric_families_use_exact_bounded_labels() {
        let registry = Registry::new();
        let list_metrics = ListApiMetrics::new(&registry);
        let method_resolutions = [
            ("list_checkpoints", "summary"),
            ("list_checkpoints", "transactions"),
            ("list_checkpoints", "objects"),
            ("list_transactions", "digest"),
            ("list_transactions", "full"),
            ("list_transactions", "full_objects"),
            ("list_events", "no_json"),
            ("list_events", "json"),
        ];
        for (method, resolution) in method_resolutions {
            list_metrics.stream_metrics(method, resolution);
        }
        let methods = ["list_checkpoints", "list_transactions", "list_events"];
        let reasons = [
            "item_limit",
            "scan_limit",
            "ledger_tip",
            "checkpoint_bound",
            "cursor_bound",
        ];
        for method in methods {
            for reason in reasons {
                list_metrics
                    .list_query_ends_total
                    .with_label_values(&[method, reason]);
            }
        }

        let subscription_metrics = SubscriptionMetrics::new(&registry);
        let types = ["checkpoint", "transaction", "event"];
        for type_label in types {
            subscription_metrics.stream_metrics(type_label);
            for filtered in ["true", "false"] {
                subscription_metrics
                    .inflight_subscribers
                    .with_label_values(&[type_label, filtered]);
            }
            for reason in [
                "client_closed",
                "slow_consumer",
                "source_lag",
                "service_shutdown",
            ] {
                subscription_metrics
                    .terminations_total
                    .with_label_values(&[type_label, reason]);
            }
        }

        let families = registry.gather();
        let method_resolution_labels = expected_label_sets(
            method_resolutions
                .into_iter()
                .map(|(method, resolution)| vec![("method", method), ("resolution", resolution)])
                .collect(),
        );
        for name in [
            "list_first_frame_seconds",
            "list_response_page_bytes",
            "list_stream_yield_wait_seconds",
            "list_render_seconds",
        ] {
            assert_metric_family(&families, name, method_resolution_labels.clone());
        }
        assert_metric_family(
            &families,
            "list_watermark_frames_total",
            expected_label_sets(
                methods
                    .into_iter()
                    .map(|method| vec![("method", method)])
                    .collect(),
            ),
        );
        assert_metric_family(
            &families,
            "list_chunk_seconds",
            expected_label_sets(
                methods
                    .into_iter()
                    .flat_map(|method| {
                        ["queue", "work"]
                            .into_iter()
                            .map(move |phase| vec![("method", method), ("phase", phase)])
                    })
                    .collect(),
            ),
        );
        assert_metric_family(
            &families,
            "list_query_ends_total",
            expected_label_sets(
                methods
                    .into_iter()
                    .flat_map(|method| {
                        reasons
                            .into_iter()
                            .map(move |reason| vec![("method", method), ("reason", reason)])
                    })
                    .collect(),
            ),
        );
        assert_metric_family(
            &families,
            "list_bitmap_buckets_evaluated",
            expected_label_sets(
                methods
                    .into_iter()
                    .map(|method| vec![("method", method)])
                    .collect(),
            ),
        );

        let type_labels = expected_label_sets(
            types
                .into_iter()
                .map(|type_label| vec![("type", type_label)])
                .collect(),
        );
        assert_metric_family(
            &families,
            "subscription_payload_messages",
            type_labels.clone(),
        );
        assert_metric_family(
            &families,
            "subscription_watermark_messages_total",
            type_labels.clone(),
        );
        assert_metric_family(
            &families,
            "subscription_stream_yield_wait_seconds",
            type_labels.clone(),
        );
        assert_metric_family(&families, "subscription_payload_bytes", type_labels);
        assert_metric_family(
            &families,
            "subscription_inflight_subscribers",
            expected_label_sets(
                types
                    .into_iter()
                    .flat_map(|type_label| {
                        ["true", "false"]
                            .into_iter()
                            .map(move |filtered| vec![("type", type_label), ("filtered", filtered)])
                    })
                    .collect(),
            ),
        );
        assert_metric_family(
            &families,
            "subscription_terminations_total",
            expected_label_sets(
                types
                    .into_iter()
                    .flat_map(|type_label| {
                        [
                            "client_closed",
                            "slow_consumer",
                            "source_lag",
                            "service_shutdown",
                        ]
                        .into_iter()
                        .map(move |reason| vec![("type", type_label), ("reason", reason)])
                    })
                    .collect(),
            ),
        );
        assert_metric_family(
            &families,
            "subscription_index_wait_seconds",
            expected_label_sets(vec![vec![]]),
        );
        assert_metric_family(
            &families,
            "subscription_index_wait_timeouts_total",
            expected_label_sets(vec![vec![]]),
        );
    }

    #[test]
    fn list_page_and_watermark_metrics_cover_all_frame_kinds() {
        let registry = Registry::new();
        let metrics = ListApiMetrics::new(&registry);
        let handles = metrics.stream_metrics("list_transactions", "full");
        let mut request_metrics = ListRequestMetrics::new(Some(handles.clone()), Instant::now());

        let mut data = ListTransactionsResponse::default();
        data.transaction = Some(Default::default());
        let mut watermark_only = ListTransactionsResponse::default();
        watermark_only.watermark = Some(Watermark::default());
        let mut terminal = ListTransactionsResponse::default();
        terminal.end = Some(QueryEnd::default());

        request_metrics.observe_frame(&watermark_only, false);
        assert_eq!(handles.first_frame.get_sample_count(), 1);
        let yield_started = request_metrics.yield_clock();
        request_metrics.observe_yield_wait(yield_started);
        request_metrics.observe_frame(&data, true);
        let yield_started = request_metrics.yield_clock();
        request_metrics.observe_yield_wait(yield_started);
        request_metrics.observe_frame(&terminal, false);
        let yield_started = request_metrics.yield_clock();
        request_metrics.observe_yield_wait(yield_started);
        handles.observe_render(Duration::from_millis(1));

        assert_eq!(handles.page_bytes.get_sample_count(), 1);
        assert_eq!(
            handles.page_bytes.get_sample_sum(),
            data.encoded_len() as f64
        );
        assert_eq!(handles.watermark_frames.get(), 2);
        assert_eq!(handles.first_frame.get_sample_count(), 1);
        assert_eq!(handles.yield_wait.get_sample_count(), 3);
        assert_eq!(handles.render.get_sample_count(), 1);

        let terminal_registry = Registry::new();
        let terminal_metrics = ListApiMetrics::new(&terminal_registry);
        let terminal_handles = terminal_metrics.stream_metrics("list_transactions", "digest");
        let mut terminal_request =
            ListRequestMetrics::new(Some(terminal_handles.clone()), Instant::now());
        terminal_request.observe_frame(&terminal, false);

        assert_eq!(terminal_handles.page_bytes.get_sample_count(), 0);
        assert_eq!(terminal_handles.page_bytes.get_sample_sum(), 0.0);
        assert_eq!(terminal_handles.watermark_frames.get(), 1);
        assert_eq!(terminal_handles.first_frame.get_sample_count(), 1);
    }

    fn assert_subscription_response_metrics<M: Message>(
        metrics: &SubscriptionMetrics,
        type_label: &'static str,
        payload: &M,
        watermark: &M,
    ) {
        let stream_metrics = metrics.stream_metrics(type_label);
        stream_metrics.observe_frame(payload, SubscriptionFrameKind::Payload);
        stream_metrics.observe_yield_wait(Duration::from_millis(1));
        stream_metrics.observe_frame(watermark, SubscriptionFrameKind::Watermark);
        stream_metrics.observe_yield_wait(Duration::from_millis(2));

        assert_eq!(stream_metrics.payload_messages.get(), 1);
        assert_eq!(stream_metrics.watermark_messages.get(), 1);
        assert_eq!(stream_metrics.payload_bytes.get_sample_count(), 1);
        assert_eq!(
            stream_metrics.payload_bytes.get_sample_sum(),
            payload.encoded_len() as f64
        );
        assert_eq!(stream_metrics.yield_wait.get_sample_count(), 2);
    }

    #[test]
    fn subscription_response_metrics_split_payload_and_watermark_frames() {
        let registry = Registry::new();
        let metrics = SubscriptionMetrics::new(&registry);

        let mut checkpoint_payload = SubscribeCheckpointsResponse::default();
        checkpoint_payload.cursor = Some(7);
        checkpoint_payload.checkpoint = Some(Default::default());
        let mut checkpoint_watermark = SubscribeCheckpointsResponse::default();
        checkpoint_watermark.cursor = Some(8);
        assert_subscription_response_metrics(
            &metrics,
            "checkpoint",
            &checkpoint_payload,
            &checkpoint_watermark,
        );

        let mut transaction_payload = SubscribeTransactionsResponse::default();
        transaction_payload.transaction = Some(Default::default());
        transaction_payload.watermark = Some(Watermark::default());
        let mut transaction_watermark = SubscribeTransactionsResponse::default();
        transaction_watermark.watermark = Some(Watermark::default());
        assert_subscription_response_metrics(
            &metrics,
            "transaction",
            &transaction_payload,
            &transaction_watermark,
        );

        let mut event_payload = SubscribeEventsResponse::default();
        event_payload.event = Some(Default::default());
        event_payload.watermark = Some(Watermark::default());
        let mut event_watermark = SubscribeEventsResponse::default();
        event_watermark.watermark = Some(Watermark::default());
        assert_subscription_response_metrics(&metrics, "event", &event_payload, &event_watermark);
    }

    /// Runs the metrics layer inside real servers wired like `sui-kv-rpc` and
    /// the fullnode, driven by a raw HTTP/2 client so that client resets land
    /// on exact frames.
    ///
    /// These tests use the default current-thread runtime, so the servers'
    /// tasks never run concurrently with an assertion: once a request's
    /// `request_latency` sample is visible, the handler's `Drop` has finished
    /// and its `rpc_requests` count is final.
    mod end_to_end {
        use super::*;
        use std::{convert::Infallible, net::SocketAddr};

        use bytes::Bytes;
        use futures::{StreamExt, future::BoxFuture, stream};
        use mysten_network::request_log::GrpcRequestLogLayer;
        use sui_http::middleware::callback::CallbackLayer;
        use tonic::codegen::{Body, StdError};

        /// Server-streaming method that sends two messages and ends with OK.
        const STREAM_OK: &str = "/test.Service/StreamOk";
        /// Server-streaming method that sends two messages and then fails with
        /// `DEADLINE_EXCEEDED`.
        const STREAM_DEADLINE_EXCEEDED: &str = "/test.Service/StreamDeadlineExceeded";
        /// Server-streaming method that sends one message and then stalls.
        const STREAM_STALL: &str = "/test.Service/StreamStall";
        /// Unary method whose handler fails with `INTERNAL`.
        const UNARY_INTERNAL: &str = "/test.Service/UnaryInternal";

        /// A length-prefixed gRPC frame carrying an empty message.
        const EMPTY_MESSAGE_FRAME: &[u8] = &[0, 0, 0, 0, 0];

        #[derive(Clone, Copy, Debug)]
        enum ServerStack {
            /// `tonic::transport::Server` with the metrics layer, as in `sui-kv-rpc`.
            KvRpc,
            /// The `grpc::Services` router with the metrics layer, served by
            /// `sui_http`, as in the fullnode.
            Fullnode,
        }

        struct TestServer {
            address: SocketAddr,
            metrics: Arc<RpcMetrics>,
            // Keeps the fullnode stack's server running.
            _handle: Option<sui_http::ServerHandle>,
        }

        #[derive(Clone)]
        struct TestService;

        impl tonic::server::NamedService for TestService {
            const NAME: &'static str = "test.Service";
        }

        impl<B> tower::Service<http::Request<B>> for TestService
        where
            B: Body + Send + 'static,
            B::Error: Into<StdError> + Send + 'static,
        {
            type Response = http::Response<tonic::body::Body>;
            type Error = Infallible;
            type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

            fn poll_ready(
                &mut self,
                _cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Result<(), Self::Error>> {
                std::task::Poll::Ready(Ok(()))
            }

            fn call(&mut self, request: http::Request<B>) -> Self::Future {
                Box::pin(async move {
                    let response = match request.uri().path() {
                        STREAM_OK => serve_stream(request, stream::iter([Ok(()), Ok(())])).await,
                        STREAM_DEADLINE_EXCEEDED => {
                            let messages = stream::iter([
                                Ok(()),
                                Ok(()),
                                Err(tonic::Status::deadline_exceeded(
                                    "request deadline exceeded",
                                )),
                            ]);
                            serve_stream(request, messages).await
                        }
                        STREAM_STALL => {
                            let messages = stream::iter([Ok(())]).chain(stream::pending());
                            serve_stream(request, messages).await
                        }
                        UNARY_INTERNAL => {
                            let handler = tower::service_fn(|_: tonic::Request<()>| async {
                                Err::<tonic::Response<()>, _>(tonic::Status::internal(
                                    "handler failed",
                                ))
                            });
                            tonic::server::Grpc::new(tonic_prost::ProstCodec::<(), ()>::default())
                                .unary(handler, request)
                                .await
                        }
                        path => panic!("unexpected method {path}"),
                    };
                    Ok(response)
                })
            }
        }

        /// Answers a server-streaming request with `messages`.
        async fn serve_stream<B, S>(
            request: http::Request<B>,
            messages: S,
        ) -> http::Response<tonic::body::Body>
        where
            B: Body + Send + 'static,
            B::Error: Into<StdError> + Send,
            S: futures::Stream<Item = Result<(), tonic::Status>> + Send + 'static,
        {
            let mut messages = Some(messages);
            let handler = tower::service_fn(move |_: tonic::Request<()>| {
                let messages = messages.take().expect("one handler call per request");
                async move { Ok::<_, tonic::Status>(tonic::Response::new(messages)) }
            });
            tonic::server::Grpc::new(tonic_prost::ProstCodec::<(), ()>::default())
                .server_streaming(handler, request)
                .await
        }

        async fn start(stack: ServerStack) -> TestServer {
            let metrics = Arc::new(RpcMetrics::new(&Registry::new()));
            let allowlist = [
                STREAM_OK,
                STREAM_DEADLINE_EXCEEDED,
                STREAM_STALL,
                UNARY_INTERNAL,
            ]
            .into_iter()
            .map(str::to_owned)
            .collect();
            let metrics_layer =
                CallbackLayer::new(RpcMetricsMakeCallbackHandler::with_grpc_method_allowlist(
                    metrics.clone(),
                    Arc::new(allowlist),
                ));

            match stack {
                ServerStack::KvRpc => {
                    let incoming =
                        tonic::transport::server::TcpIncoming::bind("127.0.0.1:0".parse().unwrap())
                            .unwrap();
                    let address = incoming.local_addr().unwrap();
                    tokio::spawn(
                        tonic::transport::Server::builder()
                            .layer(metrics_layer)
                            .add_service(TestService)
                            .serve_with_incoming(incoming),
                    );
                    TestServer {
                        address,
                        metrics,
                        _handle: None,
                    }
                }
                ServerStack::Fullnode => {
                    let router = crate::grpc::Services::new()
                        .add_service(TestService)
                        .into_router(
                            GrpcRequestLogLayer::from_encoded_file_descriptor_sets([]).unwrap(),
                        )
                        .layer(metrics_layer);
                    let handle = sui_http::Builder::new()
                        .serve("127.0.0.1:0", router)
                        .unwrap();
                    TestServer {
                        address: *handle.local_addr(),
                        metrics,
                        _handle: Some(handle),
                    }
                }
            }
        }

        async fn connect(address: SocketAddr) -> h2::client::SendRequest<Bytes> {
            let tcp = tokio::net::TcpStream::connect(address).await.unwrap();
            let (client, connection) = h2::client::handshake(tcp).await.unwrap();
            tokio::spawn(connection);
            client.ready().await.unwrap()
        }

        /// Opens a request stream with the given `content-type` on a fresh
        /// connection without sending any request body.
        async fn open_request(
            server: &TestServer,
            path: &str,
            content_type: &str,
        ) -> (h2::client::ResponseFuture, h2::SendStream<Bytes>) {
            let request = http::Request::builder()
                .method(http::Method::POST)
                .uri(format!("http://{}{path}", server.address))
                .header(http::header::CONTENT_TYPE, content_type)
                .header(http::header::TE, "trailers")
                .body(())
                .unwrap();
            connect(server.address)
                .await
                .send_request(request, false)
                .unwrap()
        }

        /// Sends a native gRPC request carrying one empty message.
        async fn send_request(
            server: &TestServer,
            path: &str,
        ) -> (h2::client::ResponseFuture, h2::SendStream<Bytes>) {
            let (response, mut request_body) = open_request(server, path, "application/grpc").await;
            request_body
                .send_data(Bytes::from_static(EMPTY_MESSAGE_FRAME), true)
                .unwrap();
            (response, request_body)
        }

        /// The gRPC status the client receives: from the headers of a
        /// trailers-only response, otherwise from the trailers after the body.
        async fn client_grpc_status(response: http::Response<h2::RecvStream>) -> tonic::Code {
            let (parts, mut body) = response.into_parts();
            let grpc_status = match parts.headers.get(&GRPC_STATUS) {
                Some(grpc_status) => grpc_status.clone(),
                None => {
                    while let Some(chunk) = body.data().await {
                        let chunk = chunk.unwrap();
                        body.flow_control().release_capacity(chunk.len()).unwrap();
                    }
                    let trailers = body.trailers().await.unwrap().expect("missing trailers");
                    trailers
                        .get(&GRPC_STATUS)
                        .expect("missing grpc-status")
                        .clone()
                }
            };
            tonic::Code::from_bytes(grpc_status.as_bytes())
        }

        /// Waits for the server to drop the metrics handler of the one request
        /// sent to `path`, then returns that path's `rpc_requests` counts.
        async fn settled_grpc_status_counts(
            server: &TestServer,
            path: &str,
        ) -> BTreeMap<&'static str, u64> {
            let request_latency = server.metrics.request_latency.with_label_values(&[path]);
            tokio::time::timeout(Duration::from_secs(10), async {
                while request_latency.get_sample_count() == 0 {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .expect("the server never finished the request");
            grpc_status_counts(&server.metrics, path)
        }

        // The incident's `internal` population: a client resets its stream
        // before the request message arrives, tonic answers the truncated
        // request with `internal` ("Missing request message."), and the client
        // never receives that answer.
        #[tokio::test]
        async fn client_reset_before_request_message_is_counted_canceled() {
            for stack in [ServerStack::KvRpc, ServerStack::Fullnode] {
                let server = start(stack).await;
                let (_response, mut request_body) =
                    open_request(&server, STREAM_OK, "application/grpc").await;
                request_body.send_reset(h2::Reason::CANCEL);

                assert_eq!(
                    settled_grpc_status_counts(&server, STREAM_OK).await,
                    BTreeMap::from([("canceled", 1)]),
                    "{stack:?}"
                );
                // The service answered the truncated request, so the count
                // comes from that answer being reclassified rather than from
                // the request being dropped before any answer.
                assert_eq!(
                    server
                        .metrics
                        .request_handler_latency
                        .with_label_values(&[STREAM_OK])
                        .get_sample_count(),
                    1,
                    "{stack:?}"
                );
            }
        }

        #[tokio::test]
        async fn handler_internal_error_is_counted_internal() {
            for stack in [ServerStack::KvRpc, ServerStack::Fullnode] {
                let server = start(stack).await;
                let (response, _request_body) = send_request(&server, UNARY_INTERNAL).await;

                assert_eq!(
                    client_grpc_status(response.await.unwrap()).await,
                    tonic::Code::Internal,
                    "{stack:?}"
                );
                assert_eq!(
                    settled_grpc_status_counts(&server, UNARY_INTERNAL).await,
                    BTreeMap::from([("internal", 1)]),
                    "{stack:?}"
                );
            }
        }

        #[tokio::test]
        async fn stream_ending_with_error_trailers_is_counted_with_trailer_status() {
            for stack in [ServerStack::KvRpc, ServerStack::Fullnode] {
                let server = start(stack).await;
                let (response, _request_body) =
                    send_request(&server, STREAM_DEADLINE_EXCEEDED).await;

                assert_eq!(
                    client_grpc_status(response.await.unwrap()).await,
                    tonic::Code::DeadlineExceeded,
                    "{stack:?}"
                );
                assert_eq!(
                    settled_grpc_status_counts(&server, STREAM_DEADLINE_EXCEEDED).await,
                    BTreeMap::from([("deadline-exceeded", 1)]),
                    "{stack:?}"
                );
            }
        }

        #[tokio::test]
        async fn stream_ending_with_ok_trailers_is_counted_ok() {
            for stack in [ServerStack::KvRpc, ServerStack::Fullnode] {
                let server = start(stack).await;
                let (response, _request_body) = send_request(&server, STREAM_OK).await;

                assert_eq!(
                    client_grpc_status(response.await.unwrap()).await,
                    tonic::Code::Ok,
                    "{stack:?}"
                );
                assert_eq!(
                    settled_grpc_status_counts(&server, STREAM_OK).await,
                    BTreeMap::from([("ok", 1)]),
                    "{stack:?}"
                );
            }
        }

        #[tokio::test]
        async fn stream_abandoned_by_client_mid_stream_is_counted_canceled() {
            for stack in [ServerStack::KvRpc, ServerStack::Fullnode] {
                let server = start(stack).await;
                let (response, mut request_body) = send_request(&server, STREAM_STALL).await;

                let mut response_body = response.await.unwrap().into_body();
                let first_message = response_body.data().await.unwrap().unwrap();
                assert_eq!(first_message.as_ref(), EMPTY_MESSAGE_FRAME, "{stack:?}");
                request_body.send_reset(h2::Reason::CANCEL);

                assert_eq!(
                    settled_grpc_status_counts(&server, STREAM_STALL).await,
                    BTreeMap::from([("canceled", 1)]),
                    "{stack:?}"
                );
            }
        }

        // grpc-web encodes its trailers into the response body, so even a
        // successful grpc-web stream ends its HTTP body without trailers. Only
        // the fullnode serves grpc-web.
        #[tokio::test]
        async fn successful_grpc_web_stream_is_counted_ok() {
            let server = start(ServerStack::Fullnode).await;
            let (response, mut request_body) =
                open_request(&server, STREAM_OK, "application/grpc-web+proto").await;
            request_body
                .send_data(Bytes::from_static(EMPTY_MESSAGE_FRAME), true)
                .unwrap();

            let response = response.await.unwrap();
            assert_eq!(
                response.headers()[http::header::CONTENT_TYPE],
                "application/grpc-web+proto"
            );
            let mut response_body = response.into_body();
            while let Some(chunk) = response_body.data().await {
                let chunk = chunk.unwrap();
                response_body
                    .flow_control()
                    .release_capacity(chunk.len())
                    .unwrap();
            }
            assert!(response_body.trailers().await.unwrap().is_none());

            assert_eq!(
                settled_grpc_status_counts(&server, STREAM_OK).await,
                BTreeMap::from([("ok", 1)])
            );
        }
    }
}
