use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry, HistogramVec,
    IntCounterVec, Registry,
};

#[derive(Clone)]
pub struct AppMetrics {
    pub requests_total: IntCounterVec,
    pub request_latency: HistogramVec,
}

impl AppMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            requests_total: register_int_counter_vec_with_registry!(
                "edge_proxy_requests_total",
                "Total number of requests processed by the edge proxy",
                &["peer_type", "status"],
                registry
            )
            .unwrap(),
            request_latency: register_histogram_vec_with_registry!(
                "edge_proxy_request_latency",
                "Request latency in seconds",
                &["peer_type"],
                registry
            )
            .unwrap(),
        }
    }
}
