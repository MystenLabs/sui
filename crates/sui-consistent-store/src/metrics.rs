// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A canonical [`prometheus::core::Collector`] that exposes the
//! RocksDB integer properties surfaced by
//! [`Db::cf_metrics`](crate::Db::cf_metrics) as labeled
//! `IntGauge`s, one set of gauges per registered column family.
//!
//! # Registering
//!
//! Register on a [`prometheus::Registry`] the same way you would
//! any other collector:
//!
//! ```
//! use prometheus::Registry;
//! use sui_consistent_store::Db;
//! use sui_consistent_store::DbOptions;
//! use sui_consistent_store::Schema;
//! use sui_consistent_store::metrics::ColumnFamilyStatsCollector;
//!
//! #[derive(Debug)]
//! struct EmptySchema;
//!
//! impl Schema for EmptySchema {
//!     fn cfs(_: &rocksdb::Options) -> Vec<sui_consistent_store::CfDescriptor> {
//!         vec![]
//!     }
//!     fn open(_: &Db) -> Result<Self, sui_consistent_store::error::OpenError> {
//!         Ok(Self)
//!     }
//! }
//!
//! let dir = tempfile::tempdir().unwrap();
//! let (db, _schema) = Db::open::<EmptySchema>(dir.path(), DbOptions::default()).unwrap();
//! let registry = Registry::new();
//! let collector = ColumnFamilyStatsCollector::new(None, &db);
//! registry.register(Box::new(collector)).unwrap();
//! ```
//!
//! # Liveness
//!
//! The collector holds a [`DbRef`] (weak handle), so
//! registering it does not keep the underlying database open. If
//! every strong [`Db`] handle has been dropped by the
//! time Prometheus scrapes, every gauge is reset to `-1` so
//! dashboards see the database is gone rather than stale-looking
//! values.
//!
//! # Sampling cadence
//!
//! Sampling happens on Prometheus scrape: every
//! `collect()` walks every column family in the underlying [`Db`]
//! and reads each property. This is cheap (each property is one
//! RocksDB API call) but not free, so a tight scrape interval over
//! many CFs has a measurable cost.

use prometheus::IntGaugeVec;
use prometheus::Opts;
use prometheus::core::Collector;
use prometheus::core::Desc;
use prometheus::proto::MetricFamily;

use crate::Db;
use crate::DbRef;
use crate::RocksMetrics;

/// Default prefix applied to every metric name when the caller
/// passes `None` to [`ColumnFamilyStatsCollector::new`].
const DEFAULT_PREFIX: &str = "rocksdb";

/// A [`prometheus::core::Collector`] that emits per-column-family
/// RocksDB integer properties as labeled `IntGauge`s.
///
/// Each metric has a single `cf_name` label whose value is the
/// column-family name; every CF the [`Db`] knew about at
/// construction (both schema-declared and framework-internal) is
/// sampled on every collect. The collector holds a
/// [`DbRef`] (weak handle), so registering it does not keep the
/// database alive — once every strong [`Db`] has been dropped,
/// subsequent scrapes report every gauge as
/// [`METRICS_ERROR`](crate::RocksMetrics).
pub struct ColumnFamilyStatsCollector {
    db: DbRef,
    /// Snapshot of `db.cf_names()` taken at construction time so
    /// the collector can emit one series per CF even after the
    /// underlying [`Db`] has been dropped. CFs are fixed once a
    /// database is open, so the snapshot does not go stale.
    cf_names: Vec<&'static str>,
    gauges: Gauges,
}

impl ColumnFamilyStatsCollector {
    /// Build a collector backed by `db`. `prefix` is prepended to
    /// every metric name (default: `"rocksdb"`). Holds a weak
    /// reference to `db`, so the collector does not keep the
    /// database alive.
    pub fn new(prefix: Option<&str>, db: &Db) -> Self {
        Self {
            db: db.downgrade(),
            cf_names: db.cf_names().to_vec(),
            gauges: Gauges::new(prefix.unwrap_or(DEFAULT_PREFIX)),
        }
    }

    /// Populate every gauge for `cf_name` from a single
    /// [`RocksMetrics`] sample. Shared by the live-DB and
    /// dropped-DB collection paths so both branches stay in sync.
    fn populate(&self, cf_name: &str, m: &RocksMetrics) {
        let labels = [cf_name];
        self.gauges
            .block_cache_capacity
            .with_label_values(&labels)
            .set(m.block_cache_capacity);
        self.gauges
            .block_cache_usage
            .with_label_values(&labels)
            .set(m.block_cache_usage);
        self.gauges
            .block_cache_pinned_usage
            .with_label_values(&labels)
            .set(m.block_cache_pinned_usage);
        self.gauges
            .current_size_active_mem_tables
            .with_label_values(&labels)
            .set(m.current_size_active_mem_tables);
        self.gauges
            .size_all_mem_tables
            .with_label_values(&labels)
            .set(m.size_all_mem_tables);
        self.gauges
            .num_immutable_mem_tables
            .with_label_values(&labels)
            .set(m.num_immutable_mem_tables);
        self.gauges
            .mem_table_flush_pending
            .with_label_values(&labels)
            .set(m.mem_table_flush_pending);
        self.gauges
            .estimate_table_readers_mem
            .with_label_values(&labels)
            .set(m.estimate_table_readers_mem);
        self.gauges
            .num_level0_files
            .with_label_values(&labels)
            .set(m.num_level0_files);
        self.gauges
            .base_level
            .with_label_values(&labels)
            .set(m.base_level);
        self.gauges
            .compaction_pending
            .with_label_values(&labels)
            .set(m.compaction_pending);
        self.gauges
            .num_running_compactions
            .with_label_values(&labels)
            .set(m.num_running_compactions);
        self.gauges
            .num_running_flushes
            .with_label_values(&labels)
            .set(m.num_running_flushes);
        self.gauges
            .estimate_pending_compaction_bytes
            .with_label_values(&labels)
            .set(m.estimate_pending_compaction_bytes);
        self.gauges
            .num_snapshots
            .with_label_values(&labels)
            .set(m.num_snapshots);
        self.gauges
            .oldest_snapshot_time
            .with_label_values(&labels)
            .set(m.oldest_snapshot_time);
        self.gauges
            .estimate_oldest_key_time
            .with_label_values(&labels)
            .set(m.estimate_oldest_key_time);
        self.gauges
            .estimated_num_keys
            .with_label_values(&labels)
            .set(m.estimated_num_keys);
        self.gauges
            .background_errors
            .with_label_values(&labels)
            .set(m.background_errors);
        self.gauges
            .total_sst_files_size
            .with_label_values(&labels)
            .set(m.total_sst_files_size);
        self.gauges
            .total_blob_files_size
            .with_label_values(&labels)
            .set(m.total_blob_files_size);
        self.gauges
            .actual_delayed_write_rate
            .with_label_values(&labels)
            .set(m.actual_delayed_write_rate);
        self.gauges
            .is_write_stopped
            .with_label_values(&labels)
            .set(m.is_write_stopped);
    }
}

impl Collector for ColumnFamilyStatsCollector {
    fn desc(&self) -> Vec<&Desc> {
        self.gauges.descs()
    }

    fn collect(&self) -> Vec<MetricFamily> {
        // If the database has been closed, fall back to a default
        // (METRICS_ERROR-filled) sample for every CF so dashboards
        // see the database is gone instead of stale values.
        let maybe_db = self.db.upgrade();
        for &cf_name in &self.cf_names {
            let m = match &maybe_db {
                Some(db) => db.cf_metrics(cf_name),
                None => RocksMetrics::default(),
            };
            self.populate(cf_name, &m);
        }
        self.gauges.collect_all()
    }
}

/// Bundle of `IntGaugeVec`s, one per field of
/// [`RocksMetrics`](crate::RocksMetrics).
struct Gauges {
    block_cache_capacity: IntGaugeVec,
    block_cache_usage: IntGaugeVec,
    block_cache_pinned_usage: IntGaugeVec,
    current_size_active_mem_tables: IntGaugeVec,
    size_all_mem_tables: IntGaugeVec,
    num_immutable_mem_tables: IntGaugeVec,
    mem_table_flush_pending: IntGaugeVec,
    estimate_table_readers_mem: IntGaugeVec,
    num_level0_files: IntGaugeVec,
    base_level: IntGaugeVec,
    compaction_pending: IntGaugeVec,
    num_running_compactions: IntGaugeVec,
    num_running_flushes: IntGaugeVec,
    estimate_pending_compaction_bytes: IntGaugeVec,
    num_snapshots: IntGaugeVec,
    oldest_snapshot_time: IntGaugeVec,
    estimate_oldest_key_time: IntGaugeVec,
    estimated_num_keys: IntGaugeVec,
    background_errors: IntGaugeVec,
    total_sst_files_size: IntGaugeVec,
    total_blob_files_size: IntGaugeVec,
    actual_delayed_write_rate: IntGaugeVec,
    is_write_stopped: IntGaugeVec,
}

impl Gauges {
    fn new(prefix: &str) -> Self {
        let g = |name: &str, help: &str| {
            IntGaugeVec::new(Opts::new(format!("{prefix}_{name}"), help), &["cf_name"]).unwrap()
        };
        Self {
            block_cache_capacity: g(
                "block_cache_capacity",
                "Configured block cache capacity (bytes).",
            ),
            block_cache_usage: g(
                "block_cache_usage",
                "Memory size of entries in the block cache.",
            ),
            block_cache_pinned_usage: g(
                "block_cache_pinned_usage",
                "Memory size of entries pinned in the block cache.",
            ),
            current_size_active_mem_tables: g(
                "current_size_active_mem_tables",
                "Approximate size of the active memtable (bytes).",
            ),
            size_all_mem_tables: g(
                "size_all_mem_tables",
                "Active + unflushed immutable + pinned memtable size (bytes).",
            ),
            num_immutable_mem_tables: g(
                "num_immutable_mem_tables",
                "Number of immutable memtables not yet flushed.",
            ),
            mem_table_flush_pending: g(
                "mem_table_flush_pending",
                "1 if a memtable flush is pending, else 0.",
            ),
            estimate_table_readers_mem: g(
                "estimate_table_readers_mem",
                "Approximate memory used by table readers (excluding block cache).",
            ),
            num_level0_files: g("num_level0_files", "Number of level-0 SST files."),
            base_level: g("base_level", "Current base level of the LSM tree."),
            compaction_pending: g(
                "compaction_pending",
                "1 if a compaction is pending, else 0.",
            ),
            num_running_compactions: g(
                "num_running_compactions",
                "Number of currently running compactions.",
            ),
            num_running_flushes: g(
                "num_running_flushes",
                "Number of currently running flushes.",
            ),
            estimate_pending_compaction_bytes: g(
                "estimate_pending_compaction_bytes",
                "Estimated bytes the compaction backlog will rewrite.",
            ),
            num_snapshots: g(
                "num_snapshots",
                "Number of unreleased rocksdb::Snapshot handles.",
            ),
            oldest_snapshot_time: g(
                "oldest_snapshot_time",
                "Unix time (seconds) of the oldest live snapshot.",
            ),
            estimate_oldest_key_time: g(
                "estimate_oldest_key_time",
                "Unix time (seconds) estimate of the oldest live key.",
            ),
            estimated_num_keys: g("estimated_num_keys", "Approximate number of live keys."),
            background_errors: g("background_errors", "Accumulated background errors."),
            total_sst_files_size: g(
                "total_sst_files_size",
                "Total size of all SST files (bytes).",
            ),
            total_blob_files_size: g(
                "total_blob_files_size",
                "Total size of all blob files (bytes).",
            ),
            actual_delayed_write_rate: g(
                "actual_delayed_write_rate",
                "Current write-rate throttle level (bytes/sec, 0 when not throttled).",
            ),
            is_write_stopped: g("is_write_stopped", "1 if writes are stopped, else 0."),
        }
    }

    fn descs(&self) -> Vec<&Desc> {
        let mut out = vec![];
        for g in self.all() {
            out.extend(g.desc());
        }
        out
    }

    fn collect_all(&self) -> Vec<MetricFamily> {
        let mut out = vec![];
        for g in self.all() {
            out.extend(g.collect());
        }
        out
    }

    fn all(&self) -> [&IntGaugeVec; 23] {
        [
            &self.block_cache_capacity,
            &self.block_cache_usage,
            &self.block_cache_pinned_usage,
            &self.current_size_active_mem_tables,
            &self.size_all_mem_tables,
            &self.num_immutable_mem_tables,
            &self.mem_table_flush_pending,
            &self.estimate_table_readers_mem,
            &self.num_level0_files,
            &self.base_level,
            &self.compaction_pending,
            &self.num_running_compactions,
            &self.num_running_flushes,
            &self.estimate_pending_compaction_bytes,
            &self.num_snapshots,
            &self.oldest_snapshot_time,
            &self.estimate_oldest_key_time,
            &self.estimated_num_keys,
            &self.background_errors,
            &self.total_sst_files_size,
            &self.total_blob_files_size,
            &self.actual_delayed_write_rate,
            &self.is_write_stopped,
        ]
    }
}

#[cfg(test)]
mod tests {
    use prometheus::Registry;
    use prometheus::core::Collector;
    use tempfile::TempDir;

    use super::*;
    use crate::CfDescriptor;
    use crate::DbOptions;
    use crate::Schema;
    use crate::error::OpenError;

    #[derive(Debug)]
    struct TwoCfSchema;

    impl Schema for TwoCfSchema {
        fn cfs(base_options: &rocksdb::Options) -> Vec<CfDescriptor> {
            vec![
                CfDescriptor::new("alpha", base_options.clone()),
                CfDescriptor::new("beta", base_options.clone()),
            ]
        }

        fn open(_: &Db) -> Result<Self, OpenError> {
            Ok(Self)
        }
    }

    fn open_db() -> (TempDir, Db) {
        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<TwoCfSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db)
    }

    #[test]
    fn cf_names_includes_user_default_and_framework_cfs() {
        let (_dir, db) = open_db();
        let names: Vec<&str> = db.cf_names().to_vec();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
        assert!(names.contains(&"default"));
        assert!(names.contains(&"__restore"));
        assert!(names.contains(&"__watermark"));
        assert!(names.contains(&"__chain_id"));
    }

    #[test]
    fn collector_registers_and_collects_for_every_cf() {
        let (_dir, db) = open_db();
        let registry = Registry::new();
        let collector = ColumnFamilyStatsCollector::new(None, &db);
        registry.register(Box::new(collector)).unwrap();

        // Gather metric families and confirm at least one
        // `cf_name="alpha"` series shows up across the families.
        let families = registry.gather();
        let mut saw_alpha = false;
        let mut saw_beta = false;
        for family in &families {
            for metric in family.get_metric() {
                for label in metric.get_label() {
                    if label.name() == "cf_name" {
                        match label.value() {
                            "alpha" => saw_alpha = true,
                            "beta" => saw_beta = true,
                            _ => {}
                        }
                    }
                }
            }
        }
        assert!(saw_alpha, "alpha cf metrics should be emitted");
        assert!(saw_beta, "beta cf metrics should be emitted");
    }

    #[test]
    fn prefix_override_changes_metric_names() {
        let (_dir, db) = open_db();
        let collector = ColumnFamilyStatsCollector::new(Some("my_app"), &db);
        let descs = collector.desc();
        assert!(descs.iter().all(|d| d.fq_name.starts_with("my_app_")));
    }

    #[test]
    fn default_prefix_is_rocksdb() {
        let (_dir, db) = open_db();
        let collector = ColumnFamilyStatsCollector::new(None, &db);
        let descs = collector.desc();
        assert!(descs.iter().all(|d| d.fq_name.starts_with("rocksdb_")));
    }

    #[test]
    fn collector_does_not_keep_db_alive() {
        // Build a collector, drop the strong Db handle, then
        // confirm the underlying `DbRef` can no longer upgrade —
        // the collector's weak handle must not pin the Db.
        let (_dir, db) = open_db();
        let collector = ColumnFamilyStatsCollector::new(None, &db);
        let weak = db.downgrade();
        drop(db);
        drop(collector);
        assert!(
            weak.upgrade().is_none(),
            "Db should drop once every strong handle is released",
        );
    }

    #[test]
    fn collect_after_db_drop_emits_metrics_error_sentinel() {
        // After the Db is dropped, every gauge must read
        // METRICS_ERROR (-1) so dashboards see "unavailable"
        // rather than the last live values.
        let (_dir, db) = open_db();
        let cf_names: Vec<&'static str> = db.cf_names().to_vec();
        let collector = ColumnFamilyStatsCollector::new(None, &db);
        drop(db);
        let families = collector.collect();

        let mut series_seen = 0usize;
        for family in &families {
            for metric in family.get_metric() {
                let mut matched_cf = false;
                for label in metric.get_label() {
                    if label.name() == "cf_name" && cf_names.iter().any(|cf| *cf == label.value()) {
                        matched_cf = true;
                    }
                }
                if !matched_cf {
                    continue;
                }
                series_seen += 1;
                assert_eq!(
                    metric.get_gauge().value() as i64,
                    -1,
                    "expected METRICS_ERROR sentinel, got {}",
                    metric.get_gauge().value(),
                );
            }
        }
        assert!(
            series_seen > 0,
            "collector should still emit one series per registered CF",
        );
    }
}
