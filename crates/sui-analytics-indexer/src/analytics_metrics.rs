// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
use prometheus::{
    register_int_counter_vec_with_registry, register_int_gauge_vec_with_registry, IntCounterVec,
    IntGaugeVec, Registry,
};

#[derive(Clone)]
pub struct AnalyticsMetrics {
    pub total_received: IntCounterVec,
    pub last_uploaded_checkpoint: IntGaugeVec,
    pub max_checkpoint_on_store: IntGaugeVec,
}

impl AnalyticsMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_received: register_int_counter_vec_with_registry!(
                "total_received",
                "Number of checkpoints received",
                &["data_type"],
                registry
            )
            .unwrap(),
            last_uploaded_checkpoint: register_int_gauge_vec_with_registry!(
                "last_uploaded_checkpoint",
                "Number of uploaded checkpoints.",
                &["data_type"],
                registry,
            )
            .unwrap(),
            max_checkpoint_on_store: register_int_gauge_vec_with_registry!(
                "max_checkpoint_on_store",
                "Max checkpoint on the db table.",
                &["data_type"],
                registry,
            )
            .unwrap(),
        }
    }
}
