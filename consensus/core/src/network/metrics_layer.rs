// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prometheus::HistogramTimer;

use super::metrics::NetworkRouteMetrics;

/// Tower layer adapters that allow specifying callbacks for request and response handling
/// exist for both anemo and http. So the metrics layer implementation can be reused across
/// networking stacks.

pub(crate) trait SizedRequest {
    fn size(&self) -> usize;
    fn route(&self) -> String;
}

pub(crate) trait SizedResponse {
    fn size(&self) -> usize;
    fn error_type(&self) -> Option<String>;
}

#[derive(Clone)]
pub(crate) struct MetricsCallbackMaker {
    metrics: Arc<NetworkRouteMetrics>,
    /// Size in bytes above which a request or response message is considered excessively large
    excessive_message_size: usize,
}

impl MetricsCallbackMaker {
    pub(crate) fn new(metrics: Arc<NetworkRouteMetrics>, excessive_message_size: usize) -> Self {
        Self {
            metrics,
            excessive_message_size,
        }
    }

    // Update request metrics. And create a callback that should be called on response.
    pub(crate) fn handle_request(&self, request: &dyn SizedRequest) -> MetricsResponseCallback {
        let route = request.route();

        self.metrics.requests.with_label_values(&[&route]).inc();
        self.metrics
            .inflight_requests
            .with_label_values(&[&route])
            .inc();
        let request_size = request.size();
        if request_size > 0 {
            self.metrics
                .request_size
                .with_label_values(&[&route])
                .observe(request_size as f64);
        }
        if request_size > self.excessive_message_size {
            self.metrics
                .excessive_size_requests
                .with_label_values(&[&route])
                .inc();
        }

        let timer = self
            .metrics
            .request_latency
            .with_label_values(&[&route])
            .start_timer();

        MetricsResponseCallback {
            metrics: self.metrics.clone(),
            timer,
            route,
            excessive_message_size: self.excessive_message_size,
        }
    }
}

pub(crate) struct MetricsResponseCallback {
    metrics: Arc<NetworkRouteMetrics>,
    // The timer is held on to and "observed" once dropped
    #[allow(unused)]
    timer: HistogramTimer,
    route: String,
    excessive_message_size: usize,
}

impl MetricsResponseCallback {
    // Update response metrics.
    pub(crate) fn on_response(&mut self, response: &dyn SizedResponse) {
        let response_size = response.size();
        if response_size > 0 {
            self.metrics
                .response_size
                .with_label_values(&[&self.route])
                .observe(response_size as f64);
        }
        if response_size > self.excessive_message_size {
            self.metrics
                .excessive_size_responses
                .with_label_values(&[&self.route])
                .inc();
        }

        if let Some(err) = response.error_type() {
            self.metrics
                .errors
                .with_label_values(&[&self.route, &err])
                .inc();
        }
    }

    pub(crate) fn on_error<E>(&mut self, _error: &E) {
        self.metrics
            .errors
            .with_label_values(&[&self.route, "unknown"])
            .inc();
    }
}

impl Drop for MetricsResponseCallback {
    fn drop(&mut self) {
        self.metrics
            .inflight_requests
            .with_label_values(&[&self.route])
            .dec();
    }
}
