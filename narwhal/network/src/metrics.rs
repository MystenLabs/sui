// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use prometheus::{default_registry, register_int_gauge_vec_with_registry, IntGaugeVec, Registry};
use std::sync::Arc;

pub trait NetworkMetrics {
    fn network_available_tasks(&self) -> &IntGaugeVec;
}

#[derive(Clone, Debug)]
pub struct PrimaryNetworkMetrics {
    /// The number of executor available tasks
    pub network_available_tasks: IntGaugeVec,
}

impl PrimaryNetworkMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            network_available_tasks: register_int_gauge_vec_with_registry!(
                "primary_network_available_tasks",
                "The number of available tasks to run in the network connector",
                &["module", "network", "address"],
                registry
            )
            .unwrap(),
        }
    }
}

impl NetworkMetrics for PrimaryNetworkMetrics {
    fn network_available_tasks(&self) -> &IntGaugeVec {
        &self.network_available_tasks
    }
}

impl Default for PrimaryNetworkMetrics {
    fn default() -> Self {
        Self::new(default_registry())
    }
}

#[derive(Clone, Debug)]
pub struct WorkerNetworkMetrics {
    /// The number of executor available tasks
    pub network_available_tasks: IntGaugeVec,
}

impl WorkerNetworkMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            network_available_tasks: register_int_gauge_vec_with_registry!(
                "worker_network_concurrent_tasks",
                "The number of available tasks to run in the network connector",
                &["module", "network", "address"],
                registry
            )
            .unwrap(),
        }
    }
}

impl NetworkMetrics for WorkerNetworkMetrics {
    fn network_available_tasks(&self) -> &IntGaugeVec {
        &self.network_available_tasks
    }
}

impl Default for WorkerNetworkMetrics {
    fn default() -> Self {
        Self::new(default_registry())
    }
}

pub struct Metrics<N: NetworkMetrics> {
    /// The handler to report the metrics.
    metrics_handler: Arc<N>,
    /// A tag used for every reported metric to trace the module where this has been used from
    module_tag: String,
    /// The network type - this shouldn't be set outside of this crate but is meant to
    /// be initialised from the network modules.
    network_type: String,
}

impl<N: NetworkMetrics> Metrics<N> {
    pub fn new(metrics_handler: Arc<N>, module_tag: String) -> Self {
        Self {
            metrics_handler,
            module_tag,
            network_type: "".to_string(),
        }
    }

    pub fn from(metrics: Metrics<N>, network_type: String) -> Metrics<N> {
        Metrics {
            metrics_handler: metrics.metrics_handler,
            module_tag: metrics.module_tag,
            network_type,
        }
    }

    pub fn module_tag(&self) -> String {
        self.module_tag.clone()
    }

    pub fn network_type(&self) -> String {
        self.network_type.clone()
    }

    pub fn set_network_available_tasks(&self, value: i64, addr: Option<String>) {
        self.metrics_handler
            .network_available_tasks()
            .with_label_values(&[
                self.module_tag.as_str(),
                self.network_type.as_str(),
                addr.map_or("".to_string(), |a| a).as_str(),
            ])
            .set(value);
    }
}

#[cfg(test)]
mod test {
    use crate::metrics::{Metrics, NetworkMetrics, PrimaryNetworkMetrics};
    use prometheus::Registry;
    use std::{collections::HashMap, sync::Arc};

    #[test]
    fn test_called_metrics() {
        // GIVEN
        let registry = Registry::new();
        let metrics = Metrics {
            metrics_handler: Arc::new(PrimaryNetworkMetrics::new(&registry)),
            module_tag: "demo_handler".to_string(),
            network_type: "primary".to_string(),
        };

        // WHEN update metrics
        metrics.set_network_available_tasks(14, Some("127.0.0.1".to_string()));

        // THEN registry should be updated with expected tag
        let mut m = HashMap::new();
        m.insert("module", "demo_handler");
        m.insert("network", "primary");
        m.insert("address", "127.0.0.1");
        assert_eq!(
            metrics
                .metrics_handler
                .network_available_tasks()
                .get_metric_with(&m)
                .unwrap()
                .get(),
            14
        );
    }
}
