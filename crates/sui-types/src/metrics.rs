// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{register_int_counter_vec_with_registry, IntCounterVec};

pub struct LimitsMetrics {
    /// Execution limits metrics
    pub excessive_estimated_effects_size: IntCounterVec,
    pub excessive_written_objects_size: IntCounterVec,
    pub excessive_new_move_object_ids: IntCounterVec,
    pub excessive_deleted_move_object_ids: IntCounterVec,
    pub excessive_transferred_move_object_ids: IntCounterVec,
    pub excessive_object_runtime_cached_objects: IntCounterVec,
    pub excessive_object_runtime_store_entries: IntCounterVec,
}

impl LimitsMetrics {
    pub fn new(registry: &prometheus::Registry) -> LimitsMetrics {
        Self {
            excessive_estimated_effects_size: register_int_counter_vec_with_registry!(
                "excessive_estimated_effects_size",
                "Number of transactions with estimated effects size exceeding the limit",
                &["metered", "limit_type"],
                registry,
            )
                .unwrap(),
            excessive_written_objects_size: register_int_counter_vec_with_registry!(
                "excessive_written_objects_size",
                "Number of transactions with written objects size exceeding the limit",
                &["metered", "limit_type"],
                registry,
            )
                .unwrap(),
            excessive_new_move_object_ids: register_int_counter_vec_with_registry!(
                "excessive_new_move_object_ids_size",
                "Number of transactions with new move object ID count exceeding the limit",
                &["metered", "limit_type"],
                registry,
            )
                .unwrap(),
            excessive_deleted_move_object_ids: register_int_counter_vec_with_registry!(
                "excessive_deleted_move_object_ids_size",
                "Number of transactions with deleted move object ID count exceeding the limit",
                &["metered", "limit_type"],
                registry,
            )
                .unwrap(),
            excessive_transferred_move_object_ids: register_int_counter_vec_with_registry!(
                "excessive_transferred_move_object_ids_size",
                "Number of transactions with transferred move object ID count exceeding the limit",
                &["metered", "limit_type"],
                registry,
            )
                .unwrap(),
            excessive_object_runtime_cached_objects: register_int_counter_vec_with_registry!(
                "excessive_object_runtime_cached_objects_size",
                "Number of transactions with object runtime cached object count exceeding the limit",
                &["metered", "limit_type"],
                registry,
            )
                .unwrap(),
            excessive_object_runtime_store_entries: register_int_counter_vec_with_registry!(
                "excessive_object_runtime_store_entries_size",
                "Number of transactions with object runtime store entry count exceeding the limit",
                &["metered", "limit_type"],
                registry,
            )
                .unwrap(),
        }
    }
}
