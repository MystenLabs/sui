// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use prometheus::{default_registry, register_int_gauge_with_registry, IntGauge, Registry};

#[derive(Clone, Debug)]
pub struct ExecutorMetrics {
    /// occupancy of the channel from the `Subscriber` to `BatchLoader`
    pub tx_batch_loader: IntGauge,
    /// occupancy of the channel from the `Subscriber` to `Core`
    pub tx_executor: IntGauge,
}

impl ExecutorMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            tx_batch_loader: register_int_gauge_with_registry!(
                "tx_batch_loader",
                "occupancy of the channel from the `Subscriber` to `BatchLoader`",
                registry
            )
            .unwrap(),
            tx_executor: register_int_gauge_with_registry!(
                "tx_executor",
                "occupancy of the channel from the `Subscriber` to `Core`",
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
