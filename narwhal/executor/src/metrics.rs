// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use prometheus::{
    default_registry, register_histogram_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, Histogram, IntCounter, IntGauge, Registry,
};

#[derive(Clone, Debug)]
pub struct ExecutorMetrics {
    /// occupancy of the channel from the `Subscriber` to `Notifier`
    pub tx_notifier: IntGauge,
    /// Time it takes to download a payload from local worker peer
    pub subscriber_local_fetch_latency: Histogram,
    /// Time it takes to download a payload from remote peer
    pub subscriber_remote_fetch_latency: Histogram,
    /// Number of times certificate was found locally
    pub subscriber_local_hit: IntCounter,
    /// Number of batches processed by notifier
    pub notifier_processed_batches: IntCounter,
    /// Number of bytes processed by notifier
    pub notifier_processed_bytes: IntCounter,
    /// Number of certificates processed by subscriber
    pub subscriber_processed_certificates: IntCounter,
    /// Round of last certificate seen by subscriber
    pub subscriber_current_round: IntGauge,
    /// The number of certificates processed by Subscriber
    /// during the recovery period to fetch their payloads.
    pub subscriber_recovered_certificates_count: IntCounter,
    /// The number of pending remote calls to request_batch
    pub pending_remote_request_batch: IntGauge,
    /// The number of pending payload downloads
    pub waiting_elements_subscriber: IntGauge,
}

impl ExecutorMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            tx_notifier: register_int_gauge_with_registry!(
                "tx_notifier",
                "occupancy of the channel from the `Subscriber` to `Notifier`",
                registry
            )
            .unwrap(),
            subscriber_local_fetch_latency: register_histogram_with_registry!(
                "subscriber_local_fetch_latency",
                "Time it takes to download a payload from local worker peer",
                // the buckets defined in seconds
                vec![
                    0.005, 0.01, 0.02, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 3.0, 5.0, 10.0, 20.0, 40.0,
                    60.0
                ],
                registry
            )
            .unwrap(),
            subscriber_remote_fetch_latency: register_histogram_with_registry!(
                "subscriber_remote_fetch_latency",
                "Time it takes to download a payload from remote worker peer",
                // the buckets defined in seconds
                vec![
                    0.005, 0.01, 0.02, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 3.0, 5.0, 10.0, 20.0, 40.0,
                    60.0
                ],
                registry
            )
            .unwrap(),
            subscriber_recovered_certificates_count: register_int_counter_with_registry!(
                "subscriber_recovered_certificates_count",
                "The number of certificates processed by Subscriber during the recovery period to fetch their payloads",
                registry
            ).unwrap(),
            subscriber_local_hit: register_int_counter_with_registry!(
                "subscriber_local_hit",
                "Number of times certificate was found locally",
                registry
            ).unwrap(),
            notifier_processed_batches: register_int_counter_with_registry!(
                "notifier_processed_batches",
                "Number of batches processed by notifier",
                registry
            ).unwrap(),
            notifier_processed_bytes: register_int_counter_with_registry!(
                "notifier_processed_bytes",
                "Number of bytes processed by notifier",
                registry
            ).unwrap(),
            subscriber_processed_certificates: register_int_counter_with_registry!(
                "subscriber_processed_certificates",
                "Number of certificates processed by subscriber",
                registry
            ).unwrap(),
            subscriber_current_round: register_int_gauge_with_registry!(
                "subscriber_current_round",
                "Round of last certificate seen by subscriber",
                registry
            ).unwrap(),
            pending_remote_request_batch: register_int_gauge_with_registry!(
                "pending_remote_request_batch",
                "The number of pending remote calls to request_batch",
                registry
            ).unwrap(),
            waiting_elements_subscriber: register_int_gauge_with_registry!(
                "waiting_elements_subscriber",
                "The number of pending payload downloads",
                registry
            ).unwrap(),
        }
    }
}

impl Default for ExecutorMetrics {
    fn default() -> Self {
        Self::new(default_registry())
    }
}
