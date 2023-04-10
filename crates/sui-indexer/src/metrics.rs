// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_histogram_with_registry, register_int_counter_with_registry, Histogram, IntCounter,
    Registry,
};

/// Prometheus metrics for sui-indexer.

// buckets defined in seconds
const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.02, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 3.0, 5.0, 10.0, 20.0, 40.0, 60.0,
    80.0, 100.0, 200.0,
];

#[derive(Clone)]
pub struct IndexerCheckpointHandlerMetrics {
    pub total_checkpoint_received: IntCounter,
    pub total_checkpoint_committed: IntCounter,
    pub total_object_checkpoint_committed: IntCounter,
    pub total_transaction_committed: IntCounter,
    pub total_object_change_committed: IntCounter,
    pub total_epoch_committed: IntCounter,
    // checkpoint E2E latency is:
    // fullnode_download_latency + checkpoint_index_latency + db_commit_latency
    pub fullnode_checkpoint_wait_and_download_latency: Histogram,
    pub fullnode_checkpoint_download_latency: Histogram,
    pub fullnode_transaction_download_latency: Histogram,
    pub fullnode_object_download_latency: Histogram,
    pub checkpoint_index_latency: Histogram,
    pub checkpoint_db_commit_latency: Histogram,
    pub object_db_commit_latency: Histogram,
    pub object_mutation_db_commit_latency: Histogram,
    pub object_deletion_db_commit_latency: Histogram,
    pub epoch_db_commit_latency: Histogram,
    // latency of event websocket subscription
    pub subscription_process_latency: Histogram,
    pub transaction_per_checkpoint: Histogram,
}

impl IndexerCheckpointHandlerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_checkpoint_received: register_int_counter_with_registry!(
                "total_checkpoint_received",
                "Total number of checkpoint received",
                registry,
            )
            .unwrap(),
            total_checkpoint_committed: register_int_counter_with_registry!(
                "total_checkpoint_committed",
                "Total number of checkpoint committed",
                registry,
            )
            .unwrap(),
            total_object_checkpoint_committed: register_int_counter_with_registry!(
                "total_object_checkpoint_committed",
                "Total number of object checkpoint committed",
                registry,
            )
            .unwrap(),
            total_transaction_committed: register_int_counter_with_registry!(
                "total_transaction_committed",
                "Total number of transaction committed",
                registry,
            )
            .unwrap(),
            total_object_change_committed: register_int_counter_with_registry!(
                "total_object_change_committed",
                "Total number of object change committed",
                registry,
            )
            .unwrap(),
            total_epoch_committed: register_int_counter_with_registry!(
                "total_epoch_committed",
                "Total number of epoch committed",
                registry,
            )
            .unwrap(),
            fullnode_checkpoint_wait_and_download_latency: register_histogram_with_registry!(
                "fullnode_checkpoint_wait_and_download_latency",
                "Time spent in waiting for a new checkpoint from the Full Node",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            fullnode_checkpoint_download_latency: register_histogram_with_registry!(
                "fullnode_checkpoint_download_latency",
                "Time spent in waiting for a new checkpoint from the Full Node",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            fullnode_transaction_download_latency: register_histogram_with_registry!(
                "fullnode_transaction_download_latency",
                "Time spent in waiting for a new transaction from the Full Node",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            fullnode_object_download_latency: register_histogram_with_registry!(
                "fullnode_object_download_latency",
                "Time spent in waiting for a new epoch from the Full Node",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_index_latency: register_histogram_with_registry!(
                "checkpoint_index_latency",
                "Time spent in indexing a checkpoint",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_db_commit_latency: register_histogram_with_registry!(
                "checkpoint_db_commit_latency",
                "Time spent commiting a checkpoint to the db",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            object_db_commit_latency: register_histogram_with_registry!(
                "object_db_commit_latency",
                "Time spent commiting a object to the db",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            object_mutation_db_commit_latency: register_histogram_with_registry!(
                "object_mutation_db_commit_latency",
                "Time spent commiting a object mutation to the db",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            object_deletion_db_commit_latency: register_histogram_with_registry!(
                "object_deletion_db_commit_latency",
                "Time spent commiting a object deletion to the db",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            epoch_db_commit_latency: register_histogram_with_registry!(
                "epoch_db_commit_latency",
                "Time spent commiting a epoch to the db",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            subscription_process_latency: register_histogram_with_registry!(
                "subscription_process_latency",
                "Time spent in process Websocket subscription",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            transaction_per_checkpoint: register_histogram_with_registry!(
                "transaction_per_checkpoint",
                "Number of transactions per checkpoint",
                vec![1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0],
                registry,
            )
            .unwrap(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct IndexerObjectProcessorMetrics {
    pub total_object_batch_processed: IntCounter,
    pub total_object_processor_error: IntCounter,
}

impl IndexerObjectProcessorMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_object_batch_processed: register_int_counter_with_registry!(
                "total_object_batch_processed",
                "Total number of object batches processed",
                registry,
            )
            .unwrap(),
            total_object_processor_error: register_int_counter_with_registry!(
                "total_object_processor_error",
                "Total number of object processor error",
                registry,
            )
            .unwrap(),
        }
    }
}
