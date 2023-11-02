// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{register_histogram_vec_with_registry, HistogramVec, Registry};

pub const LATENCY_S: &str = "latency_s";
const LATENCY_SEC_BUCKETS: &[f64] = &[0.1, 0.25, 0.5, 0.75, 1., 1.25, 1.5, 2.0];

#[derive(Clone)]
pub struct Metrics {
    pub latency_s: HistogramVec,
}

impl Metrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            latency_s: register_histogram_vec_with_registry!(
                LATENCY_S,
                "Buckets measuring the end-to-end latency of a workload in seconds",
                &["workload"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
        }
    }
}
