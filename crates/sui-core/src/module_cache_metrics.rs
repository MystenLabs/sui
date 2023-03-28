// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{register_int_gauge_with_registry, IntGauge, Registry};

pub struct ResolverMetrics {
    /// Track the size of the module cache.
    pub module_cache_size: IntGauge,
}

impl ResolverMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            module_cache_size: register_int_gauge_with_registry!(
                "module_cache_size",
                "Number of compiled move modules in the authority's cache.",
                registry
            )
            .unwrap(),
        }
    }
}
