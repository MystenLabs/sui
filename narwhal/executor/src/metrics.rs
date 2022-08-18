// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use prometheus::{default_registry, register_int_gauge_with_registry, IntGauge, Registry};

#[derive(Clone, Debug)]
pub struct ExecutorMetrics {
    /// occupancy of the channel from the `Subscriber` to `Core`
    pub tx_executor: IntGauge,
    /// Number of elements in the waiting (ready-to-deliver) list of subscriber
    pub waiting_elements_subscriber: IntGauge,
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
            waiting_elements_subscriber: register_int_gauge_with_registry!(
                "waiting_elements_subscriber",
                "Number of waiting elements in the subscriber",
                registry
            )
            .unwrap(),
        }
    }
}

impl Default for ExecutorMetrics {
    fn default() -> Self {
        Self::new(default_registry())
    }
}
