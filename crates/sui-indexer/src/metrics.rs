// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{register_int_counter_with_registry, IntCounter, Registry};

/// Prometheus metrics for sui-indexer.

#[derive(Clone, Debug)]
pub struct IndexerTransactionHandlerMetrics {
    pub total_transactions_received: IntCounter,
    pub total_transactions_processed: IntCounter,

    pub total_transaction_page_fetch_attempt: IntCounter,
    pub total_transaction_page_received: IntCounter,
    pub total_transaction_page_committed: IntCounter,

    pub total_transaction_handler_error: IntCounter,
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
            total_transaction_page_fetch_attempt: register_int_counter_with_registry!(
                "total_transaction_page_fetch_attempt",
                "Total number of transaction page fetch attempt",
                registry,
            )
            .unwrap(),
            total_transaction_page_received: register_int_counter_with_registry!(
                "total_transaction_page_received",
                "Total number of transaction page received",
                registry,
            )
            .unwrap(),
            total_transaction_page_committed: register_int_counter_with_registry!(
                "total_transaction_page_committed",
                "Total number of transaction page committed",
                registry,
            )
            .unwrap(),
            total_transaction_handler_error: register_int_counter_with_registry!(
                "total_transaction_handler_error",
                "Total number of transaction handler error",
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
        }
    }
}

#[derive(Clone, Debug)]
pub struct IndexerCheckpointHandlerMetrics {
    pub total_checkpoint_requested: IntCounter,
    pub total_checkpoint_received: IntCounter,
    pub total_checkpoint_processed: IntCounter,
    pub total_checkpoint_handler_error: IntCounter,
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
