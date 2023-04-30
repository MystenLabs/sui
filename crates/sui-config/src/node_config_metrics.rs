// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::NodeConfig;
use prometheus::{register_int_gauge_with_registry, IntGauge, Registry};
use std::sync::Arc;

pub struct NodeConfigMetrics {
    tx_deny_config_user_transaction_disabled: IntGauge,
    tx_deny_config_shared_object_disabled: IntGauge,
    tx_deny_config_package_publish_disabled: IntGauge,
    tx_deny_config_package_upgrade_disabled: IntGauge,
    tx_deny_config_num_denied_objects: IntGauge,
    tx_deny_config_num_denied_packages: IntGauge,
    tx_deny_config_num_denied_addresses: IntGauge,
}

impl NodeConfigMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        let this = Self {
            tx_deny_config_user_transaction_disabled: register_int_gauge_with_registry!(
                "tx_deny_config_user_transaction_disabled",
                "Whether all user transactions are disabled",
                registry
            )
            .unwrap(),
            tx_deny_config_shared_object_disabled: register_int_gauge_with_registry!(
                "tx_deny_config_shared_object_disabled",
                "Whether all shared object transactions are disabled",
                registry
            )
            .unwrap(),
            tx_deny_config_package_publish_disabled: register_int_gauge_with_registry!(
                "tx_deny_config_package_publish_disabled",
                "Whether all package publish transactions are disabled",
                registry
            )
            .unwrap(),
            tx_deny_config_package_upgrade_disabled: register_int_gauge_with_registry!(
                "tx_deny_config_package_upgrade_disabled",
                "Whether all package upgrade transactions are disabled",
                registry
            )
            .unwrap(),
            tx_deny_config_num_denied_objects: register_int_gauge_with_registry!(
                "tx_deny_config_num_denied_objects",
                "Number of denied objects",
                registry
            )
            .unwrap(),
            tx_deny_config_num_denied_packages: register_int_gauge_with_registry!(
                "tx_deny_config_num_denied_packages",
                "Number of denied packages",
                registry
            )
            .unwrap(),
            tx_deny_config_num_denied_addresses: register_int_gauge_with_registry!(
                "tx_deny_config_num_denied_addresses",
                "Number of denied addresses",
                registry
            )
            .unwrap(),
        };
        Arc::new(this)
    }

    pub fn record_metrics(&self, config: &NodeConfig) {
        self.tx_deny_config_user_transaction_disabled
            .set(config.transaction_deny_config.user_transaction_disabled() as i64);
        self.tx_deny_config_shared_object_disabled
            .set(config.transaction_deny_config.shared_object_disabled() as i64);
        self.tx_deny_config_package_publish_disabled
            .set(config.transaction_deny_config.package_publish_disabled() as i64);
        self.tx_deny_config_package_upgrade_disabled
            .set(config.transaction_deny_config.package_upgrade_disabled() as i64);
        self.tx_deny_config_num_denied_objects
            .set(config.transaction_deny_config.get_object_deny_set().len() as i64);
        self.tx_deny_config_num_denied_packages
            .set(config.transaction_deny_config.get_package_deny_set().len() as i64);
        self.tx_deny_config_num_denied_addresses
            .set(config.transaction_deny_config.get_address_deny_set().len() as i64);
    }
}
