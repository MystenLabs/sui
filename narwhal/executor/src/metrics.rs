// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use prometheus::{
    default_registry, register_histogram_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, Histogram, IntCounter, IntGauge, Registry,
};

#[derive(Clone, Debug)]
pub struct ExecutorMetrics {
    /// occupancy of the channel from the `Subscriber` to `Core`
    pub tx_executor: IntGauge,
    /// Time it takes to download a payload on the Subscriber
    pub subscriber_download_payload_latency: Histogram,
    /// The number of attempts to successfully download
    /// a certificate's payload in Subscriber.
    pub subscriber_download_payload_attempts: Histogram,
    /// The number of certificates processed by Subscriber
    /// during the recovery period to fetch their payloads.
    pub subscriber_recovered_certificates_count: IntCounter,
}

impl ExecutorMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            tx_executor: register_int_gauge_with_registry!(
                "tx_executor",
                "occupancy of the channel from the `Subscriber` to `Core`",
                registry
            )
            .unwrap(),
            subscriber_download_payload_latency: register_histogram_with_registry!(
                "subscriber_download_payload_latency",
                "Time it takes to download a payload on the Subscriber",
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
            subscriber_download_payload_attempts: register_histogram_with_registry!(
                "subscriber_download_payload_attempts",
                "The number of attempts to successfully download a certificate's payload in Subscriber",
                vec![
                    1.0, 2.0, 3.0, 4.0, 5.0, 7.0, 10.0, 15.0, 20.0
                ],
                registry
            ).unwrap()
        }
    }
}

impl Default for ExecutorMetrics {
    fn default() -> Self {
        Self::new(default_registry())
    }
}
