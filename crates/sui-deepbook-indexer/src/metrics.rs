// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_int_counter_with_registry, register_int_gauge_with_registry, IntCounter, IntGauge,
    Registry,
};

#[derive(Clone, Debug)]
pub struct DeepBookIndexerMetrics {
    pub(crate) total_deepbook_transactions: IntCounter,
}

impl DeepBookIndexerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_deepbook_transactions: register_int_counter_with_registry!(
                "total_deepbook_transactions",
                "Total number of deepbook transactions",
                registry,
            )
            .unwrap(),
        }
    }

    pub fn new_for_testing() -> Self {
        let registry = Registry::new();
        Self::new(&registry)
    }
}
