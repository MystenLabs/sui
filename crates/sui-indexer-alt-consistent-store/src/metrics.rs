// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use once_cell::sync::OnceCell;
use prometheus::{register_int_gauge_vec_with_registry, IntGaugeVec, Registry};
use rocksdb::{properties, properties::num_files_at_level, AsColumnFamilyRef, PerfContext};
use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;
use tap::TapFallible;
use tracing::{debug, error, warn};

thread_local! {
    static PER_THREAD_ROCKS_PERF_CONTEXT: std::cell::RefCell<rocksdb::PerfContext>  = RefCell::new(PerfContext::default());
}

// Constants for periodic metrics reporting
const CF_METRICS_REPORT_PERIOD_SECS: u64 = 30;
const METRICS_ERROR: i64 = -1;

// TODO: remove this after Rust rocksdb has the TOTAL_BLOB_FILES_SIZE property built-in.
// From https://github.com/facebook/rocksdb/blob/bd80433c73691031ba7baa65c16c63a83aef201a/include/rocksdb/db.h#L1169
const ROCKSDB_PROPERTY_TOTAL_BLOB_FILES_SIZE: &std::ffi::CStr = unsafe {
    std::ffi::CStr::from_bytes_with_nul_unchecked("rocksdb.total-blob-file-size\0".as_bytes())
};

#[derive(Debug)]
pub struct ColumnFamilyMetrics {
    pub rocksdb_total_sst_files_size: IntGaugeVec,
    pub rocksdb_total_blob_files_size: IntGaugeVec,
    pub rocksdb_total_num_files: IntGaugeVec,
    pub rocksdb_num_level0_files: IntGaugeVec,
    pub rocksdb_current_size_active_mem_tables: IntGaugeVec,
    pub rocksdb_size_all_mem_tables: IntGaugeVec,
    pub rocksdb_num_snapshots: IntGaugeVec,
    pub rocksdb_oldest_snapshot_time: IntGaugeVec,
    pub rocksdb_actual_delayed_write_rate: IntGaugeVec,
    pub rocksdb_is_write_stopped: IntGaugeVec,
    pub rocksdb_block_cache_capacity: IntGaugeVec,
    pub rocksdb_block_cache_usage: IntGaugeVec,
    pub rocksdb_block_cache_pinned_usage: IntGaugeVec,
    pub rocksdb_estimate_table_readers_mem: IntGaugeVec,
    pub rocksdb_num_immutable_mem_tables: IntGaugeVec,
    pub rocksdb_mem_table_flush_pending: IntGaugeVec,
    pub rocksdb_compaction_pending: IntGaugeVec,
    pub rocksdb_estimate_pending_compaction_bytes: IntGaugeVec,
    pub rocksdb_num_running_compactions: IntGaugeVec,
    pub rocksdb_num_running_flushes: IntGaugeVec,
    pub rocksdb_estimate_oldest_key_time: IntGaugeVec,
    pub rocksdb_background_errors: IntGaugeVec,
    pub rocksdb_estimated_num_keys: IntGaugeVec,
    pub rocksdb_base_level: IntGaugeVec,
}

impl ColumnFamilyMetrics {
    pub(crate) fn new(registry: &Registry) -> Self {
        ColumnFamilyMetrics {
            rocksdb_total_sst_files_size: register_int_gauge_vec_with_registry!(
                "rocksdb_total_sst_files_size",
                "The storage size occupied by the sst files in the column family",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_total_blob_files_size: register_int_gauge_vec_with_registry!(
                "rocksdb_total_blob_files_size",
                "The storage size occupied by the blob files in the column family",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_total_num_files: register_int_gauge_vec_with_registry!(
                "rocksdb_total_num_files",
                "Total number of files used in the column family",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_num_level0_files: register_int_gauge_vec_with_registry!(
                "rocksdb_num_level0_files",
                "Number of level 0 files in the column family",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_current_size_active_mem_tables: register_int_gauge_vec_with_registry!(
                "rocksdb_current_size_active_mem_tables",
                "The current approximate size of active memtable (bytes).",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_size_all_mem_tables: register_int_gauge_vec_with_registry!(
                "rocksdb_size_all_mem_tables",
                "The memory size occupied by the column family's in-memory buffer",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_num_snapshots: register_int_gauge_vec_with_registry!(
                "rocksdb_num_snapshots",
                "Number of snapshots held for the column family",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_oldest_snapshot_time: register_int_gauge_vec_with_registry!(
                "rocksdb_oldest_snapshot_time",
                "Unit timestamp of the oldest unreleased snapshot",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_actual_delayed_write_rate: register_int_gauge_vec_with_registry!(
                "rocksdb_actual_delayed_write_rate",
                "The current actual delayed write rate. 0 means no delay",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_is_write_stopped: register_int_gauge_vec_with_registry!(
                "rocksdb_is_write_stopped",
                "A flag indicating whether writes are stopped on this column family. 1 indicates writes have been stopped.",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_block_cache_capacity: register_int_gauge_vec_with_registry!(
                "rocksdb_block_cache_capacity",
                "The block cache capacity of the column family.",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_block_cache_usage: register_int_gauge_vec_with_registry!(
                "rocksdb_block_cache_usage",
                "The memory size used by the column family in the block cache.",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_block_cache_pinned_usage: register_int_gauge_vec_with_registry!(
                "rocksdb_block_cache_pinned_usage",
                "The memory size used by the column family in the block cache where entries are pinned",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_estimate_table_readers_mem: register_int_gauge_vec_with_registry!(
                "rocksdb_estimate_table_readers_mem",
                "The estimated memory size used for reading SST tables in this column family such as filters and index blocks. Note that this number does not include the memory used in block cache.",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_num_immutable_mem_tables: register_int_gauge_vec_with_registry!(
                "rocksdb_num_immutable_mem_tables",
                "The number of immutable memtables that have not yet been flushed.",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_mem_table_flush_pending: register_int_gauge_vec_with_registry!(
                "rocksdb_mem_table_flush_pending",
                "A 1 or 0 flag indicating whether a memtable flush is pending. If this number is 1, it means a memtable is waiting for being flushed, but there might be too many L0 files that prevents it from being flushed.",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_compaction_pending: register_int_gauge_vec_with_registry!(
                "rocksdb_compaction_pending",
                "A 1 or 0 flag indicating whether a compaction job is pending. If this number is 1, it means some part of the column family requires compaction in order to maintain shape of LSM tree, but the compaction is pending because the desired compaction job is either waiting for other dependent compactions to be finished or waiting for an available compaction thread.",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_estimate_pending_compaction_bytes: register_int_gauge_vec_with_registry!(
                "rocksdb_estimate_pending_compaction_bytes",
                "Estimated total number of bytes compaction needs to rewrite to get all levels down to under target size. Not valid for other compactions than level-based.",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_num_running_compactions: register_int_gauge_vec_with_registry!(
                "rocksdb_num_running_compactions",
                "The number of compactions that are currently running for the column family.",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_num_running_flushes: register_int_gauge_vec_with_registry!(
                "rocksdb_num_running_flushes",
                "The number of flushes that are currently running for the column family.",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_estimate_oldest_key_time: register_int_gauge_vec_with_registry!(
                "rocksdb_estimate_oldest_key_time",
                "Estimation of the oldest key timestamp in the DB. Only available for FIFO compaction with compaction_options_fifo.allow_compaction = false.",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_estimated_num_keys: register_int_gauge_vec_with_registry!(
                "rocksdb_estimated_num_keys",
                "The estimated number of keys in the table",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_background_errors: register_int_gauge_vec_with_registry!(
                "rocksdb_background_errors",
                "The accumulated number of RocksDB background errors.",
                &["cf_name"],
                registry,
            )
            .unwrap(),
            rocksdb_base_level: register_int_gauge_vec_with_registry!(
                "rocksdb_base_level",
                "The number of level to which L0 data will be compacted.",
                &["cf_name"],
                registry,
            )
            .unwrap(),
        }
    }
}

pub struct ConsistentStoreMetrics {
    pub cf_metrics: ColumnFamilyMetrics,
}

static ONCE: OnceCell<Arc<ConsistentStoreMetrics>> = OnceCell::new();

impl ConsistentStoreMetrics {
    pub fn new(registry: &Registry) -> Self {
        ConsistentStoreMetrics {
            cf_metrics: ColumnFamilyMetrics::new(registry),
        }
    }

    pub fn init(registry: &Registry) -> &'static Arc<ConsistentStoreMetrics> {
        let _ = ONCE
            .set(Arc::new(ConsistentStoreMetrics::new(registry)))
            .tap_err(|_| warn!("ConsistentStoreMetrics registry overwritten"));
        ONCE.get().unwrap()
    }

    pub fn get() -> &'static Arc<ConsistentStoreMetrics> {
        ONCE.get()
            .unwrap_or_else(|| ConsistentStoreMetrics::init(&Registry::new()))
    }
}

// Periodic metrics reporting functionality for consistent store Db
pub fn start_periodic_metrics_reporting_consistent_store(
    db: Arc<crate::db::Db>,
    cf_names: Vec<String>,
    cancel_token: tokio_util::sync::CancellationToken,
) {
    let metrics = ConsistentStoreMetrics::get();

    tokio::task::spawn(async move {
        let mut interval =
            tokio::time::interval(Duration::from_secs(CF_METRICS_REPORT_PERIOD_SECS));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    for cf_name in &cf_names {
                        if let Err(e) = tokio::task::spawn_blocking({
                            let db = db.clone();
                            let cf_name = cf_name.clone();
                            let metrics = metrics.clone();
                            move || {
                                report_rocksdb_metrics_consistent_store(&db, &cf_name, &metrics);
                            }
                        }).await {
                            error!("Failed to report metrics for cf {}: {}", cf_name, e);
                        }
                    }
                }
                _ = cancel_token.cancelled() => {
                    debug!("Periodic metrics reporting cancelled");
                    break;
                }
            }
        }
    });
}

fn report_rocksdb_metrics_consistent_store(
    db: &crate::db::Db,
    cf_name: &str,
    metrics: &Arc<ConsistentStoreMetrics>,
) {
    let Some(cf) = db.cf(cf_name) else {
        warn!("unable to report metrics for cf {cf_name:?} in db",);
        return;
    };

    // Access the underlying RocksDB database through the closure method
    db.with_rocksdb_db(|rocks_db| {
        // Now we can access RocksDB properties
        metrics
            .cf_metrics
            .rocksdb_total_sst_files_size
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::TOTAL_SST_FILES_SIZE,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_total_blob_files_size
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    ROCKSDB_PROPERTY_TOTAL_BLOB_FILES_SIZE,
                )
                .unwrap_or(METRICS_ERROR),
            );

        // Calculate total number of files across all levels
        let total_num_files: i64 = (0..=6)
            .map(|level| {
                get_rocksdb_int_property_consistent_store(rocks_db, &cf, &num_files_at_level(level))
                    .unwrap_or(METRICS_ERROR)
            })
            .sum();

        metrics
            .cf_metrics
            .rocksdb_total_num_files
            .with_label_values(&[cf_name])
            .set(total_num_files);

        metrics
            .cf_metrics
            .rocksdb_num_level0_files
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(rocks_db, &cf, &num_files_at_level(0))
                    .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_current_size_active_mem_tables
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::CUR_SIZE_ACTIVE_MEM_TABLE,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_size_all_mem_tables
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::SIZE_ALL_MEM_TABLES,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_num_snapshots
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(rocks_db, &cf, properties::NUM_SNAPSHOTS)
                    .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_oldest_snapshot_time
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::OLDEST_SNAPSHOT_TIME,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_actual_delayed_write_rate
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::ACTUAL_DELAYED_WRITE_RATE,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_is_write_stopped
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::IS_WRITE_STOPPED,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_block_cache_capacity
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::BLOCK_CACHE_CAPACITY,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_block_cache_usage
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::BLOCK_CACHE_USAGE,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_block_cache_pinned_usage
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::BLOCK_CACHE_PINNED_USAGE,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_estimate_table_readers_mem
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::ESTIMATE_TABLE_READERS_MEM,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_estimated_num_keys
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::ESTIMATE_NUM_KEYS,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_num_immutable_mem_tables
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::NUM_IMMUTABLE_MEM_TABLE,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_mem_table_flush_pending
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::MEM_TABLE_FLUSH_PENDING,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_compaction_pending
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::COMPACTION_PENDING,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_estimate_pending_compaction_bytes
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::ESTIMATE_PENDING_COMPACTION_BYTES,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_num_running_compactions
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::NUM_RUNNING_COMPACTIONS,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_num_running_flushes
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::NUM_RUNNING_FLUSHES,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_estimate_oldest_key_time
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::ESTIMATE_OLDEST_KEY_TIME,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_background_errors
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(
                    rocks_db,
                    &cf,
                    properties::BACKGROUND_ERRORS,
                )
                .unwrap_or(METRICS_ERROR),
            );

        metrics
            .cf_metrics
            .rocksdb_base_level
            .with_label_values(&[cf_name])
            .set(
                get_rocksdb_int_property_consistent_store(rocks_db, &cf, properties::BASE_LEVEL)
                    .unwrap_or(METRICS_ERROR),
            );
    });
}

fn get_rocksdb_int_property_consistent_store(
    db: &rocksdb::DB,
    cf: &impl AsColumnFamilyRef,
    property_name: &std::ffi::CStr,
) -> Result<i64, String> {
    match db.property_int_value_cf(cf, property_name) {
        Ok(Some(value)) => Ok(value.min(i64::MAX as u64).try_into().unwrap_or_default()),
        Ok(None) => Ok(0),
        Err(e) => Err(e.into_string()),
    }
}
