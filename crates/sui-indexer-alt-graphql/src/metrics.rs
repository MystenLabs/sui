// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prometheus::{
    register_histogram_with_registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry, register_int_gauge_vec_with_registry,
    register_int_gauge_with_registry, Histogram, IntCounter, IntCounterVec, IntGauge, IntGaugeVec,
    Registry,
};

/// Histogram buckets for the distribution of latency (time between receiving a request and sending
/// a response).
const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0,
    200.0, 500.0, 1000.0,
];

const INPUT_NODES_BUCKETS: &[f64] = &[1., 2., 5., 10., 20., 50., 100., 200., 500., 1000.];

const INPUT_DEPTH_BUCKETS: &[f64] = &[1., 2., 5., 10., 20., 50.];

const OUTPUT_NODES_BUCKETS: &[f64] = &[
    10., 20., 50., 100., 200., 500., 1000., 2000., 5000., 10000., 20000., 50000., 100000., 200000.,
    500000., 1000000.,
];

const PAYLOAD_SIZE_BUCKETS: &[f64] = &[
    0., 100., 200., 500., 1000., 2000., 5000., 10000., 20000., 50000., 100000., 200000., 500000.,
];

pub struct RpcMetrics {
    // Metrics related to retention across various data sources.
    pub watermark_epoch: IntGaugeVec,
    pub watermark_checkpoint: IntGaugeVec,
    pub watermark_transaction: IntGaugeVec,
    pub watermark_timestamp_ms: IntGaugeVec,
    pub watermark_reader_epoch_lo: IntGaugeVec,
    pub watermark_reader_checkpoint_lo: IntGaugeVec,
    pub watermark_reader_transaction_lo: IntGaugeVec,

    // Top-level metrics for all read requests (queries).
    pub query_latency: Histogram,
    pub queries_received: IntCounter,
    pub queries_succeeded: IntCounter,
    pub queries_failed: IntCounterVec,
    pub queries_in_flight: IntGauge,

    pub limits_validation_latency: Histogram,

    // Limits checked during validation, for requests that pass all checks.
    pub input_nodes: Histogram,
    pub input_depth: Histogram,
    pub output_nodes: Histogram,

    pub total_payload_size: Histogram,
    pub query_payload_size: Histogram,
    pub tx_payload_size: Histogram,

    // Metrics per type and field.
    pub fields_received: IntCounterVec,
    pub fields_succeeded: IntCounterVec,
    pub fields_failed: IntCounterVec,
}

impl RpcMetrics {
    pub(crate) fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            watermark_epoch: register_int_gauge_vec_with_registry!(
                "watermark_epoch",
                "The epoch that the RPC considers to be the latest, for this pipeline",
                &["pipeline"],
                registry
            )
            .unwrap(),

            watermark_checkpoint: register_int_gauge_vec_with_registry!(
                "watermark_checkpoint",
                "The checkpoint sequence number that the RPC considers to be the latest, for this pipeline",
                &["pipeline"],
                registry
            )
            .unwrap(),

            watermark_transaction: register_int_gauge_vec_with_registry!(
                "watermark_transaction",
                "This RPC's exclusive upper bound on transaction sequence numbers, for this pipeline",
                &["pipeline"],
                registry
            )
            .unwrap(),

            watermark_timestamp_ms: register_int_gauge_vec_with_registry!(
                "watermark_timestamp_ms",
                "The timestamp in milliseconds of the checkpoint that the RPC considers to be the latest, for this pipeline",
                &["pipeline"],
                registry
            )
            .unwrap(),

            watermark_reader_epoch_lo: register_int_gauge_vec_with_registry!(
                "watermark_reader_epoch_lo",
                "The earliest epoch that the RPC has data for, for this pipeline",
                &["pipeline"],
                registry
            )
            .unwrap(),

            watermark_reader_checkpoint_lo: register_int_gauge_vec_with_registry!(
                "watermark_reader_checkpoint_lo",
                "The earliest checkpoint that the RPC has data for, for this pipeline",
                &["pipeline"],
                registry
            )
            .unwrap(),

            watermark_reader_transaction_lo: register_int_gauge_vec_with_registry!(
                "watermark_reader_transaction_lo",
                "The earliest transaction that the RPC has data for, for this pipeline",
                &["pipeline"],
                registry
            )
            .unwrap(),

            query_latency: register_histogram_with_registry!(
                "graphql_query_latency",
                "Time taken to respond to read requests",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            queries_received: register_int_counter_with_registry!(
                "graphql_queries_received",
                "Number of read requests the service has received",
                registry,
            )
            .unwrap(),

            queries_succeeded: register_int_counter_with_registry!(
                "graphql_queries_succeeded",
                "Number of read requests that have completed without any errors",
                registry,
            )
            .unwrap(),

            queries_failed: register_int_counter_vec_with_registry!(
                "graphql_queries_failed",
                "Number of read requests that have completed with at least one error",
                &["code"],
                registry,
            )
            .unwrap(),

            queries_in_flight: register_int_gauge_with_registry!(
                "graphql_queries_in_flight",
                "Number of read requests currently flowing through the service",
                registry
            )
            .unwrap(),

            limits_validation_latency: register_histogram_with_registry!(
                "graphql_limits_validation_latency",
                "Time taken to validate query limits",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            input_nodes: register_histogram_with_registry!(
                "graphql_input_nodes",
                "Number of nodes in the request input",
                INPUT_NODES_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),

            input_depth: register_histogram_with_registry!(
                "graphql_input_depth",
                "Depth of the request input",
                INPUT_DEPTH_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),

            output_nodes: register_histogram_with_registry!(
                "graphql_output_nodes",
                "Number of nodes in the response output",
                OUTPUT_NODES_BUCKETS.to_vec(),
                registry
            )
            .unwrap(),

            total_payload_size: register_histogram_with_registry!(
                "graphql_total_payload_size",
                "Total size of the request",
                PAYLOAD_SIZE_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            query_payload_size: register_histogram_with_registry!(
                "graphql_query_payload_size",
                "Size of the query part of a request",
                PAYLOAD_SIZE_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            tx_payload_size: register_histogram_with_registry!(
                "graphql_tx_payload_size",
                "Size of the transaction part of a request",
                PAYLOAD_SIZE_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),

            fields_received: register_int_counter_vec_with_registry!(
                "graphql_fields_received",
                "Number of times a field of a type has been requested in the GraphQL schema",
                &["type", "field"],
                registry,
            )
            .unwrap(),

            fields_succeeded: register_int_counter_vec_with_registry!(
                "graphql_fields_succeeded",
                "Number of times a field of a type has been successfully resolved",
                &["type", "field"],
                registry,
            )
            .unwrap(),

            fields_failed: register_int_counter_vec_with_registry!(
                "graphql_fields_failed",
                "Number of times a field of a type has failed to resolve",
                &["type", "field"],
                registry,
            )
            .unwrap(),
        })
    }
}
