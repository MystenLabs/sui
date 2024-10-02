// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry, Histogram,
    HistogramVec, IntCounter, IntCounterVec, Registry,
};

#[derive(Clone)]
pub struct OracleMetrics {
    pub(crate) data_source_successes: IntCounterVec,
    pub(crate) data_source_errors: IntCounterVec,
    pub(crate) data_staleness: IntCounterVec,
    pub(crate) upload_successes: IntCounterVec,
    pub(crate) upload_data_errors: IntCounterVec,
    pub(crate) download_successes: IntCounterVec,
    pub(crate) download_data_errors: IntCounterVec,
    pub(crate) uploaded_values: HistogramVec,
    pub(crate) downloaded_values: HistogramVec,

    pub(crate) total_gas_cost: IntCounter,
    pub(crate) total_gas_rebate: IntCounter,
    pub(crate) computation_gas_used: Histogram,
    pub(crate) total_data_points_uploaded: IntCounter,
}

impl OracleMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            data_source_successes: register_int_counter_vec_with_registry!(
                "oracle_data_source_successes",
                "Total number of successful data retrieval requests to data sources",
                &["feed", "source"],
                registry,
            )
            .unwrap(),
            data_source_errors: register_int_counter_vec_with_registry!(
                "oracle_data_source_errors",
                "Total number of erroneous data retrieval requests to data sources",
                &["feed", "source"],
                registry,
            )
            .unwrap(),
            data_staleness: register_int_counter_vec_with_registry!(
                "oracle_data_staleness",
                "Total number of stale data that are skipped",
                &["feed"],
                registry,
            )
            .unwrap(),
            upload_successes: register_int_counter_vec_with_registry!(
                "oracle_upload_successes",
                "Total number of successful data upload",
                &["feed"],
                registry,
            )
            .unwrap(),
            upload_data_errors: register_int_counter_vec_with_registry!(
                "oracle_upload_data_errors",
                "Total number of erroneous data upload",
                &["feed"],
                registry,
            )
            .unwrap(),
            download_successes: register_int_counter_vec_with_registry!(
                "oracle_download_successes",
                "Total number of successful data download",
                &["feed", "object_id"],
                registry,
            )
            .unwrap(),
            download_data_errors: register_int_counter_vec_with_registry!(
                "oracle_download_data_errors",
                "Total number of erroneous data download",
                &["feed", "object_id"],
                registry,
            )
            .unwrap(),
            uploaded_values: register_histogram_vec_with_registry!(
                "oracle_uploaded_values",
                "Values uploaded on chain",
                &["feed"],
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            downloaded_values: register_histogram_vec_with_registry!(
                "oracle_downloaded_values",
                "Values downloaded on chain",
                &["feed"],
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            total_gas_cost: register_int_counter_with_registry!(
                "oracle_total_gas_cost",
                "Total number of gas used, before gas rebate",
                registry,
            )
            .unwrap(),
            total_gas_rebate: register_int_counter_with_registry!(
                "oracle_total_gas_rebate",
                "Total number of gas rebate",
                registry,
            )
            .unwrap(),
            computation_gas_used: register_histogram_with_registry!(
                "oracle_computation_gas_used",
                "computation gas used",
                mysten_metrics::COUNT_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            total_data_points_uploaded: register_int_counter_with_registry!(
                "oracle_total_data_points_uploaded",
                "Total number of data points uploaded",
                registry,
            )
            .unwrap(),
        }
    }
}
