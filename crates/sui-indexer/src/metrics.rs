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

#[derive(Clone, Debug)]
pub struct IndexerTransactionHandlerMetrics {
    pub total_transactions_received: IntCounter,
    pub total_transactions_processed: IntCounter,
    pub total_transaction_checkpoint_processed: IntCounter,
    pub total_transaction_handler_error: IntCounter,

    pub db_write_request_latency: Histogram,
    pub full_node_read_request_latency: Histogram,
    pub checkpoint_db_read_request_latency: Histogram,
}

impl IndexerTransactionHandlerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_transactions_received: register_int_counter_with_registry!(
                "total_transactions_received",
                "Total number of transactions received",
                registry,
            )
            .unwrap(),
            total_transactions_processed: register_int_counter_with_registry!(
                "total_transactions_processed",
                "Total number of transactions processed",
                registry,
            )
            .unwrap(),
            total_transaction_checkpoint_processed: register_int_counter_with_registry!(
                "total_transaction_checkpoint_processed",
                "Total number of transactions processed",
                registry,
            )
            .unwrap(),
            total_transaction_handler_error: register_int_counter_with_registry!(
                "total_transaction_handler_error",
                "Total number of transaction handler error",
                registry,
            )
            .unwrap(),
            db_write_request_latency: register_histogram_with_registry!(
                "transaction_db_write_request_latency",
                "Time spent commiting a transaction to the db",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            full_node_read_request_latency: register_histogram_with_registry!(
                "transaction_full_node_read_request_latency",
                "Time spent in waiting for a new transaction from the Full Node",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_db_read_request_latency: register_histogram_with_registry!(
                "transaction_checkpoint_db_read_request_latency",
                "Time spent in reading a transaction from the checkpoint db",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct IndexerEventHandlerMetrics {
    pub total_events_received: IntCounter,
    pub total_events_processed: IntCounter,

    pub total_event_page_fetch_attempt: IntCounter,
    pub total_event_page_received: IntCounter,
    pub total_event_page_committed: IntCounter,

    pub total_event_handler_error: IntCounter,

    pub db_write_request_latency: Histogram,
    pub full_node_read_request_latency: Histogram,
}

impl IndexerEventHandlerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_events_received: register_int_counter_with_registry!(
                "total_events_received",
                "Total number of events received",
                registry,
            )
            .unwrap(),
            total_events_processed: register_int_counter_with_registry!(
                "total_events_processed",
                "Total number of events processed",
                registry,
            )
            .unwrap(),
            total_event_page_fetch_attempt: register_int_counter_with_registry!(
                "total_event_page_fetch_attempt",
                "Total number of event page fetch attempt",
                registry,
            )
            .unwrap(),
            total_event_page_received: register_int_counter_with_registry!(
                "total_event_page_received",
                "Total number of event page received",
                registry,
            )
            .unwrap(),
            total_event_page_committed: register_int_counter_with_registry!(
                "total_event_page_committed",
                "Total number of event page committed",
                registry,
            )
            .unwrap(),
            total_event_handler_error: register_int_counter_with_registry!(
                "total_event_handler_error",
                "Total number of event handler error",
                registry,
            )
            .unwrap(),
            db_write_request_latency: register_histogram_with_registry!(
                "event_db_write_request_latency",
                "Time spent commiting an event to the db",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            full_node_read_request_latency: register_histogram_with_registry!(
                "event_full_node_read_request_latency",
                "Time spent in waiting for a new event from the Full Node",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
        }
    }
}

#[derive(Clone, Debug)]

// TODO: remove object and publish event related metrics after wave 2
pub struct IndexerObjectEventHandlerMetrics {
    pub total_object_events_received: IntCounter,
    pub total_object_events_processed: IntCounter,
    pub total_object_event_handler_error: IntCounter,
}

impl IndexerObjectEventHandlerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_object_events_received: register_int_counter_with_registry!(
                "total_object_events_received",
                "Total number of object events received",
                registry,
            )
            .unwrap(),
            total_object_events_processed: register_int_counter_with_registry!(
                "total_object_events_processed",
                "Total number of object events processed",
                registry,
            )
            .unwrap(),
            total_object_event_handler_error: register_int_counter_with_registry!(
                "total_object_event_handler_error",
                "Total number of object event handler error",
                registry,
            )
            .unwrap(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct IndexerPublishEventHandlerMetrics {
    pub total_publish_events_received: IntCounter,
    pub total_publish_events_processed: IntCounter,
    pub total_publish_event_handler_error: IntCounter,
}

impl IndexerPublishEventHandlerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_publish_events_received: register_int_counter_with_registry!(
                "total_publish_events_received",
                "Total number of publish events received",
                registry,
            )
            .unwrap(),
            total_publish_events_processed: register_int_counter_with_registry!(
                "total_publish_events_processed",
                "Total number of publish events processed",
                registry,
            )
            .unwrap(),
            total_publish_event_handler_error: register_int_counter_with_registry!(
                "total_publish_event_handler_error",
                "Total number of publish event handler error",
                registry,
            )
            .unwrap(),
        }
    }
}

pub struct IndexerMoveEventHandlerMetrics {
    pub total_move_events_received: IntCounter,
    pub total_move_events_processed: IntCounter,
    pub total_move_event_handler_error: IntCounter,
}

impl IndexerMoveEventHandlerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_move_events_received: register_int_counter_with_registry!(
                "total_move_events_received",
                "Total number of move events received",
                registry,
            )
            .unwrap(),
            total_move_events_processed: register_int_counter_with_registry!(
                "total_move_events_processed",
                "Total number of move events processed",
                registry,
            )
            .unwrap(),
            total_move_event_handler_error: register_int_counter_with_registry!(
                "total_move_event_handler_error",
                "Total number of move event handler error",
                registry,
            )
            .unwrap(),
        }
    }
}

pub struct IndexerCheckpointHandlerMetrics {
    pub total_checkpoint_requested: IntCounter,
    pub total_checkpoint_received: IntCounter,
    pub total_checkpoint_processed: IntCounter,
    pub total_checkpoint_handler_error: IntCounter,
    pub db_write_request_latency: Histogram,
    pub full_node_read_request_latency: Histogram,
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
            total_checkpoint_processed: register_int_counter_with_registry!(
                "total_checkpoint_processed",
                "Total number of checkpoint processed",
                registry,
            )
            .unwrap(),
            total_checkpoint_handler_error: register_int_counter_with_registry!(
                "total_checkpoint_handler_error",
                "Total number of checkpoint handler error",
                registry,
            )
            .unwrap(),
            total_checkpoint_requested: register_int_counter_with_registry!(
                "total_checkpoint_requested",
                "Total number of checkpoint requested",
                registry,
            )
            .unwrap(),
            db_write_request_latency: register_histogram_with_registry!(
                "checkpoint_db_write_request_latency",
                "Time spent commiting a checkpoint to the db",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            full_node_read_request_latency: register_histogram_with_registry!(
                "checkpoint_full_node_read_request_latency",
                "Time spent in waiting for a new checkpoint from the Full Node",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
        }
    }
}

/// derivative data processor related metrics
#[derive(Clone, Debug)]
pub struct IndexerAddressProcessorMetrics {
    pub total_address_batch_processed: IntCounter,
    pub total_address_processor_error: IntCounter,
}

impl IndexerAddressProcessorMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_address_batch_processed: register_int_counter_with_registry!(
                "total_address_batch_processed",
                "Total number of address batches processed",
                registry,
            )
            .unwrap(),
            total_address_processor_error: register_int_counter_with_registry!(
                "total_address_processor_error",
                "Total number of address processor error",
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

#[derive(Clone, Debug)]
pub struct IndexerPackageProcessorMetrics {
    pub total_package_batch_processed: IntCounter,
    pub total_package_processor_error: IntCounter,
}

impl IndexerPackageProcessorMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_package_batch_processed: register_int_counter_with_registry!(
                "total_package_batch_processed",
                "Total number of package batches processed",
                registry,
            )
            .unwrap(),
            total_package_processor_error: register_int_counter_with_registry!(
                "total_package_processor_error",
                "Total number of package processor error",
                registry,
            )
            .unwrap(),
        }
    }
}
