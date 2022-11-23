// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Weak};

use move_bytecode_utils::module_cache::SyncModuleCache;
use move_core_types::resolver::ModuleResolver;
use prometheus::{
    core::{Collector, Desc},
    labels,
    proto::{Gauge, Metric, MetricFamily, MetricType},
};

/// Holds the module cache to collect its size and pass it to Prometheus, for monitoring in Grafana.
pub struct ModuleCacheGauge<R: ModuleResolver> {
    desc: Desc,
    module_cache: Weak<SyncModuleCache<R>>,
}

impl<R: ModuleResolver> ModuleCacheGauge<R> {
    pub fn new(module_cache: &Arc<SyncModuleCache<R>>) -> Self {
        Self {
            desc: Desc::new(
                "module_cache_size".into(),
                "Number of compiled move modules in the authority's cache.".into(),
                /* variable_labels */ vec![],
                /* const_labels */ labels! {},
            )
            .unwrap(),
            module_cache: Arc::downgrade(module_cache),
        }
    }

    fn metric(&self) -> Option<Metric> {
        let cache = self.module_cache.upgrade()?;
        let mut m = Metric::default();
        let mut gauge = Gauge::default();
        // NB. lossy conversion from usize to f64, to match prometheus' API.
        gauge.set_value(cache.len() as f64);
        m.set_gauge(gauge);
        Some(m)
    }
}

impl<R: ModuleResolver + Send + Sync> Collector for ModuleCacheGauge<R> {
    fn desc(&self) -> Vec<&Desc> {
        vec![&self.desc]
    }

    fn collect(&self) -> Vec<MetricFamily> {
        let mut m = MetricFamily::default();

        m.set_name(self.desc.fq_name.clone());
        m.set_help(self.desc.help.clone());
        m.set_field_type(MetricType::GAUGE);

        if let Some(metric) = self.metric() {
            m.mut_metric().push(metric);
        }

        vec![m]
    }
}
