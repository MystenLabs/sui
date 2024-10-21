// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_network::metrics::MetricsCallbackProvider;
use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
    register_int_gauge_vec_with_registry, register_int_gauge_with_registry, HistogramVec,
    IntCounterVec, IntGauge, IntGaugeVec, Registry,
};
use std::time::Duration;
use tonic::Code;

/// Prometheus metrics which can be displayed in Grafana, queried and alerted on

/// Metrics relevant to the requests coming into the service
#[derive(Clone, Debug)]
pub struct RequestMetrics {
    pub(crate) total_requests_received: IntCounterVec,
    pub(crate) total_requests_succeeded: IntCounterVec,
    pub(crate) total_requests_shed: IntCounterVec,
    pub(crate) total_requests_failed: IntCounterVec,
    pub(crate) total_requests_disconnected: IntCounterVec,
    pub(crate) current_requests_in_flight: IntGaugeVec,
    pub(crate) process_latency: HistogramVec,
}

/// Metrics relevant to the running of the service
#[derive(Clone, Debug)]
pub struct FaucetMetrics {
    pub(crate) balance: IntGauge,
    pub(crate) current_executions_in_flight: IntGauge,
    pub(crate) total_available_coins: IntGauge,
    pub(crate) total_discarded_coins: IntGauge,
    pub(crate) total_coin_requests_succeeded: IntGauge,
}

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1., 2.5, 5., 10., 20., 30., 60., 90.,
];

impl RequestMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_requests_received: register_int_counter_vec_with_registry!(
                "total_requests_received",
                "Total number of requests received in Faucet",
                &["path"],
                registry,
            )
            .unwrap(),
            total_requests_succeeded: register_int_counter_vec_with_registry!(
                "total_requests_succeeded",
                "Total number of requests processed successfully in Faucet",
                &["path"],
                registry,
            )
            .unwrap(),
            total_requests_shed: register_int_counter_vec_with_registry!(
                "total_requests_shed",
                "Total number of requests that were dropped because the service was saturated",
                &["path"],
                registry,
            )
            .unwrap(),
            total_requests_failed: register_int_counter_vec_with_registry!(
                "total_requests_failed",
                "Total number of requests that started but failed with an uncaught error",
                &["path"],
                registry,
            )
            .unwrap(),
            total_requests_disconnected: register_int_counter_vec_with_registry!(
                "total_requests_disconnected",
                "Total number of requests where the client disconnected before the service \
                 returned a response",
                &["path"],
                registry,
            )
            .unwrap(),
            current_requests_in_flight: register_int_gauge_vec_with_registry!(
                "current_requests_in_flight",
                "Current number of requests being processed in Faucet",
                &["path"],
                registry,
            )
            .unwrap(),
            process_latency: register_histogram_vec_with_registry!(
                "process_latency",
                "Latency of processing a Faucet request",
                &["path"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
        }
    }
}

impl FaucetMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            balance: register_int_gauge_with_registry!(
                "balance",
                "Current balance of the all the available coins",
                registry,
            ).unwrap(),
            current_executions_in_flight: register_int_gauge_with_registry!(
                "current_executions_in_flight",
                "Current number of transactions being executed in Faucet",
                registry,
            )
            .unwrap(),
            total_available_coins: register_int_gauge_with_registry!(
                "total_available_coins",
                "Total number of available coins in queue",
                registry,
            )
            .unwrap(),
            total_discarded_coins: register_int_gauge_with_registry!(
                "total_discarded_coins",
                "Total number of discarded coins",
                registry,
            )
            .unwrap(),
            total_coin_requests_succeeded: register_int_gauge_with_registry!(
                "total_coin_requests_succeeded",
                "Total number of requests processed successfully in Faucet (both batch and non_batched)",
                registry,
            )
            .unwrap(),
        }
    }
}

impl MetricsCallbackProvider for RequestMetrics {
    fn on_request(&self, path: String) {
        self.total_requests_received
            .with_label_values(&[path.as_str()])
            .inc();
    }

    fn on_response(&self, path: String, latency: Duration, _status: u16, grpc_status_code: Code) {
        self.process_latency
            .with_label_values(&[path.as_str()])
            .observe(latency.as_secs_f64());

        match grpc_status_code {
            Code::Ok => {
                self.total_requests_succeeded
                    .with_label_values(&[path.as_str()])
                    .inc();
            }
            Code::Unavailable | Code::ResourceExhausted => {
                self.total_requests_shed
                    .with_label_values(&[path.as_str()])
                    .inc();
            }
            _ => {
                self.total_requests_failed
                    .with_label_values(&[path.as_str()])
                    .inc();
            }
        }
    }

    fn on_start(&self, path: &str) {
        self.current_requests_in_flight
            .with_label_values(&[path])
            .inc();
    }

    fn on_drop(&self, path: &str) {
        self.total_requests_disconnected
            .with_label_values(&[path])
            .inc();
        self.current_requests_in_flight
            .with_label_values(&[path])
            .dec();
    }
}
