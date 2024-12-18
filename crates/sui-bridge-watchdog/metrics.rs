// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{register_int_gauge_with_registry, IntGauge, Registry};

#[derive(Clone, Debug)]
pub struct WatchdogMetrics {
    pub eth_vault_balance: IntGauge,
    pub eth_bridge_paused: IntGauge,
    pub sui_bridge_paused: IntGauge,
}

impl WatchdogMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            eth_vault_balance: register_int_gauge_with_registry!(
                "bridge_eth_vault_balance",
                "Current balance of eth vault",
                registry,
            )
            .unwrap(),
            eth_bridge_paused: register_int_gauge_with_registry!(
                "bridge_eth_bridge_paused",
                "Whether the eth bridge is paused",
                registry,
            )
            .unwrap(),
            sui_bridge_paused: register_int_gauge_with_registry!(
                "bridge_sui_bridge_paused",
                "Whether the sui bridge is paused",
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
