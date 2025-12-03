// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use prometheus::{
    IntGaugeVec, Opts,
    core::{Collector, Desc},
    proto::MetricFamily,
};

pub struct ColumnFamilyStatsCollector {
    db: Arc<crate::db::Db>,
    metrics: ColumnFamilyMetrics,
    cf_names: Vec<String>,
}

#[derive(Debug)]
pub struct ColumnFamilyMetrics {
    /// Size of the active memtable in bytes.
    pub current_size_active_mem_tables: IntGaugeVec,
    /// Size of active, unflushed immutable, and pinned memtable in bytes.
    pub size_all_mem_tables: IntGaugeVec,
    /// Memory size for the entries residing in the block cache.
    pub block_cache_usage: IntGaugeVec,
    /// Memory size of entries pinned in the block cache.
    pub block_cache_pinned_usage: IntGaugeVec,
    /// Estimated memory used by SST table readers, not including memory used.
    pub estimate_table_readers_mem: IntGaugeVec,
    /// Total number of bytes that need to be compacted to get all levels down to under target size.
    pub estimate_pending_compaction_bytes: IntGaugeVec,
    /// Number of L0 files.
    pub num_level0_files: IntGaugeVec,
    /// Number of immutable memtables that have not yet been flushed.
    pub num_immutable_mem_tables: IntGaugeVec,
    /// Boolean flag (0/1) indicating whether a memtable flush is pending.
    pub mem_table_flush_pending: IntGaugeVec,
    /// Boolean flag (0/1) indicating whether a compaction is pending.
    pub compaction_pending: IntGaugeVec,
    /// Number of snapshots.
    pub num_snapshots: IntGaugeVec,
    /// Number of running compactions.
    pub num_running_compactions: IntGaugeVec,
    /// Number of running flushes.
    pub num_running_flushes: IntGaugeVec,
    /// The current delayed write rate. 0 means no delay.
    pub actual_delayed_write_rate: IntGaugeVec,
    /// Boolean flag (0/1) indicating whether RocksDB has stopped all writes.
    pub is_write_stopped: IntGaugeVec,
}

impl ColumnFamilyStatsCollector {
    pub fn new(prefix: Option<&str>, db: Arc<crate::db::Db>, cf_names: Vec<String>) -> Self {
        let metrics = ColumnFamilyMetrics::new(prefix);

        Self {
            db,
            metrics,
            cf_names,
        }
    }
}

/// Create metrics without registering them
impl ColumnFamilyMetrics {
    pub(crate) fn new(prefix: Option<&str>) -> Self {
        let prefix = prefix.unwrap_or("rocksdb");
        let name = |n| format!("{prefix}_{n}");

        Self {
            num_level0_files: IntGaugeVec::new(
                Opts::new(name("num_level0_files"), "Number of level 0 files in the column family"),
                &["cf_name"],
            ).unwrap(),
            current_size_active_mem_tables: IntGaugeVec::new(
                Opts::new(name("current_size_active_mem_tables"), "The current approximate size of active memtable (bytes)."),
                &["cf_name"],
            ).unwrap(),
            size_all_mem_tables: IntGaugeVec::new(
                Opts::new(name("size_all_mem_tables"), "The memory size occupied by the column family's in-memory buffer"),
                &["cf_name"],
            ).unwrap(),
            num_snapshots: IntGaugeVec::new(
                Opts::new(name("num_snapshots"), "Number of snapshots held for the column family"),
                &["cf_name"],
            ).unwrap(),
            actual_delayed_write_rate: IntGaugeVec::new(
                Opts::new(name("actual_delayed_write_rate"), "The current actual delayed write rate. 0 means no delay"),
                &["cf_name"],
            ).unwrap(),
            is_write_stopped: IntGaugeVec::new(
                Opts::new(name("is_write_stopped"), "A flag indicating whether writes are stopped on this column family. 1 indicates writes have been stopped."),
                &["cf_name"],
            ).unwrap(),
            block_cache_usage: IntGaugeVec::new(
                Opts::new(name("block_cache_usage"), "The memory size used by the column family in the block cache."),
                &["cf_name"],
            ).unwrap(),
            block_cache_pinned_usage: IntGaugeVec::new(
                Opts::new(name("block_cache_pinned_usage"), "The memory size used by the column family in the block cache where entries are pinned"),
                &["cf_name"],
            ).unwrap(),
            estimate_table_readers_mem: IntGaugeVec::new(
                Opts::new(name("estimate_table_readers_mem"), "The estimated memory size used for reading SST tables in this column family such as filters and index blocks. Note that this number does not include the memory used in block cache."),
                &["cf_name"],
            ).unwrap(),
            num_immutable_mem_tables: IntGaugeVec::new(
                Opts::new(name("num_immutable_mem_tables"), "The number of immutable memtables that have not yet been flushed."),
                &["cf_name"],
            ).unwrap(),
            mem_table_flush_pending: IntGaugeVec::new(
                Opts::new(name("mem_table_flush_pending"), "A 1 or 0 flag indicating whether a memtable flush is pending. If this number is 1, it means a memtable is waiting for being flushed, but there might be too many L0 files that prevents it from being flushed."),
                &["cf_name"],
            ).unwrap(),
            compaction_pending: IntGaugeVec::new(
                Opts::new(name("compaction_pending"), "A 1 or 0 flag indicating whether a compaction job is pending. If this number is 1, it means some part of the column family requires compaction in order to maintain shape of LSM tree, but the compaction is pending because the desired compaction job is either waiting for other dependent compactions to be finished or waiting for an available compaction thread."),
                &["cf_name"],
            ).unwrap(),
            estimate_pending_compaction_bytes: IntGaugeVec::new(
                Opts::new(name("estimate_pending_compaction_bytes"), "Estimated total number of bytes compaction needs to rewrite to get all levels down to under target size. Not valid for other compactions than level-based."),
                &["cf_name"],
            ).unwrap(),
            num_running_compactions: IntGaugeVec::new(
                Opts::new(name("num_running_compactions"), "The number of compactions that are currently running for the column family."),
                &["cf_name"],
            ).unwrap(),
            num_running_flushes: IntGaugeVec::new(
                Opts::new(name("num_running_flushes"), "The number of flushes that are currently running for the column family."),
                &["cf_name"],
            ).unwrap(),
        }
    }

    pub(crate) fn desc(&self) -> Vec<&Desc> {
        vec![
            self.num_level0_files.desc(),
            self.current_size_active_mem_tables.desc(),
            self.size_all_mem_tables.desc(),
            self.num_snapshots.desc(),
            self.actual_delayed_write_rate.desc(),
            self.is_write_stopped.desc(),
            self.block_cache_usage.desc(),
            self.block_cache_pinned_usage.desc(),
            self.estimate_table_readers_mem.desc(),
            self.num_immutable_mem_tables.desc(),
            self.mem_table_flush_pending.desc(),
            self.compaction_pending.desc(),
            self.estimate_pending_compaction_bytes.desc(),
            self.num_running_compactions.desc(),
            self.num_running_flushes.desc(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    pub(crate) fn collect_all(&self) -> Vec<MetricFamily> {
        vec![
            self.num_level0_files.collect(),
            self.current_size_active_mem_tables.collect(),
            self.size_all_mem_tables.collect(),
            self.num_snapshots.collect(),
            self.actual_delayed_write_rate.collect(),
            self.is_write_stopped.collect(),
            self.block_cache_usage.collect(),
            self.block_cache_pinned_usage.collect(),
            self.estimate_table_readers_mem.collect(),
            self.num_immutable_mem_tables.collect(),
            self.mem_table_flush_pending.collect(),
            self.compaction_pending.collect(),
            self.estimate_pending_compaction_bytes.collect(),
            self.num_running_compactions.collect(),
            self.num_running_flushes.collect(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

impl Collector for ColumnFamilyStatsCollector {
    fn desc(&self) -> Vec<&Desc> {
        self.metrics.desc()
    }

    fn collect(&self) -> Vec<MetricFamily> {
        for cf_name in &self.cf_names {
            let cf_metrics = self.db.column_family_metrics(cf_name);
            self.metrics
                .current_size_active_mem_tables
                .with_label_values(&[cf_name])
                .set(cf_metrics.current_size_active_mem_tables);

            self.metrics
                .size_all_mem_tables
                .with_label_values(&[cf_name])
                .set(cf_metrics.size_all_mem_tables);

            self.metrics
                .block_cache_usage
                .with_label_values(&[cf_name])
                .set(cf_metrics.block_cache_usage);

            self.metrics
                .block_cache_pinned_usage
                .with_label_values(&[cf_name])
                .set(cf_metrics.block_cache_pinned_usage);

            self.metrics
                .estimate_table_readers_mem
                .with_label_values(&[cf_name])
                .set(cf_metrics.estimate_table_readers_mem);

            self.metrics
                .estimate_pending_compaction_bytes
                .with_label_values(&[cf_name])
                .set(cf_metrics.estimate_pending_compaction_bytes);

            self.metrics
                .num_level0_files
                .with_label_values(&[cf_name])
                .set(cf_metrics.num_level0_files);

            self.metrics
                .actual_delayed_write_rate
                .with_label_values(&[cf_name])
                .set(cf_metrics.actual_delayed_write_rate);

            self.metrics
                .is_write_stopped
                .with_label_values(&[cf_name])
                .set(cf_metrics.is_write_stopped);

            self.metrics
                .num_immutable_mem_tables
                .with_label_values(&[cf_name])
                .set(cf_metrics.num_immutable_mem_tables);

            self.metrics
                .mem_table_flush_pending
                .with_label_values(&[cf_name])
                .set(cf_metrics.mem_table_flush_pending);

            self.metrics
                .compaction_pending
                .with_label_values(&[cf_name])
                .set(cf_metrics.compaction_pending);

            self.metrics
                .num_snapshots
                .with_label_values(&[cf_name])
                .set(cf_metrics.num_snapshots);

            self.metrics
                .num_running_compactions
                .with_label_values(&[cf_name])
                .set(cf_metrics.num_running_compactions);

            self.metrics
                .num_running_flushes
                .with_label_values(&[cf_name])
                .set(cf_metrics.num_running_flushes);
        }

        self.metrics.collect_all()
    }
}
