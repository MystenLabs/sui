// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{register_int_counter_vec_with_registry, IntCounterVec, Registry};

use mysten_metrics::histogram::HistogramVec;

#[derive(Clone)]
pub struct OracleMetrics {
    pub(crate) data_source_successes: IntCounterVec,
    pub(crate) data_source_errors: IntCounterVec,
    pub(crate) upload_successes: IntCounterVec,
    pub(crate) upload_data_errors: IntCounterVec,
    pub(crate) download_successes: IntCounterVec,
    pub(crate) download_data_errors: IntCounterVec,
    pub(crate) uploaded_values: HistogramVec,
    pub(crate) downloaded_values: HistogramVec,

    pub(crate) total_gas_used: IntCounterVec,
    pub(crate) gas_used: HistogramVec,
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
            upload_successes: register_int_counter_vec_with_registry!(
                "oracle_upload_successes",
                "Total number of successful data upload",
                &["feed", "source"],
                registry,
            )
            .unwrap(),
            upload_data_errors: register_int_counter_vec_with_registry!(
                "oracle_upload_data_errors",
                "Total number of erroneous data upload",
                &["feed", "source"],
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
            uploaded_values: HistogramVec::new_in_registry(
                "oracle_uploaded_values",
                "Values uploaded on chain",
                &["feed"],
                registry,
            ),
            downloaded_values: HistogramVec::new_in_registry(
                "oracle_downloaded_values",
                "Values downloaded on chain",
                &["feed"],
                registry,
            ),
            total_gas_used: register_int_counter_vec_with_registry!(
                "oracle_total_gas_used",
                "Total number of gas used for uploading data",
                &["feed", "source"],
                registry,
            )
            .unwrap(),
            gas_used: HistogramVec::new_in_registry(
                "oracle_gas_used",
                "Gas used for uploading data",
                &["feed", "source"],
                registry,
            ),
        }
    }
}
