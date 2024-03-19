// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::net::SocketAddr;

use axum::{extract::Extension, http::StatusCode, routing::get, Router};
use prometheus::{
    register_histogram_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, Histogram, IntCounter, IntGauge,
};
use prometheus::{Registry, TextEncoder};
use regex::Regex;
use tracing::{info, warn};

use mysten_metrics::RegistryService;

const METRICS_ROUTE: &str = "/metrics";

pub fn start_prometheus_server(
    addr: SocketAddr,
    fn_url: &str,
) -> Result<(RegistryService, Registry), anyhow::Error> {
    let converted_fn_url = convert_url(fn_url);
    if converted_fn_url.is_none() {
        warn!(
            "Failed to convert full node url {} to a shorter version",
            fn_url
        );
    }
    let fn_url_str = converted_fn_url.unwrap_or_else(|| "unknown_url".to_string());

    let labels = HashMap::from([("indexer_fullnode".to_string(), fn_url_str)]);
    info!("Starting prometheus server with labels: {:?}", labels);
    let registry = Registry::new_custom(Some("indexer".to_string()), Some(labels))?;
    let registry_service = RegistryService::new(registry.clone());

    let app = Router::new()
        .route(METRICS_ROUTE, get(metrics))
        .layer(Extension(registry_service.clone()));

    tokio::spawn(async move {
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
    });
    Ok((registry_service, registry))
}

async fn metrics(Extension(registry_service): Extension<RegistryService>) -> (StatusCode, String) {
    let metrics_families = registry_service.gather_all();
    match TextEncoder.encode_to_string(&metrics_families) {
        Ok(metrics) => (StatusCode::OK, metrics),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("unable to encode metrics: {error}"),
        ),
    }
}

fn convert_url(url_str: &str) -> Option<String> {
    // NOTE: unwrap here is safe because the regex is a constant.
    let re = Regex::new(r"https?://([a-z0-9-]+\.[a-z0-9-]+\.[a-z]+)").unwrap();
    let captures = re.captures(url_str)?;

    captures.get(1).map(|m| m.as_str().to_string())
}

/// Prometheus metrics for sui-indexer.
// buckets defined in seconds
const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.02, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 3.0, 5.0, 10.0, 20.0, 40.0, 60.0,
    80.0, 100.0, 200.0,
];

const DB_COMMIT_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 2.0, 3.0,
    5.0, 10.0, 20.0, 40.0, 60.0, 80.0, 100.0, 200.0,
];

#[derive(Clone)]
pub struct IndexerMetrics {
    pub total_checkpoint_received: IntCounter,
    pub total_tx_checkpoint_committed: IntCounter,
    pub total_object_checkpoint_committed: IntCounter,
    pub total_transaction_committed: IntCounter,
    pub total_object_change_committed: IntCounter,
    pub total_transaction_chunk_committed: IntCounter,
    pub total_object_change_chunk_committed: IntCounter,
    pub total_epoch_committed: IntCounter,
    pub latest_fullnode_checkpoint_sequence_number: IntGauge,
    pub latest_tx_checkpoint_sequence_number: IntGauge,
    pub latest_indexer_object_checkpoint_sequence_number: IntGauge,
    pub latest_object_snapshot_sequence_number: IntGauge,
    // analytical
    pub latest_move_call_metrics_tx_seq: IntGauge,
    pub latest_address_metrics_tx_seq: IntGauge,
    pub latest_network_metrics_cp_seq: IntGauge,
    // checkpoint E2E latency is:
    // fullnode_download_latency + checkpoint_index_latency + db_commit_latency
    pub checkpoint_download_bytes_size: IntGauge,
    pub fullnode_checkpoint_data_download_latency: Histogram,
    pub fullnode_checkpoint_wait_and_download_latency: Histogram,
    pub fullnode_transaction_download_latency: Histogram,
    pub fullnode_object_download_latency: Histogram,
    pub checkpoint_index_latency: Histogram,
    pub indexing_tx_object_changes_latency: Histogram,
    pub indexing_objects_latency: Histogram,
    pub indexing_get_object_in_mem_hit: IntCounter,
    pub indexing_get_object_db_hit: IntCounter,
    pub indexing_module_resolver_in_mem_hit: IntCounter,
    pub indexing_package_resolver_in_mem_hit: IntCounter,
    pub indexing_packages_latency: Histogram,
    pub checkpoint_objects_index_latency: Histogram,
    pub checkpoint_db_commit_latency: Histogram,
    pub checkpoint_db_commit_latency_step_1: Histogram,
    pub checkpoint_db_commit_latency_transactions: Histogram,
    pub checkpoint_db_commit_latency_transactions_chunks: Histogram,
    pub checkpoint_db_commit_latency_transactions_chunks_transformation: Histogram,
    pub checkpoint_db_commit_latency_objects: Histogram,
    pub checkpoint_db_commit_latency_objects_history: Histogram,
    pub checkpoint_db_commit_latency_objects_chunks: Histogram,
    pub checkpoint_db_commit_latency_objects_history_chunks: Histogram,
    pub checkpoint_db_commit_latency_events: Histogram,
    pub checkpoint_db_commit_latency_events_chunks: Histogram,
    pub checkpoint_db_commit_latency_packages: Histogram,
    pub checkpoint_db_commit_latency_tx_indices: Histogram,
    pub checkpoint_db_commit_latency_tx_indices_chunks: Histogram,
    pub checkpoint_db_commit_latency_checkpoints: Histogram,
    pub checkpoint_db_commit_latency_epoch: Histogram,
    pub advance_epoch_latency: Histogram,
    pub update_object_snapshot_latency: Histogram,
    pub tokio_blocking_task_wait_latency: Histogram,
    // average latency of committing 1000 transactions.
    // 1000 is not necessarily the batch size, it's to roughly map average tx commit latency to [0.1, 1] seconds,
    // which is well covered by DB_COMMIT_LATENCY_SEC_BUCKETS.
    pub thousand_transaction_avg_db_commit_latency: Histogram,
    pub object_db_commit_latency: Histogram,
    pub object_mutation_db_commit_latency: Histogram,
    pub object_deletion_db_commit_latency: Histogram,
    pub epoch_db_commit_latency: Histogram,
    // latency of event websocket subscription
    pub subscription_process_latency: Histogram,
    pub transaction_per_checkpoint: Histogram,
    // FN RPC latencies on the read path
    // read.rs
    pub get_transaction_block_latency: Histogram,
    pub multi_get_transaction_blocks_latency: Histogram,
    pub get_object_latency: Histogram,
    pub multi_get_objects_latency: Histogram,
    pub try_get_past_object_latency: Histogram,
    pub try_multi_get_past_objects_latency: Histogram,
    pub get_checkpoint_latency: Histogram,
    pub get_checkpoints_latency: Histogram,
    pub get_events_latency: Histogram,
    pub get_loaded_child_objects_latency: Histogram,
    pub get_total_transaction_blocks_latency: Histogram,
    pub get_latest_checkpoint_sequence_number_latency: Histogram,
    // indexer.rs
    pub get_owned_objects_latency: Histogram,
    pub query_transaction_blocks_latency: Histogram,
    pub query_events_latency: Histogram,
    pub get_dynamic_fields_latency: Histogram,
    pub get_dynamic_field_object_latency: Histogram,
    pub get_protocol_config_latency: Histogram,
    // indexer state metrics
    pub db_conn_pool_size: IntGauge,
    pub idle_db_conn: IntGauge,

    pub address_processor_failure: IntCounter,
    pub checkpoint_metrics_processor_failure: IntCounter,
}

impl IndexerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            total_checkpoint_received: register_int_counter_with_registry!(
                "total_checkpoint_received",
                "Total number of checkpoint received",
                registry,
            )
            .unwrap(),
            total_tx_checkpoint_committed: register_int_counter_with_registry!(
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
            total_transaction_chunk_committed: register_int_counter_with_registry!(
                "total_transaction_chunk_commited",
                "Total number of transaction chunk committed",
                registry,
            )
            .unwrap(),
            total_object_change_chunk_committed: register_int_counter_with_registry!(
                "total_object_change_chunk_committed",
                "Total number of object change chunk committed",
                registry,
            )
            .unwrap(),
            total_epoch_committed: register_int_counter_with_registry!(
                "total_epoch_committed",
                "Total number of epoch committed",
                registry,
            )
            .unwrap(),
            latest_fullnode_checkpoint_sequence_number: register_int_gauge_with_registry!(
                "latest_fullnode_checkpoint_sequence_number",
                "Latest checkpoint sequence number from the Full Node",
                registry,
            )
            .unwrap(),
            latest_tx_checkpoint_sequence_number: register_int_gauge_with_registry!(
                "latest_indexer_checkpoint_sequence_number",
                "Latest checkpoint sequence number from the Indexer",
                registry,
            )
            .unwrap(),
            latest_indexer_object_checkpoint_sequence_number: register_int_gauge_with_registry!(
                "latest_indexer_object_checkpoint_sequence_number",
                "Latest object checkpoint sequence number from the Indexer",
                registry,
            )
            .unwrap(),
            latest_object_snapshot_sequence_number: register_int_gauge_with_registry!(
                "latest_object_snapshot_sequence_number",
                "Latest object snapshot sequence number from the Indexer",
                registry,
            ).unwrap(),
            latest_move_call_metrics_tx_seq: register_int_gauge_with_registry!(
                "latest_move_call_metrics_tx_seq",
                "Latest move call metrics tx seq",
                registry,
            ).unwrap(),
            latest_address_metrics_tx_seq: register_int_gauge_with_registry!(
                "latest_address_metrics_tx_seq",
                "Latest address metrics tx seq",
                registry,
            ).unwrap(),
            latest_network_metrics_cp_seq: register_int_gauge_with_registry!(
                "latest_network_metrics_cp_seq",
                "Latest network metrics cp seq",
                registry,
            ).unwrap(),
            checkpoint_download_bytes_size: register_int_gauge_with_registry!(
                "checkpoint_download_bytes_size",
                "Size of the downloaded checkpoint in bytes",
                registry,
            ).unwrap(),
            fullnode_checkpoint_data_download_latency: register_histogram_with_registry!(
                "fullnode_checkpoint_data_download_latency",
                "Time spent in downloading checkpoint and transation for a new checkpoint from the Full Node",
                LATENCY_SEC_BUCKETS.to_vec(),
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
            indexing_tx_object_changes_latency: register_histogram_with_registry!(
                "indexing_tx_object_changes_latency",
                "Time spent in indexing object changes for a transaction",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            indexing_objects_latency: register_histogram_with_registry!(
                "indexing_objects_latency",
                "Time spent in indexing objects",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            indexing_packages_latency: register_histogram_with_registry!(
                "indexing_packages_latency",
                "Time spent in indexing packages",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            indexing_get_object_in_mem_hit: register_int_counter_with_registry!(
                "indexing_get_object_in_mem_hit",
                "Total number get object hit in mem",
                registry,
            )
            .unwrap(),
            indexing_get_object_db_hit: register_int_counter_with_registry!(
                "indexing_get_object_db_hit",
                "Total number get object hit in db",
                registry,
            )
            .unwrap(),
            indexing_module_resolver_in_mem_hit: register_int_counter_with_registry!(
                "indexing_module_resolver_in_mem_hit",
                "Total number module resolver hit in mem",
                registry,
            )
            .unwrap(),
            indexing_package_resolver_in_mem_hit: register_int_counter_with_registry!(
                "indexing_package_resolver_in_mem_hit",
                "Total number package resolver hit in mem",
                registry,
            )
            .unwrap(),
            checkpoint_objects_index_latency: register_histogram_with_registry!(
                "checkpoint_object_index_latency",
                "Time spent in indexing a checkpoint objects",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_db_commit_latency: register_histogram_with_registry!(
                "checkpoint_db_commit_latency",
                "Time spent commiting a checkpoint to the db",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            checkpoint_db_commit_latency_step_1: register_histogram_with_registry!(
                "checkpoint_db_commit_latency_step_1",
                "Time spent commiting a checkpoint to the db, step 1",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_db_commit_latency_transactions: register_histogram_with_registry!(
                "checkpoint_db_commit_latency_transactions",
                "Time spent commiting transactions",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_db_commit_latency_transactions_chunks: register_histogram_with_registry!(
                "checkpoint_db_commit_latency_transactions_chunks",
                "Time spent commiting transactions chunks",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_db_commit_latency_transactions_chunks_transformation: register_histogram_with_registry!(
                "checkpoint_db_commit_latency_transactions_transaformation",
                "Time spent in transactions chunks transformation prior to commit",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_db_commit_latency_objects: register_histogram_with_registry!(
                "checkpoint_db_commit_latency_objects",
                "Time spent commiting objects",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_db_commit_latency_objects_history: register_histogram_with_registry!(
                "checkpoint_db_commit_latency_objects_history",
                "Time spent commiting objects history",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            checkpoint_db_commit_latency_objects_chunks: register_histogram_with_registry!(
                "checkpoint_db_commit_latency_objects_chunks",
                "Time spent commiting objects chunks",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_db_commit_latency_objects_history_chunks: register_histogram_with_registry!(
                "checkpoint_db_commit_latency_objects_history_chunks",
                "Time spent commiting objects history chunks",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            checkpoint_db_commit_latency_events: register_histogram_with_registry!(
                "checkpoint_db_commit_latency_events",
                "Time spent commiting events",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_db_commit_latency_events_chunks: register_histogram_with_registry!(
                "checkpoint_db_commit_latency_events_chunks",
                "Time spent commiting events chunks",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            checkpoint_db_commit_latency_packages: register_histogram_with_registry!(
                "checkpoint_db_commit_latency_packages",
                "Time spent commiting packages",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_db_commit_latency_tx_indices: register_histogram_with_registry!(
                "checkpoint_db_commit_latency_tx_indices",
                "Time spent commiting tx indices",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_db_commit_latency_tx_indices_chunks: register_histogram_with_registry!(
                "checkpoint_db_commit_latency_tx_indices_chunks",
                "Time spent commiting tx_indices chunks",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_db_commit_latency_checkpoints: register_histogram_with_registry!(
                "checkpoint_db_commit_latency_checkpoints",
                "Time spent commiting checkpoints",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_db_commit_latency_epoch: register_histogram_with_registry!(
                "checkpoint_db_commit_latency_epochs",
                "Time spent commiting epochs",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            advance_epoch_latency: register_histogram_with_registry!(
                "advance_epoch_latency",
                "Time spent in advancing epoch",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            update_object_snapshot_latency: register_histogram_with_registry!(
                "update_object_snapshot_latency",
                "Time spent in updating object snapshot",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            tokio_blocking_task_wait_latency: register_histogram_with_registry!(
                "tokio_blocking_task_wait_latency",
                "Time spent to wait for tokio blocking task pool",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            ).unwrap(),
            thousand_transaction_avg_db_commit_latency: register_histogram_with_registry!(
                "transaction_db_commit_latency",
                "Average time spent commiting 1000 transactions to the db",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            object_db_commit_latency: register_histogram_with_registry!(
                "object_db_commit_latency",
                "Time spent commiting a object to the db",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            object_mutation_db_commit_latency: register_histogram_with_registry!(
                "object_mutation_db_commit_latency",
                "Time spent commiting a object mutation to the db",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            object_deletion_db_commit_latency: register_histogram_with_registry!(
                "object_deletion_db_commit_latency",
                "Time spent commiting a object deletion to the db",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            epoch_db_commit_latency: register_histogram_with_registry!(
                "epoch_db_commit_latency",
                "Time spent commiting a epoch to the db",
                DB_COMMIT_LATENCY_SEC_BUCKETS.to_vec(),
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
            get_transaction_block_latency: register_histogram_with_registry!(
                "get_transaction_block_latency",
                "Time spent in get_transaction_block on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            multi_get_transaction_blocks_latency: register_histogram_with_registry!(
                "multi_get_transaction_blocks_latency",
                "Time spent in multi_get_transaction_blocks on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            get_object_latency: register_histogram_with_registry!(
                "get_object_latency",
                "Time spent in get_object on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            multi_get_objects_latency: register_histogram_with_registry!(
                "multi_get_objects_latency",
                "Time spent in multi_get_objects on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            try_get_past_object_latency: register_histogram_with_registry!(
                "try_get_past_object_latency",
                "Time spent in try_get_past_object on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            try_multi_get_past_objects_latency: register_histogram_with_registry!(
                "try_multi_get_past_objects_latency",
                "Time spent in try_multi_get_past_objects on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            get_checkpoint_latency: register_histogram_with_registry!(
                "get_checkpoint_latency",
                "Time spent in get_checkpoint on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            get_checkpoints_latency: register_histogram_with_registry!(
                "get_checkpoints_latency",
                "Time spent in get_checkpoints on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            get_events_latency: register_histogram_with_registry!(
                "get_events_latency",
                "Time spent in get_events on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            get_total_transaction_blocks_latency: register_histogram_with_registry!(
                "get_total_transaction_blocks_latency",
                "Time spent in get_total_transaction_blocks on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            get_latest_checkpoint_sequence_number_latency: register_histogram_with_registry!(
                "get_latest_checkpoint_sequence_number_latency",
                "Time spent in get_latest_checkpoint_sequence_number on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            get_owned_objects_latency: register_histogram_with_registry!(
                "get_owned_objects_latency",
                "Time spent in get_owned_objects on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            query_transaction_blocks_latency: register_histogram_with_registry!(
                "query_transaction_blocks_latency",
                "Time spent in query_transaction_blocks on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            query_events_latency: register_histogram_with_registry!(
                "query_events_latency",
                "Time spent in query_events on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            get_dynamic_fields_latency: register_histogram_with_registry!(
                "get_dynamic_fields_latency",
                "Time spent in get_dynamic_fields on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            get_dynamic_field_object_latency: register_histogram_with_registry!(
                "get_dynamic_field_object_latency",
                "Time spent in get_dynamic_field_object on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            get_loaded_child_objects_latency: register_histogram_with_registry!(
                "get_loaded_child_objects_latency",
                "Time spent in get_loaded_child_objects_latency on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            get_protocol_config_latency: register_histogram_with_registry!(
                "get_protocol_config_latency",
                "Time spent in get_protocol_config_latency on the fullnode behind.",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),
            db_conn_pool_size: register_int_gauge_with_registry!(
                "db_conn_pool_size",
                "Size of the database connection pool",
                registry
            ).unwrap(),
            idle_db_conn: register_int_gauge_with_registry!(
                "idle_db_conn",
                "Number of idle database connections",
                registry
            ).unwrap(),
            address_processor_failure: register_int_counter_with_registry!(
                "address_processor_failure",
                "Total number of address processor failure",
                registry,
            )
            .unwrap(),
            checkpoint_metrics_processor_failure: register_int_counter_with_registry!(
                "checkpoint_metrics_processor_failure",
                "Total number of checkpoint metrics processor failure",
                registry,
            )
            .unwrap(),
        }
    }
}
