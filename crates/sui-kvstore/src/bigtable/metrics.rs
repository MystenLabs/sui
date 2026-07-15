// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prometheus::GaugeVec;
use prometheus::HistogramVec;
use prometheus::IntCounterVec;
use prometheus::IntGaugeVec;
use prometheus::Registry;
use prometheus::register_gauge_vec_with_registry;
use prometheus::register_histogram_vec_with_registry;
use prometheus::register_int_counter_vec_with_registry;
use prometheus::register_int_gauge_vec_with_registry;

pub(crate) struct KvMetrics {
    pub kv_get_success: IntCounterVec,
    pub kv_get_not_found: IntCounterVec,
    pub kv_get_errors: IntCounterVec,
    pub kv_get_latency_ms: HistogramVec,
    pub kv_get_batch_size: HistogramVec,
    pub kv_get_latency_ms_per_key: HistogramVec,
    pub kv_get_stream_poll_wait_ms: HistogramVec,
    pub kv_get_stream_poll_wait_ms_per_key: HistogramVec,
    pub kv_scan_success: IntCounterVec,
    pub kv_scan_not_found: IntCounterVec,
    pub kv_scan_error: IntCounterVec,
    pub kv_scan_latency_ms: HistogramVec,
    pub kv_bt_chunk_latency_ms: HistogramVec,
    pub kv_bt_read_rows_started_total: IntCounterVec,
    pub kv_bt_chunk_rows_returned_count: IntCounterVec,
    pub kv_bt_chunk_rows_seen_count: IntCounterVec,
    pub kv_write_flow_control_qps: GaugeVec,
    pub kv_write_flow_control_enabled: IntGaugeVec,
    pub kv_write_flow_control_hints: IntCounterVec,
    pub kv_write_throttle_latency_ms: HistogramVec,
}

impl KvMetrics {
    pub(crate) fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            kv_get_success: register_int_counter_vec_with_registry!(
                "kv_get_success",
                "Number of successful fetches from kv store",
                &["client", "table"],
                registry,
            )
            .unwrap(),
            kv_get_not_found: register_int_counter_vec_with_registry!(
                "kv_get_not_found",
                "Number of fetches from kv store that returned not found",
                &["client", "table"],
                registry,
            )
            .unwrap(),
            kv_get_errors: register_int_counter_vec_with_registry!(
                "kv_get_errors",
                "Number of fetches from kv store that returned an error",
                &["client", "table"],
                registry,
            )
            .unwrap(),
            kv_get_latency_ms: register_histogram_vec_with_registry!(
                "kv_get_latency_ms",
                "Latency of fetches from kv store",
                &["client", "table"],
                prometheus::exponential_buckets(1.0, 1.6, 24)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
            kv_get_batch_size: register_histogram_vec_with_registry!(
                "kv_get_batch_size",
                "Number of keys fetched per batch from kv store",
                &["client", "table"],
                prometheus::exponential_buckets(1.0, 1.6, 20)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
            kv_get_latency_ms_per_key: register_histogram_vec_with_registry!(
                "kv_get_latency_ms_per_key",
                "Latency of fetches from kv store per key",
                &["client", "table"],
                prometheus::exponential_buckets(1.0, 1.6, 24)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
            kv_get_stream_poll_wait_ms: register_histogram_vec_with_registry!(
                "kv_get_stream_poll_wait_ms",
                "Accumulated demand-visible supply wait for a successfully drained streamed BigTable multi-get: ReadRows RPC open time plus time from downstream polling for a row until that demand returns a row or natural EOF. Excludes idle time after delivering a row and before the next downstream poll. May include BigTable service time, network transit, gRPC decoding, and executor scheduling while demand is pending; pair with kv_bt_chunk_latency_ms for BigTable frontend latency. Observed only after natural successful EOF, so errors, early drops, cancellations, and item-limit truncation are absent and create survivorship bias.",
                &["client", "table"],
                prometheus::exponential_buckets(1.0, 1.6, 24)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
            kv_get_stream_poll_wait_ms_per_key: register_histogram_vec_with_registry!(
                "kv_get_stream_poll_wait_ms_per_key",
                "Accumulated demand-visible supply wait per requested key for a successfully drained streamed BigTable multi-get: ReadRows RPC open time plus downstream-active row poll-to-ready time, divided by requested keys. Excludes idle time between delivered rows and later demand. May include BigTable service time, network transit, gRPC decoding, and executor scheduling; pair with kv_bt_chunk_latency_ms. Observed only after natural successful EOF, so errors, early drops, cancellations, and item-limit truncation are absent and create survivorship bias.",
                &["client", "table"],
                prometheus::exponential_buckets(1.0, 1.6, 24)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
            kv_scan_success: register_int_counter_vec_with_registry!(
                "kv_scan_success",
                "Number of successful scans from kv store",
                &["client", "table"],
                registry,
            )
            .unwrap(),
            kv_scan_not_found: register_int_counter_vec_with_registry!(
                "kv_scan_not_found",
                "Number of fetches from kv store that returned not found",
                &["client", "table"],
                registry,
            )
            .unwrap(),
            kv_scan_error: register_int_counter_vec_with_registry!(
                "kv_scan_error",
                "Number of scans from kv store that returned an error",
                &["client", "table"],
                registry,
            )
            .unwrap(),
            kv_scan_latency_ms: register_histogram_vec_with_registry!(
                "kv_scan_latency_ms",
                "Latency of scans from kv store",
                &["client", "table"],
                prometheus::exponential_buckets(1.0, 1.6, 24)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
            kv_bt_chunk_latency_ms: register_histogram_vec_with_registry!(
                "kv_bt_chunk_latency_ms",
                "Reported BigTable latency for a single chunk",
                &["client", "table"],
                prometheus::exponential_buckets(1.0, 1.6, 24)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
            kv_bt_read_rows_started_total: register_int_counter_vec_with_registry!(
                "kv_bt_read_rows_started_total",
                "ReadRows RPCs initiated per table; incremented at RPC start, covering point multi-gets and range scans alike",
                &["client", "table"],
                registry,
            )
            .unwrap(),
            kv_bt_chunk_rows_returned_count: register_int_counter_vec_with_registry!(
                "kv_bt_chunk_rows_returned_count",
                "Reported BigTable rows returned count for a single chunk",
                &["client", "table"],
                registry,
            )
            .unwrap(),
            kv_bt_chunk_rows_seen_count: register_int_counter_vec_with_registry!(
                "kv_bt_chunk_rows_seen_count",
                "Reported BigTable rows seen count for a single chunk",
                &["client", "table"],
                registry,
            )
            .unwrap(),
            kv_write_flow_control_qps: register_gauge_vec_with_registry!(
                "kv_write_flow_control_qps",
                "Current adaptive write flow-control target QPS",
                &["client"],
                registry,
            )
            .unwrap(),
            kv_write_flow_control_enabled: register_int_gauge_vec_with_registry!(
                "kv_write_flow_control_enabled",
                "Whether adaptive write flow control is currently enabled (0/1)",
                &["client"],
                registry,
            )
            .unwrap(),
            kv_write_flow_control_hints: register_int_counter_vec_with_registry!(
                "kv_write_flow_control_hints",
                "Number of valid MutateRows rate-limit hints received",
                &["client"],
                registry,
            )
            .unwrap(),
            kv_write_throttle_latency_ms: register_histogram_vec_with_registry!(
                "kv_write_throttle_latency_ms",
                "Time blocked acquiring a write flow-control token",
                &["client"],
                prometheus::exponential_buckets(1.0, 1.6, 24)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
        })
    }
}
