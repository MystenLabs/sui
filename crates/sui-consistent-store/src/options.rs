// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tunable RocksDB options, resolved per column family.
//!
//! This module turns a serde-friendly [`RocksDbConfig`] into concrete
//! [`rocksdb::Options`] — one set of database-wide options plus one
//! set per column family. The intent is that operators tune
//! performance knobs (compression, write buffers, write-stall
//! thresholds, compaction parallelism) through configuration, while
//! correctness-bearing settings (merge operators and compaction
//! filters) stay in the schema's Rust code and are layered on top of
//! the resolved options by the schema itself.
//!
//! # Resolution model
//!
//! Every field is an [`Option`], so three states compose cleanly:
//!
//! - `None` — leave the RocksDB native default in place (the setter
//!   is simply not called).
//! - a code-supplied default — the downstream crate that owns the
//!   schema provides a fully populated [`RocksDbConfig`] as its
//!   baseline (see `sui_rpc_store::default_rocksdb_config`).
//! - a configuration override — values parsed from TOML are layered
//!   over the code defaults via [`RocksDbConfig::merge_over`].
//!
//! Within a single resolved config, a per-column-family entry in
//! [`RocksDbConfig::column_family`] is layered over
//! [`RocksDbConfig::default_cf`] at resolve time, so a CF override
//! only has to name the fields it wants to change.
//!
//! # What is *not* configurable here
//!
//! Merge operators and compaction filters are correctness-bearing and
//! deliberately absent from [`CfTuning`]. The schema attaches them in
//! code, on top of the [`rocksdb::Options`] this module produces.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

use crate::error::OpenError;

/// Compression algorithm for a column family.
///
/// Maps onto [`rocksdb::DBCompressionType`]. The variants here are the
/// ones the crate's RocksDB build enables.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Compression {
    None,
    Snappy,
    Lz4,
    Zstd,
    Zlib,
}

impl From<Compression> for rocksdb::DBCompressionType {
    fn from(c: Compression) -> Self {
        match c {
            Compression::None => Self::None,
            Compression::Snappy => Self::Snappy,
            Compression::Lz4 => Self::Lz4,
            Compression::Zstd => Self::Zstd,
            Compression::Zlib => Self::Zlib,
        }
    }
}

/// Write-stall thresholds for a column family.
///
/// RocksDB slows or stops writes when L0 file counts or the estimated
/// pending-compaction byte debt cross these thresholds. The values
/// are exposed individually (rather than behind a single "disable"
/// flag) so each can be tuned per CF, and so the chosen policy is
/// explicit rather than hidden behind a boolean.
///
/// The pending-compaction limits use a sentinel of `0` to mean
/// "disabled". The L0 file-count triggers have no disable sentinel;
/// the idiom is to set them to a value high enough never to fire.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default, deny_unknown_fields)]
pub struct WriteStallConfig {
    /// Soft limit on estimated pending-compaction bytes before writes
    /// slow down, in MiB. `0` disables the soft pending-bytes stall.
    pub soft_pending_compaction_bytes_limit_mb: Option<u64>,

    /// Hard limit on estimated pending-compaction bytes before writes
    /// stop, in MiB. `0` disables the hard pending-bytes stall.
    pub hard_pending_compaction_bytes_limit_mb: Option<u64>,

    /// Number of L0 files that triggers a compaction. Forms the floor
    /// of the L0 trigger chain; must not exceed the slowdown trigger.
    pub level0_file_num_compaction_trigger: Option<i32>,

    /// Number of L0 files at which writes are slowed. Must be greater
    /// than or equal to the compaction trigger and less than or equal
    /// to the stop trigger.
    pub level0_slowdown_writes_trigger: Option<i32>,

    /// Number of L0 files at which writes are stopped. Top of the L0
    /// trigger chain.
    pub level0_stop_writes_trigger: Option<i32>,
}

impl WriteStallConfig {
    /// Field-wise overlay: every field set on `self` wins; otherwise
    /// fall back to `base`.
    pub fn merge_over(&self, base: &Self) -> Self {
        Self {
            soft_pending_compaction_bytes_limit_mb: self
                .soft_pending_compaction_bytes_limit_mb
                .or(base.soft_pending_compaction_bytes_limit_mb),
            hard_pending_compaction_bytes_limit_mb: self
                .hard_pending_compaction_bytes_limit_mb
                .or(base.hard_pending_compaction_bytes_limit_mb),
            level0_file_num_compaction_trigger: self
                .level0_file_num_compaction_trigger
                .or(base.level0_file_num_compaction_trigger),
            level0_slowdown_writes_trigger: self
                .level0_slowdown_writes_trigger
                .or(base.level0_slowdown_writes_trigger),
            level0_stop_writes_trigger: self
                .level0_stop_writes_trigger
                .or(base.level0_stop_writes_trigger),
        }
    }

    /// Validate the relationships RocksDB requires between the set
    /// fields. Only constraints between fields that are both present
    /// are checked; absent fields fall back to RocksDB defaults that
    /// are internally consistent.
    ///
    /// `cf` names the column family for error messages.
    fn validate(&self, cf: &str) -> Result<(), OpenError> {
        let c = self.level0_file_num_compaction_trigger;
        let sd = self.level0_slowdown_writes_trigger;
        let st = self.level0_stop_writes_trigger;

        for (name, value) in [
            ("level0-file-num-compaction-trigger", c),
            ("level0-slowdown-writes-trigger", sd),
            ("level0-stop-writes-trigger", st),
        ] {
            if let Some(v) = value
                && v < 0
            {
                return Err(OpenError::msg(format!(
                    "rocksdb config for column family `{cf}`: {name} must be non-negative, got {v}"
                )));
            }
        }

        if let (Some(c), Some(sd)) = (c, sd)
            && c > sd
        {
            return Err(OpenError::msg(format!(
                "rocksdb config for column family `{cf}`: level0-file-num-compaction-trigger \
                 ({c}) must be <= level0-slowdown-writes-trigger ({sd})"
            )));
        }
        if let (Some(sd), Some(st)) = (sd, st)
            && sd > st
        {
            return Err(OpenError::msg(format!(
                "rocksdb config for column family `{cf}`: level0-slowdown-writes-trigger ({sd}) \
                 must be <= level0-stop-writes-trigger ({st})"
            )));
        }
        if let (Some(c), Some(st)) = (c, st)
            && c > st
        {
            return Err(OpenError::msg(format!(
                "rocksdb config for column family `{cf}`: level0-file-num-compaction-trigger ({c}) \
                 must be <= level0-stop-writes-trigger ({st})"
            )));
        }

        if let (Some(soft), Some(hard)) = (
            self.soft_pending_compaction_bytes_limit_mb,
            self.hard_pending_compaction_bytes_limit_mb,
        ) && soft != 0
            && hard != 0
            && soft > hard
        {
            return Err(OpenError::msg(format!(
                "rocksdb config for column family `{cf}`: soft-pending-compaction-bytes-limit-mb \
                 ({soft}) must be <= hard-pending-compaction-bytes-limit-mb ({hard})"
            )));
        }

        Ok(())
    }

    /// Apply the set fields to `opts`.
    fn apply(&self, opts: &mut rocksdb::Options) {
        if let Some(mb) = self.soft_pending_compaction_bytes_limit_mb {
            opts.set_soft_pending_compaction_bytes_limit(mib_usize(mb));
        }
        if let Some(mb) = self.hard_pending_compaction_bytes_limit_mb {
            opts.set_hard_pending_compaction_bytes_limit(mib_usize(mb));
        }
        if let Some(n) = self.level0_file_num_compaction_trigger {
            opts.set_level_zero_file_num_compaction_trigger(n);
        }
        if let Some(n) = self.level0_slowdown_writes_trigger {
            opts.set_level_zero_slowdown_writes_trigger(n);
        }
        if let Some(n) = self.level0_stop_writes_trigger {
            opts.set_level_zero_stop_writes_trigger(n);
        }
    }
}

/// Per-column-family performance tuning.
///
/// Correctness-bearing settings (merge operators, compaction filters)
/// are intentionally not here; the schema adds those in code.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default, deny_unknown_fields)]
pub struct CfTuning {
    /// Per-memtable size, in MiB.
    pub write_buffer_size_mb: Option<usize>,

    /// Maximum number of memtables before writes are throttled by
    /// flush back-pressure.
    pub max_write_buffer_number: Option<i32>,

    /// Compression for all but the bottommost level.
    pub compression: Option<Compression>,

    /// Compression for the bottommost level (where most data settles).
    pub bottommost_compression: Option<Compression>,

    /// Block size, in KiB.
    pub block_size_kb: Option<usize>,

    /// Bits per key for a full (non-block-based) bloom filter. Set on
    /// point-lookup-heavy CFs; leave unset on range-scanned CFs.
    pub bloom_filter_bits: Option<f64>,

    /// Memtable prefix-bloom size as a fraction of the memtable.
    pub memtable_prefix_bloom_ratio: Option<f64>,

    /// Target SST file size at level base, in MiB.
    pub target_file_size_mb: Option<u64>,

    /// Write-stall thresholds.
    pub write_stall: WriteStallConfig,
}

impl CfTuning {
    /// Field-wise overlay: every field set on `self` wins; otherwise
    /// fall back to `base`. The nested [`WriteStallConfig`] is merged
    /// recursively.
    pub fn merge_over(&self, base: &Self) -> Self {
        Self {
            write_buffer_size_mb: self.write_buffer_size_mb.or(base.write_buffer_size_mb),
            max_write_buffer_number: self
                .max_write_buffer_number
                .or(base.max_write_buffer_number),
            compression: self.compression.or(base.compression),
            bottommost_compression: self.bottommost_compression.or(base.bottommost_compression),
            block_size_kb: self.block_size_kb.or(base.block_size_kb),
            bloom_filter_bits: self.bloom_filter_bits.or(base.bloom_filter_bits),
            memtable_prefix_bloom_ratio: self
                .memtable_prefix_bloom_ratio
                .or(base.memtable_prefix_bloom_ratio),
            target_file_size_mb: self.target_file_size_mb.or(base.target_file_size_mb),
            write_stall: self.write_stall.merge_over(&base.write_stall),
        }
    }

    /// Build a fresh [`rocksdb::Options`] with the set fields applied.
    /// `block_cache`, when present, is shared into this CF's
    /// block-based table factory.
    fn to_options(&self, block_cache: Option<&rocksdb::Cache>) -> rocksdb::Options {
        let mut opts = rocksdb::Options::default();

        if let Some(mb) = self.write_buffer_size_mb {
            opts.set_write_buffer_size(mib_usize(mb as u64));
        }
        if let Some(n) = self.max_write_buffer_number {
            opts.set_max_write_buffer_number(n);
        }
        if let Some(c) = self.compression {
            opts.set_compression_type(c.into());
        }
        if let Some(c) = self.bottommost_compression {
            opts.set_bottommost_compression_type(c.into());
        }

        // The block-based table factory bundles block size, the shared
        // block cache, and the bloom filter, so it is constructed once
        // and only when at least one of them is configured.
        if self.block_size_kb.is_some() || block_cache.is_some() || self.bloom_filter_bits.is_some()
        {
            let mut bbt = rocksdb::BlockBasedOptions::default();
            if let Some(kb) = self.block_size_kb {
                bbt.set_block_size(kb << 10);
            }
            if let Some(cache) = block_cache {
                bbt.set_block_cache(cache);
            }
            if let Some(bits) = self.bloom_filter_bits {
                bbt.set_bloom_filter(bits, false);
            }
            opts.set_block_based_table_factory(&bbt);
        }

        if let Some(ratio) = self.memtable_prefix_bloom_ratio {
            opts.set_memtable_prefix_bloom_ratio(ratio);
        }
        if let Some(mb) = self.target_file_size_mb {
            opts.set_target_file_size_base(mb << 20);
        }

        self.write_stall.apply(&mut opts);

        opts
    }
}

/// Database-wide RocksDB options (one instance, shared by every CF).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default, deny_unknown_fields)]
pub struct DbWideConfig {
    /// Total number of background threads for compactions and flushes
    /// (`increase_parallelism`).
    pub parallelism: Option<i32>,

    /// Maximum number of concurrent background jobs.
    pub max_background_jobs: Option<i32>,

    /// Maximum number of open files (`-1` for unlimited).
    pub max_open_files: Option<i32>,

    /// Total write-buffer budget across all CFs, in MiB.
    pub db_write_buffer_size_mb: Option<usize>,

    /// Maximum total WAL size before a flush is forced, in MiB.
    pub max_total_wal_size_mb: Option<u64>,

    /// Enable pipelined writes (separate WAL and memtable write
    /// threads).
    pub enable_pipelined_write: Option<bool>,

    /// Number of shards (`2^bits`) for the table cache lock.
    pub table_cache_num_shard_bits: Option<i32>,

    /// Size of the single LRU block cache shared by every CF, in MiB.
    /// When unset, each CF uses its own RocksDB-default cache.
    pub block_cache_size_mb: Option<usize>,
}

impl DbWideConfig {
    /// Field-wise overlay: every field set on `self` wins; otherwise
    /// fall back to `base`.
    pub fn merge_over(&self, base: &Self) -> Self {
        Self {
            parallelism: self.parallelism.or(base.parallelism),
            max_background_jobs: self.max_background_jobs.or(base.max_background_jobs),
            max_open_files: self.max_open_files.or(base.max_open_files),
            db_write_buffer_size_mb: self.db_write_buffer_size_mb.or(base.db_write_buffer_size_mb),
            max_total_wal_size_mb: self.max_total_wal_size_mb.or(base.max_total_wal_size_mb),
            enable_pipelined_write: self.enable_pipelined_write.or(base.enable_pipelined_write),
            table_cache_num_shard_bits: self
                .table_cache_num_shard_bits
                .or(base.table_cache_num_shard_bits),
            block_cache_size_mb: self.block_cache_size_mb.or(base.block_cache_size_mb),
        }
    }
}

/// Tunable RocksDB configuration for a [`Db`](crate::Db).
///
/// Combines database-wide options, a default per-CF profile applied to
/// every column family, and per-CF overrides keyed by column-family
/// name. See the [module docs](self) for the resolution model.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default, deny_unknown_fields)]
pub struct RocksDbConfig {
    /// Database-wide options.
    pub db: DbWideConfig,

    /// Profile applied to every column family as the baseline.
    pub default_cf: CfTuning,

    /// Per-column-family overrides, layered over [`Self::default_cf`].
    /// Keyed by column-family name.
    pub column_family: BTreeMap<String, CfTuning>,
}

impl RocksDbConfig {
    /// Field-wise overlay producing the effective configuration:
    /// values set on `self` (typically parsed from TOML) win;
    /// otherwise fall back to `base` (typically the code defaults).
    /// The `column_family` maps are unioned by key, with each shared
    /// key's [`CfTuning`] merged recursively.
    pub fn merge_over(&self, base: &Self) -> Self {
        let mut column_family = base.column_family.clone();
        for (name, cf) in &self.column_family {
            let merged = match base.column_family.get(name) {
                Some(b) => cf.merge_over(b),
                None => cf.clone(),
            };
            column_family.insert(name.clone(), merged);
        }
        Self {
            db: self.db.merge_over(&base.db),
            default_cf: self.default_cf.merge_over(&base.default_cf),
            column_family,
        }
    }

    /// Validate the write-stall threshold relationships for the
    /// default profile and for every per-CF override (resolved
    /// against the default profile). Returns the first violation.
    pub fn validate(&self) -> Result<(), OpenError> {
        self.default_cf.write_stall.validate("default-cf")?;
        for (name, cf) in &self.column_family {
            cf.merge_over(&self.default_cf).write_stall.validate(name)?;
        }
        Ok(())
    }
}

/// Resolves per-column-family [`rocksdb::Options`] from a validated
/// [`RocksDbConfig`].
///
/// Built once by [`Db::open`](crate::Db::open) and handed to
/// [`Schema::cfs`](crate::Schema::cfs). Owns the single shared block
/// cache so every CF that opts into a block cache shares one instance
/// rather than allocating its own.
pub struct CfOptionsResolver {
    config: RocksDbConfig,
    block_cache: Option<rocksdb::Cache>,
}

impl CfOptionsResolver {
    /// Validate `config` and build the resolver, allocating the shared
    /// block cache if one is configured.
    pub fn new(config: RocksDbConfig) -> Result<Self, OpenError> {
        config.validate()?;
        let block_cache = config
            .db
            .block_cache_size_mb
            .map(|mb| rocksdb::Cache::new_lru_cache(mib_usize(mb as u64)));
        Ok(Self {
            config,
            block_cache,
        })
    }

    /// Database-wide options for `open_cf_descriptors`. Always sets
    /// `create_if_missing` and `create_missing_column_families`; the
    /// remaining knobs are applied only when configured.
    pub(crate) fn db_options(&self) -> rocksdb::Options {
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let db = &self.config.db;
        if let Some(n) = db.parallelism {
            opts.increase_parallelism(n);
        }
        if let Some(n) = db.max_background_jobs {
            opts.set_max_background_jobs(n);
        }
        if let Some(n) = db.max_open_files {
            opts.set_max_open_files(n);
        }
        if let Some(mb) = db.db_write_buffer_size_mb {
            opts.set_db_write_buffer_size(mib_usize(mb as u64));
        }
        if let Some(mb) = db.max_total_wal_size_mb {
            opts.set_max_total_wal_size(mb << 20);
        }
        if let Some(v) = db.enable_pipelined_write {
            opts.set_enable_pipelined_write(v);
        }
        if let Some(bits) = db.table_cache_num_shard_bits {
            opts.set_table_cache_num_shard_bits(bits);
        }
        opts
    }

    /// Resolved [`rocksdb::Options`] for the column family named `cf`:
    /// the default profile with the per-CF override (if any) layered
    /// on top, plus the shared block cache. Does **not** attach merge
    /// operators or compaction filters — the schema adds those.
    pub fn options(&self, cf: &str) -> rocksdb::Options {
        let tuning = match self.config.column_family.get(cf) {
            Some(o) => o.merge_over(&self.config.default_cf),
            None => self.config.default_cf.clone(),
        };
        tuning.to_options(self.block_cache.as_ref())
    }

    /// Names of the column families with an explicit per-CF override.
    /// Used by [`Db::open`](crate::Db::open) to reject overrides that
    /// name a column family the schema does not declare.
    pub(crate) fn configured_cf_names(&self) -> impl Iterator<Item = &str> {
        self.config.column_family.keys().map(String::as_str)
    }
}

/// Convert a MiB count to bytes as a `usize`, saturating rather than
/// overflowing on absurd inputs.
fn mib_usize(mb: u64) -> usize {
    mb.saturating_mul(1024 * 1024)
        .try_into()
        .unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(c: Option<i32>, sd: Option<i32>, st: Option<i32>) -> WriteStallConfig {
        WriteStallConfig {
            level0_file_num_compaction_trigger: c,
            level0_slowdown_writes_trigger: sd,
            level0_stop_writes_trigger: st,
            ..Default::default()
        }
    }

    #[test]
    fn cf_tuning_merge_prefers_self_then_base() {
        let base = CfTuning {
            write_buffer_size_mb: Some(64),
            compression: Some(Compression::Lz4),
            ..Default::default()
        };
        let over = CfTuning {
            write_buffer_size_mb: Some(256),
            ..Default::default()
        };
        let merged = over.merge_over(&base);
        assert_eq!(merged.write_buffer_size_mb, Some(256));
        assert_eq!(merged.compression, Some(Compression::Lz4));
    }

    #[test]
    fn rocksdb_config_merge_unions_column_families() {
        let mut base = RocksDbConfig::default();
        base.default_cf.write_buffer_size_mb = Some(64);
        base.column_family.insert(
            "objects".to_string(),
            CfTuning {
                write_buffer_size_mb: Some(128),
                ..Default::default()
            },
        );

        let mut over = RocksDbConfig::default();
        over.column_family.insert(
            "objects".to_string(),
            CfTuning {
                bloom_filter_bits: Some(10.0),
                ..Default::default()
            },
        );
        over.column_family.insert(
            "events".to_string(),
            CfTuning {
                write_buffer_size_mb: Some(256),
                ..Default::default()
            },
        );

        let merged = over.merge_over(&base);
        let objects = &merged.column_family["objects"];
        assert_eq!(objects.write_buffer_size_mb, Some(128));
        assert_eq!(objects.bloom_filter_bits, Some(10.0));
        assert_eq!(
            merged.column_family["events"].write_buffer_size_mb,
            Some(256)
        );
        assert_eq!(merged.default_cf.write_buffer_size_mb, Some(64));
    }

    #[test]
    fn validate_rejects_slowdown_above_stop() {
        let cfg = RocksDbConfig {
            default_cf: CfTuning {
                write_stall: ws(Some(4), Some(900), Some(100)),
                ..Default::default()
            },
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("default-cf"), "{err}");
        assert!(err.contains("slowdown"), "{err}");
    }

    #[test]
    fn validate_rejects_compaction_above_slowdown() {
        let cfg = RocksDbConfig {
            default_cf: CfTuning {
                write_stall: ws(Some(20), Some(10), Some(1000)),
                ..Default::default()
            },
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("compaction-trigger"), "{err}");
    }

    #[test]
    fn validate_rejects_negative_trigger() {
        let cfg = RocksDbConfig {
            default_cf: CfTuning {
                write_stall: ws(Some(-1), None, None),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_per_cf_override_uses_effective_values() {
        // default-cf sets slowdown=512; the per-CF override sets only
        // stop=100, so the effective (512, 100) must be rejected.
        let mut cfg = RocksDbConfig {
            default_cf: CfTuning {
                write_stall: ws(Some(4), Some(512), Some(1024)),
                ..Default::default()
            },
            ..Default::default()
        };
        cfg.column_family.insert(
            "bitmap".to_string(),
            CfTuning {
                write_stall: WriteStallConfig {
                    level0_stop_writes_trigger: Some(100),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("bitmap"), "{err}");
    }

    #[test]
    fn validate_rejects_soft_above_hard() {
        let cfg = RocksDbConfig {
            default_cf: CfTuning {
                write_stall: WriteStallConfig {
                    soft_pending_compaction_bytes_limit_mb: Some(2048),
                    hard_pending_compaction_bytes_limit_mb: Some(1024),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn zero_pending_limits_are_valid_disabled() {
        let cfg = RocksDbConfig {
            default_cf: CfTuning {
                write_stall: WriteStallConfig {
                    soft_pending_compaction_bytes_limit_mb: Some(0),
                    hard_pending_compaction_bytes_limit_mb: Some(0),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn resolver_builds_options_without_panicking() {
        let mut cfg = RocksDbConfig::default();
        cfg.db.block_cache_size_mb = Some(64);
        cfg.default_cf = CfTuning {
            write_buffer_size_mb: Some(64),
            compression: Some(Compression::Lz4),
            bottommost_compression: Some(Compression::Zstd),
            block_size_kb: Some(16),
            memtable_prefix_bloom_ratio: Some(0.02),
            target_file_size_mb: Some(128),
            write_stall: ws(Some(4), Some(512), Some(1024)),
            ..Default::default()
        };
        cfg.column_family.insert(
            "digest".to_string(),
            CfTuning {
                bloom_filter_bits: Some(10.0),
                ..Default::default()
            },
        );
        let resolver = CfOptionsResolver::new(cfg).unwrap();
        // Building the options must not panic for either an
        // override-bearing or a default-only CF.
        let _ = resolver.options("digest");
        let _ = resolver.options("anything-else");
        let _ = resolver.db_options();
    }

    #[test]
    fn toml_round_trip_partial() {
        let toml = r#"
            [db]
            parallelism = 8

            [default-cf]
            compression = "lz4"
            bottommost-compression = "zstd"

            [default-cf.write-stall]
            soft-pending-compaction-bytes-limit-mb = 0
            level0-stop-writes-trigger = 1024

            [column-family.transaction_bitmap]
            write-buffer-size-mb = 256
        "#;
        let cfg: RocksDbConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.db.parallelism, Some(8));
        assert_eq!(cfg.default_cf.compression, Some(Compression::Lz4));
        assert_eq!(
            cfg.default_cf
                .write_stall
                .soft_pending_compaction_bytes_limit_mb,
            Some(0)
        );
        assert_eq!(
            cfg.column_family["transaction_bitmap"].write_buffer_size_mb,
            Some(256)
        );
        // Unset fields stay None so they fall back at resolve time.
        assert_eq!(cfg.default_cf.write_buffer_size_mb, None);
    }
}
