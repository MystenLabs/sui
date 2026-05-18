// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::http;
use std::{borrow::Cow, collections::HashSet, sync::Arc, time::Instant};

use mysten_network::callback::{MakeCallbackHandler, ResponseHandler};
use prometheus::{
    HistogramVec, IntCounterVec, IntGauge, IntGaugeVec, Registry,
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry,
};
use prost::Message;

#[derive(Clone)]
pub struct RpcMetrics {
    inflight_requests: IntGaugeVec,
    num_requests: IntCounterVec,
    request_latency: HistogramVec,
}

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
                "Latency of RPC requests per route",
                &["path"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
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
    type Handler = RpcMetricsCallbackHandler;

    fn make_handler(&self, request: &http::request::Parts) -> Self::Handler {
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

        RpcMetricsCallbackHandler {
            metrics,
            path,
            start,
            counted_response: false,
        }
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
}

impl ResponseHandler for RpcMetricsCallbackHandler {
    fn on_response(&mut self, response: &http::response::Parts) {
        const GRPC_STATUS: http::HeaderName = http::HeaderName::from_static("grpc-status");

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

    fn on_error<E>(&mut self, _error: &E) {
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
pub(crate) struct SubscriptionMetrics {
    pub inflight_subscribers: IntGauge,
    pub last_recieved_checkpoint: IntGauge,
}

impl SubscriptionMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            inflight_subscribers: register_int_gauge_with_registry!(
                "subscription_inflight_subscribers",
                "Total in-flight subscriptions",
                registry,
            )
            .unwrap(),
            last_recieved_checkpoint: register_int_gauge_with_registry!(
                "subscription_last_recieved_checkpoint",
                "Last recieved checkpoint by the subscription service",
                registry,
            )
            .unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost_types::{
        FileDescriptorProto, FileDescriptorSet, MethodDescriptorProto, ServiceDescriptorProto,
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
        let alpha = fds("sui.rpc.alpha", &[("EventService", &["Subscribe"])]);

        let paths = grpc_method_paths_from_file_descriptor_sets(&[&v2, &alpha]).unwrap();

        assert_eq!(paths.len(), 3);
        assert!(paths.contains("/sui.rpc.v2.LedgerService/GetCheckpoint"));
        assert!(paths.contains("/sui.rpc.v2.LedgerService/GetTransaction"));
        assert!(paths.contains("/sui.rpc.alpha.EventService/Subscribe"));
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
        allowlist.insert("/sui.rpc.v2alpha.LedgerService/ListTransactions".to_owned());

        let label = compute_metric_label(
            true,
            "/sui.rpc.v2alpha.LedgerService/ListTransactions",
            None,
            &allowlist,
        );
        assert_eq!(label, "/sui.rpc.v2alpha.LedgerService/ListTransactions");
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
}
