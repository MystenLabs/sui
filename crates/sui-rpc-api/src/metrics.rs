// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{body::Body as AxumBody, extract::State, http, middleware::Next};
use http_body::{Body, Frame};
use pin_project_lite::pin_project;
use std::{
    borrow::Cow,
    collections::HashSet,
    pin::Pin,
    sync::{Arc, OnceLock},
    task::{Context, Poll},
    time::{Duration, Instant},
};

use prometheus::{
    Histogram, HistogramTimer, HistogramVec, IntCounter, IntCounterVec, IntGauge, IntGaugeVec,
    Registry, register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry,
};
use prost::Message;
use sui_http::middleware::callback::{MakeCallbackHandler, ResponseHandler};

#[derive(Clone)]
pub struct RpcMetrics {
    inflight_requests: IntGaugeVec,
    num_requests: IntCounterVec,
    request_latency: HistogramVec,
    request_handler_latency: HistogramVec,
    first_chunk_latency: HistogramVec,
    grpc_body_poll_gap: HistogramVec,
    grpc_body_first_poll: HistogramVec,
    grpc_body_poll_results: IntCounterVec,
    grpc_body_trailers_gap: HistogramVec,
    tokio_runtime_num_workers: IntGauge,
    tokio_runtime_global_queue_depth: IntGauge,
    tokio_runtime_num_alive_tasks: IntGauge,
}

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
];

fn parse_list_probes(value: Option<&str>) -> bool {
    match value {
        None | Some("1" | "on" | "true") => true,
        Some("0" | "off" | "false") => false,
        Some(_) => true,
    }
}

pub(crate) fn list_probes_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();

    *ENABLED.get_or_init(|| match std::env::var("SUI_RPC_LIST_PROBES") {
        Ok(value) => {
            if !matches!(
                value.as_str(),
                "1" | "on" | "true" | "0" | "off" | "false"
            ) {
                tracing::warn!(
                    value,
                    "unrecognized SUI_RPC_LIST_PROBES value; list probes enabled"
                );
            }
            parse_list_probes(Some(&value))
        }
        Err(std::env::VarError::NotPresent) => parse_list_probes(None),
        Err(error) => {
            tracing::warn!(%error, "unable to read SUI_RPC_LIST_PROBES; list probes enabled");
            true
        }
    })
}

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
            grpc_body_poll_gap: register_histogram_vec_with_registry!(
                "grpc_body_poll_gap_seconds",
                "Elapsed time between consecutive polls of one gRPC response body at the body boundary. It conflates h2 backpressure with connection-task runtime scheduling, so it discriminates their combined downstream delay from producer chunk work but cannot separate them.",
                &["path"],
                prometheus::exponential_buckets(0.00001, 2.0, 24).unwrap(),
                registry,
            )
            .unwrap(),
            grpc_body_first_poll: register_histogram_vec_with_registry!(
                "grpc_body_first_poll_seconds",
                "Elapsed time from gRPC response-body wrapper creation until its first poll at the body boundary. It measures delay before body consumption begins and helps distinguish consumer scheduling or backpressure from producer chunk work.",
                &["path"],
                prometheus::exponential_buckets(0.00001, 2.0, 24).unwrap(),
                registry,
            )
            .unwrap(),
            grpc_body_poll_results: register_int_counter_vec_with_registry!(
                "grpc_body_poll_results_total",
                "Response-body poll results observed at the gRPC body boundary. Data and trailers count ready frames; pending identifies body-level stalls and helps separate producer readiness from poll cadence.",
                &["path", "result"],
                registry,
            )
            .unwrap(),
            grpc_body_trailers_gap: register_histogram_vec_with_registry!(
                "grpc_body_trailers_gap_seconds",
                "Elapsed time from the last data frame polled to the trailers frame polled at the gRPC body boundary. This is not evidence that the terminal frame reached the client; it distinguishes body-production delay from later transport work.",
                &["path"],
                prometheus::exponential_buckets(0.00001, 2.0, 24).unwrap(),
                registry,
            )
            .unwrap(),
            tokio_runtime_num_workers: register_int_gauge_with_registry!(
                "tokio_runtime_num_workers",
                "Number of Tokio runtime worker threads sampled by the RPC service; provides the scheduler capacity baseline for runtime-congestion attribution.",
                registry,
            )
            .unwrap(),
            tokio_runtime_global_queue_depth: register_int_gauge_with_registry!(
                "tokio_runtime_global_queue_depth",
                "Tasks waiting in the Tokio runtime global queue, sampled once per second; sustained depth identifies runtime scheduling congestion.",
                registry,
            )
            .unwrap(),
            tokio_runtime_num_alive_tasks: register_int_gauge_with_registry!(
                "tokio_runtime_num_alive_tasks",
                "Alive tasks in the Tokio runtime, sampled once per second; growth indicates runtime task pressure that can delay gRPC body polling.",
                registry,
            )
            .unwrap(),
        }
    }
}
impl RpcMetrics {
    pub(crate) fn spawn_runtime_metrics_sampler(self: &Arc<Self>) {
        if !list_probes_enabled() {
            return;
        }
        let weak_metrics = Arc::downgrade(self);
        let runtime_metrics = tokio::runtime::Handle::current().metrics();

        // Tokio 1.52's stable RuntimeMetrics surface exposes worker count,
        // alive tasks, and global queue depth. Blocking thread count, blocking
        // queue depth, and worker-local queue depth require `tokio_unstable`;
        // this sampler deliberately does not enable that cfg.
        let _sampler = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                let Some(metrics) = weak_metrics.upgrade() else {
                    break;
                };
                metrics
                    .tokio_runtime_num_workers
                    .set(usize_to_i64(runtime_metrics.num_workers()));
                metrics
                    .tokio_runtime_global_queue_depth
                    .set(usize_to_i64(runtime_metrics.global_queue_depth()));
                metrics
                    .tokio_runtime_num_alive_tasks
                    .set(usize_to_i64(runtime_metrics.num_alive_tasks()));
            }
        });
    }

    fn grpc_body_metrics(&self, path: &str) -> GrpcBodyMetricHandles {
        GrpcBodyMetricHandles {
            poll_gap: self.grpc_body_poll_gap.with_label_values(&[path]),
            first_poll: self.grpc_body_first_poll.with_label_values(&[path]),
            data: self
                .grpc_body_poll_results
                .with_label_values(&[path, "data"]),
            trailers: self
                .grpc_body_poll_results
                .with_label_values(&[path, "trailers"]),
            pending: self
                .grpc_body_poll_results
                .with_label_values(&[path, "pending"]),
            trailers_gap: self.grpc_body_trailers_gap.with_label_values(&[path]),
        }
    }
}

fn usize_to_i64(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[derive(Clone)]
pub(crate) struct GrpcBodyInstrumentationState {
    metrics: Arc<RpcMetrics>,
    grpc_method_allowlist: GrpcMethodAllowlist,
    list_probes_enabled: bool,
}

impl GrpcBodyInstrumentationState {
    pub(crate) fn new(
        metrics: Arc<RpcMetrics>,
        grpc_method_allowlist: GrpcMethodAllowlist,
    ) -> Self {
        Self {
            metrics,
            grpc_method_allowlist,
            list_probes_enabled: list_probes_enabled(),
        }
    }
}

pub(crate) async fn instrument_grpc_response_body(
    State(state): State<GrpcBodyInstrumentationState>,
    request: http::Request<AxumBody>,
    next: Next,
) -> http::Response<AxumBody> {
    if !state.list_probes_enabled {
        return next.run(request).await;
    }

    let uri_path = request.uri().path();
    let matched_path = request
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|path| path.as_str());
    let body_metrics = uri_path.starts_with("/sui.rpc.v2.").then(|| {
        let path = compute_metric_label(
            true,
            uri_path,
            matched_path,
            &state.grpc_method_allowlist,
        );
        state.metrics.grpc_body_metrics(path.as_ref())
    });
    let response = next.run(request).await;

    match body_metrics {
        Some(metrics) => response.map(|body| AxumBody::new(InstrumentedBody::new(body, metrics))),
        None => response,
    }
}

#[derive(Clone)]
struct GrpcBodyMetricHandles {
    poll_gap: Histogram,
    first_poll: Histogram,
    data: IntCounter,
    trailers: IntCounter,
    pending: IntCounter,
    trailers_gap: Histogram,
}

pin_project! {
    /// Instruments only the boundary where Hyper polls the response body.
    ///
    /// Full attribution requires h2/socket-level instrumentation of
    /// `poll_capacity`, frame queueing, and write timestamps. That deeper
    /// instrumentation is deliberately out of scope here.
    struct InstrumentedBody<B> {
        #[pin]
        inner: B,
        metrics: GrpcBodyMetricHandles,
        created_at: Instant,
        previous_poll: Option<Instant>,
        last_data_poll: Option<Instant>,
    }
}

impl<B> InstrumentedBody<B> {
    fn new(inner: B, metrics: GrpcBodyMetricHandles) -> Self {
        Self {
            inner,
            metrics,
            created_at: Instant::now(),
            previous_poll: None,
            last_data_poll: None,
        }
    }
}

impl<B> Body for InstrumentedBody<B>
where
    B: Body,
{
    type Data = B::Data;
    type Error = B::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let mut this = self.project();
        let poll_started = Instant::now();
        if let Some(previous_poll) = this.previous_poll.replace(poll_started) {
            this.metrics
                .poll_gap
                .observe(poll_started.duration_since(previous_poll).as_secs_f64());
        } else {
            this.metrics
                .first_poll
                .observe(poll_started.duration_since(*this.created_at).as_secs_f64());
        }

        let result = this.inner.as_mut().poll_frame(cx);
        match &result {
            Poll::Pending => this.metrics.pending.inc(),
            Poll::Ready(Some(Ok(frame))) if frame.is_data() => {
                this.metrics.data.inc();
                this.last_data_poll.replace(Instant::now());
            }
            Poll::Ready(Some(Ok(frame))) if frame.is_trailers() => {
                this.metrics.trailers.inc();
                if let Some(last_data_poll) = this.last_data_poll.as_ref() {
                    this.metrics
                        .trailers_gap
                        .observe(last_data_poll.elapsed().as_secs_f64());
                }
            }
            _ => {}
        }
        result
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.inner.size_hint()
    }

}

#[cfg(target_os = "linux")]
fn configured_chunk_schedstat_sample_every() -> Option<u64> {
    use std::sync::LazyLock;

    static SAMPLE_EVERY: LazyLock<Option<u64>> =
        LazyLock::new(|| match std::env::var("SUI_RPC_CHUNK_SCHEDSTAT") {
            Ok(value) => match value.parse::<u64>() {
                Ok(sample_every) if sample_every > 0 => Some(sample_every),
                _ => {
                    tracing::warn!(
                        value,
                        "SUI_RPC_CHUNK_SCHEDSTAT must be a positive integer; schedstat sampling disabled"
                    );
                    None
                }
            },
            Err(std::env::VarError::NotPresent) => None,
            Err(error) => {
                tracing::warn!(%error, "unable to read SUI_RPC_CHUNK_SCHEDSTAT; schedstat sampling disabled");
                None
            }
        });
    *SAMPLE_EVERY
}

#[cfg(not(target_os = "linux"))]
fn configured_chunk_schedstat_sample_every() -> Option<u64> {
    None
}

#[derive(Clone)]
pub(crate) struct ListApiMetrics {
    list_first_frame_seconds: HistogramVec,
    list_response_page_bytes: HistogramVec,
    list_watermark_frames_total: IntCounterVec,
    list_stream_yield_wait_seconds: HistogramVec,
    list_render_seconds: HistogramVec,
    list_chunk_seconds: HistogramVec,
    list_chunk_work_cpu_seconds: HistogramVec,
    list_chunk_run_delay_seconds: HistogramVec,
    list_chunk_schedstat_probes_total: IntCounterVec,
    list_chunk_buckets_decoded_total: IntCounterVec,
    list_store_read_batches_total: IntCounterVec,
    list_store_read_keys_total: IntCounterVec,
    list_object_cache_hits_total: IntCounterVec,
    list_query_ends_total: IntCounterVec,
    list_bitmap_buckets_evaluated: HistogramVec,
    chunk_schedstat_sample_every: Option<u64>,
    list_probes_enabled: bool,
}

impl ListApiMetrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        let list_probes_enabled = list_probes_enabled();
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
                "Consumer-driven repoll cadence: time from yielding one List response frame until the handler stream is polled again. It conflates h2 capacity/backpressure, runtime scheduling, and in-flight buildup; it does not measure producer-side blocking.",
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
                "Time in seconds for one blocking List chunk phase. queue spans immediately before spawn_blocking through entry into its closure; setup spans closure entry through scan positioning and reference discovery before response-item materialization; work spans the complete blocking chunk and includes chunks that return an error; read spans chunk-level batched store reads within work.",
                &["method", "phase"],
                prometheus::exponential_buckets(0.0001, 2.0, 20).unwrap(),
                registry,
            )
            .unwrap(),
            list_chunk_work_cpu_seconds: register_histogram_vec_with_registry!(
                "list_chunk_work_cpu_seconds",
                "Thread CPU time consumed by one blocking List chunk worker. Compare with the work wall phase: both increasing identifies slower store or computation, while wall increasing with flat CPU identifies descheduling or blocking.",
                &["method"],
                prometheus::exponential_buckets(0.00001, 2.0, 24).unwrap(),
                registry,
            )
            .unwrap(),
            list_chunk_run_delay_seconds: register_histogram_vec_with_registry!(
                "list_chunk_run_delay_seconds",
                "Linux scheduler run-delay delta for a sampled blocking List chunk: time runnable but not running, separating OS descheduling from I/O or lock blocking. Requires kernel.sched_schedstats=1; sampling is disabled and no samples are emitted when schedstat fields read as zero.",
                &["method"],
                prometheus::exponential_buckets(0.00001, 2.0, 24).unwrap(),
                registry,
            )
            .unwrap(),
            list_chunk_schedstat_probes_total: register_int_counter_vec_with_registry!(
                "list_chunk_schedstat_probes_total",
                "Completed per-chunk /proc/thread-self/schedstat probe pairs. SUI_RPC_CHUNK_SCHEDSTAT=N enables one pair every Nth chunk per blocking thread, allowing run-delay sample density and probe overhead to be interpreted.",
                &["method"],
                registry,
            )
            .unwrap(),
            list_chunk_buckets_decoded_total: register_int_counter_vec_with_registry!(
                "list_chunk_buckets_decoded_total",
                "Bitmap buckets decoded by blocking List chunk workers. This identifies per-chunk fixed scan costs and sparse-filter amplification.",
                &["method"],
                registry,
            )
            .unwrap(),
            list_store_read_batches_total: register_int_counter_vec_with_registry!(
                "list_store_read_batches_total",
                "Batched store reads issued by List chunk workers, by read kind. Each data-bearing transactions chunk issues exactly one batch per kind; digest-only pages and empty chunks issue none.",
                &["method", "kind"],
                registry,
            )
            .unwrap(),
            list_store_read_keys_total: register_int_counter_vec_with_registry!(
                "list_store_read_keys_total",
                "Total keys requested across batched store reads issued by List chunk workers, by read kind.",
                &["method", "kind"],
                registry,
            )
            .unwrap(),
            list_object_cache_hits_total: register_int_counter_vec_with_registry!(
                "list_object_cache_hits_total",
                "Object keys served from the request-scoped object cache instead of a batched store read.",
                &["method"],
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
            chunk_schedstat_sample_every: list_probes_enabled
                .then(configured_chunk_schedstat_sample_every)
                .flatten(),
            list_probes_enabled,
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
            chunk_read: self.list_chunk_seconds.with_label_values(&[method, "read"]),
            chunk_setup: self
                .list_probes_enabled
                .then(|| self.list_chunk_seconds.with_label_values(&[method, "setup"])),
            chunk_work_cpu: self
                .list_probes_enabled
                .then(|| self.list_chunk_work_cpu_seconds.with_label_values(&[method])),
            chunk_run_delay: self
                .list_probes_enabled
                .then(|| self.list_chunk_run_delay_seconds.with_label_values(&[method])),
            chunk_schedstat_probes: self.list_probes_enabled.then(|| {
                self.list_chunk_schedstat_probes_total
                    .with_label_values(&[method])
            }),
            chunk_buckets_decoded: self.list_probes_enabled.then(|| {
                self.list_chunk_buckets_decoded_total
                    .with_label_values(&[method])
            }),
            chunk_schedstat_sample_every: self.chunk_schedstat_sample_every,
            list_probes_enabled: self.list_probes_enabled,
            store_read_batches: self.list_store_read_batches_total.clone(),
            store_read_keys: self.list_store_read_keys_total.clone(),
            object_cache_hits: self.list_object_cache_hits_total.clone(),
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
    type RequestHandler = ();
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

        (
            (),
            RpcMetricsCallbackHandler {
                metrics,
                path,
                start,
                counted_response: false,
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

pub struct RpcMetricsCallbackHandler {
    metrics: Arc<RpcMetrics>,
    path: Cow<'static, str>,
    start: Instant,
    // Indicates if we successfully counted the response. In some cases when a request is
    // prematurely canceled this will remain false
    counted_response: bool,
    counted_first_chunk: bool,
}

impl ResponseHandler for RpcMetricsCallbackHandler {
    fn on_response(&mut self, response: &http::response::Parts) {
        const GRPC_STATUS: http::HeaderName = http::HeaderName::from_static("grpc-status");

        // Unlike `request_latency` (observed in `Drop`, after the response
        // body finished streaming), this fires as soon as the handler
        // produced a response, so it excludes client-side network latency.
        self.metrics
            .request_handler_latency
            .with_label_values(&[self.path.as_ref()])
            .observe(self.start.elapsed().as_secs_f64());

        let status = if response
            .headers
            .get(&http::header::CONTENT_TYPE)
            .is_some_and(is_grpc_content_type)
        {
            let code = response
                .headers
                .get(&GRPC_STATUS)
                .map(http::HeaderValue::as_bytes)
                .map(tonic::Code::from_bytes)
                .unwrap_or(tonic::Code::Ok);

            code_as_str(code)
        } else {
            response.status.as_str()
        };

        self.metrics
            .num_requests
            .with_label_values(&[self.path.as_ref(), status])
            .inc();

        self.counted_response = true;
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

        if !self.counted_response {
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
    chunk_setup: Option<Histogram>,
    chunk_work: Histogram,
    chunk_read: Histogram,
    chunk_work_cpu: Option<Histogram>,
    chunk_run_delay: Option<Histogram>,
    chunk_schedstat_probes: Option<IntCounter>,
    chunk_buckets_decoded: Option<IntCounter>,
    chunk_schedstat_sample_every: Option<u64>,
    list_probes_enabled: bool,
    store_read_batches: IntCounterVec,
    store_read_keys: IntCounterVec,
    object_cache_hits: IntCounterVec,
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

    pub(crate) fn start_setup_timer(&self) -> ListChunkSetupTimer {
        ListChunkSetupTimer {
            timer: self.chunk_setup.as_ref().map(Histogram::start_timer),
        }
    }

    pub(crate) fn observe_chunk_work(&self, elapsed: Duration) {
        self.chunk_work.observe(elapsed.as_secs_f64());
    }

    pub(crate) fn observe_chunk_work_cpu(&self, elapsed: Duration) {
        if let Some(chunk_work_cpu) = &self.chunk_work_cpu {
            chunk_work_cpu.observe(elapsed.as_secs_f64());
        }
    }

    pub(crate) fn list_probes_enabled(&self) -> bool {
        self.list_probes_enabled
    }

    pub(crate) fn schedstat_sample_every(&self) -> Option<u64> {
        self.chunk_schedstat_sample_every
    }

    pub(crate) fn observe_chunk_run_delay(&self, elapsed: Duration) {
        if let (Some(chunk_run_delay), Some(chunk_schedstat_probes)) =
            (&self.chunk_run_delay, &self.chunk_schedstat_probes)
        {
            chunk_run_delay.observe(elapsed.as_secs_f64());
            chunk_schedstat_probes.inc();
        }
    }

    pub(crate) fn observe_chunk_buckets_decoded(&self, buckets: usize) {
        if let Some(chunk_buckets_decoded) = &self.chunk_buckets_decoded {
            chunk_buckets_decoded.inc_by(buckets as u64);
        }
    }

    pub(crate) fn observe_chunk_read(&self, elapsed: Duration) {
        self.chunk_read.observe(elapsed.as_secs_f64());
    }

    pub(crate) fn observe_store_read_batch(&self, kind: &'static str, keys: usize) {
        self.store_read_batches
            .with_label_values(&[self.method, kind])
            .inc();
        self.store_read_keys
            .with_label_values(&[self.method, kind])
            .inc_by(keys as u64);
    }

    pub(crate) fn observe_object_cache_hits(&self, keys: usize) {
        self.object_cache_hits
            .with_label_values(&[self.method])
            .inc_by(keys as u64);
    }
}

pub(crate) struct ListChunkSetupTimer {
    timer: Option<HistogramTimer>,
}

impl ListChunkSetupTimer {
    pub(crate) fn disabled() -> Self {
        Self { timer: None }
    }

    pub(crate) fn finish_setup(&mut self) {
        if let Some(timer) = self.timer.take() {
            timer.stop_and_record();
        }
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
                "Total progress-only response frames emitted by gRPC subscriptions, including initial filtered-subscription start frames, by type.",
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
    use std::collections::BTreeSet;

    use prost_types::{
        FileDescriptorProto, FileDescriptorSet, MethodDescriptorProto, ServiceDescriptorProto,
    };
    use sui_rpc::proto::sui::rpc::v2::{
        ListTransactionsResponse, QueryEnd, SubscribeCheckpointsResponse, SubscribeEventsResponse,
        SubscribeTransactionsResponse, Watermark,
    };

    #[test]
    fn parses_list_probes_values() {
        for (value, expected) in [
            (None, true),
            (Some("1"), true),
            (Some("junk"), true),
            (Some("0"), false),
            (Some("off"), false),
        ] {
            assert_eq!(parse_list_probes(value), expected, "{value:?}");
        }
    }

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
        let ((), handler) = make.make_handler(&parts);
        handler
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
                        ["queue", "setup", "work", "read"]
                            .into_iter()
                            .map(move |phase| vec![("method", method), ("phase", phase)])
                    })
                    .collect(),
            ),
        );
        let method_labels = expected_label_sets(
            methods
                .into_iter()
                .map(|method| vec![("method", method)])
                .collect(),
        );
        for name in [
            "list_chunk_work_cpu_seconds",
            "list_chunk_run_delay_seconds",
            "list_chunk_schedstat_probes_total",
            "list_chunk_buckets_decoded_total",
        ] {
            assert_metric_family(&families, name, method_labels.clone());
        }
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
    fn list_store_read_metrics_are_lazy_and_observe_exact_values() {
        let registry = Registry::new();
        let metrics = ListApiMetrics::new(&registry);

        metrics.stream_metrics("list_checkpoints", "summary");
        metrics.stream_metrics("list_events", "json");
        let families = registry.gather();
        for name in [
            "list_store_read_batches_total",
            "list_store_read_keys_total",
            "list_object_cache_hits_total",
        ] {
            assert!(
                families.iter().all(|family| family.name() != name),
                "{name} must not create series when stream metrics are constructed"
            );
        }

        let handles = metrics.stream_metrics("list_transactions", "full_objects");
        handles.observe_chunk_read(Duration::from_secs(2));
        handles.observe_store_read_batch("checkpoint_summaries", 7);
        handles.observe_object_cache_hits(5);

        assert_eq!(handles.chunk_read.get_sample_count(), 1);
        assert_eq!(handles.chunk_read.get_sample_sum(), 2.0);

        let families = registry.gather();
        let expected_labels = expected_label_sets(vec![vec![
            ("method", "list_transactions"),
            ("kind", "checkpoint_summaries"),
        ]]);
        assert_metric_family(
            &families,
            "list_store_read_batches_total",
            expected_labels.clone(),
        );
        assert_metric_family(&families, "list_store_read_keys_total", expected_labels);
        assert_metric_family(
            &families,
            "list_object_cache_hits_total",
            expected_label_sets(vec![vec![("method", "list_transactions")]]),
        );
        assert_eq!(
            metrics
                .list_store_read_batches_total
                .with_label_values(&["list_transactions", "checkpoint_summaries"])
                .get(),
            1
        );
        assert_eq!(
            metrics
                .list_store_read_keys_total
                .with_label_values(&["list_transactions", "checkpoint_summaries"])
                .get(),
            7
        );
        assert_eq!(
            metrics
                .list_object_cache_hits_total
                .with_label_values(&["list_transactions"])
                .get(),
            5
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
}
