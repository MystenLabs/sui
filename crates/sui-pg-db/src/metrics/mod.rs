// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::IntCounter;
use prometheus::Registry;
use prometheus::register_int_counter_with_registry;
use std::sync::Arc;

pub struct PoolMetrics {
    pub requested: IntCounter,
    pub acquired: IntCounter,
    pub unacquired_error: IntCounter,
    pub unacquired_canceled: IntCounter,
}

impl PoolMetrics {
    pub fn new(prefix: Option<&str>, registry: &Registry) -> anyhow::Result<Arc<Self>> {
        let prefix = prefix.unwrap_or("db");
        let name = |n| format!("{prefix}_{n}");
        let pool_metrics = PoolMetrics {
            requested: register_int_counter_with_registry!(
                name("connections_requested"),
                "Total requested connections from the pool",
                registry,
            )?,
            acquired: register_int_counter_with_registry!(
                name("connections_acquired"),
                "Total requested connections from the pool that were acquired",
                registry,
            )?,
            unacquired_error: register_int_counter_with_registry!(
                name("connections_unacquired_error"),
                "Total requested connections from the pool that were not acquired due to an error",
                registry,
            )?,
            unacquired_canceled: register_int_counter_with_registry!(
                name("connections_unacquired_canceled"),
                "Total requested connections from the pool that were not acquired due to task cancellation",
                registry,
            )?,
        };
        Ok(Arc::new(pool_metrics))
    }
}
