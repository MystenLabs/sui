// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! TOML-driven runtime configuration for the `sui-rpc-node`
//! binary.
//!
//! The top-level [`ServiceConfig`] is parsed via
//! [`sui_default_config::DefaultConfig`], so every field is
//! optional in the TOML and falls back to a sensible default.
//! Defaults are tuned for a development single-process run; a
//! production deployment overrides them in its TOML.

use std::net::SocketAddr;

use sui_consistent_store::DbOptions;
use sui_consistent_store::RocksDbConfig;
use sui_default_config::DefaultConfig;
use sui_indexer_alt_framework::config::ConcurrencyConfig;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_rpc_store::CommitterLayer;
use sui_rpc_store::ConsistencyConfig;
use sui_rpc_store::PrunerConfig;
use sui_rpc_store::default_rocksdb_config;

/// Top-level configuration for the `sui-rpc-node` service.
///
/// All pipelines are run unconditionally — the binary's whole
/// purpose is to prove out the full rpc-store stack — so there's
/// no per-pipeline enable / disable knob (compare
/// [`sui_rpc_store::PipelineLayer`]).
#[DefaultConfig]
#[derive(Default)]
#[serde(deny_unknown_fields)]
pub struct ServiceConfig {
    /// How checkpoints are pulled from the ingestion endpoint
    /// (concurrency, retry interval, streaming back-off shape).
    pub ingestion: IngestionConfig,

    /// Cross-pipeline consistency knobs: snapshot stride, snapshot
    /// buffer size, and per-pipeline write-buffer depth.
    pub consistency: ConsistencyConfig,

    /// Default committer settings shared by every pipeline.
    pub committer: CommitterLayer,

    /// HTTP RPC server configuration: listen address(es) plus the
    /// `sui-rpc-api` policy knobs (TLS, JSON budgets, ledger-history
    /// tunables, …).
    pub rpc: RpcConfig,

    /// Backing RocksDB tuning. Anything left unset falls back to
    /// [`sui_rpc_store::default_rocksdb_config`].
    pub db: DbConfig,

    /// Restore-driver tuning. Consulted, together with `db`, by the
    /// `restore` subcommand.
    pub restore: RestoreConfig,

    /// Pruning policy for the historical CFs. Absent (the default)
    /// disables pruning; the `run` subcommand starts the background
    /// pruner when it is present. The `serve` and `restore`
    /// subcommands ignore it.
    pub pruner: Option<PrunerConfig>,
}

/// Tuning for the formal-snapshot restore driver.
#[DefaultConfig]
#[serde(deny_unknown_fields)]
pub struct RestoreConfig {
    /// Number of snapshot partitions fetched concurrently during a
    /// restore. Each partition is one `.obj` file decoded and held
    /// in memory while it is committed, so this trades restore
    /// throughput against peak memory. The `--shard-concurrency` CLI
    /// flag overrides it.
    pub shard_concurrency: usize,
}

impl Default for RestoreConfig {
    fn default() -> Self {
        Self {
            shard_concurrency: 8,
        }
    }
}

/// RocksDB-related configuration for the backing database.
///
/// `snapshot_capacity` is declared before `rocksdb` because TOML
/// requires a struct's scalar fields to be serialized ahead of its
/// tables.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", default, deny_unknown_fields)]
pub struct DbConfig {
    /// Number of in-memory snapshots retained for consistent reads.
    /// This is the sole retention knob for the consistent-read
    /// window: combined with `[consistency] stride`, the window spans
    /// roughly `stride * snapshot_capacity` checkpoints. Retention
    /// lives here rather than under `[consistency]` because it is an
    /// open-time property of the database. Defaults to
    /// `DEFAULT_SNAPSHOT_CAPACITY` when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_capacity: Option<usize>,

    /// Tunable RocksDB options, layered over the crate defaults.
    pub rocksdb: RocksDbConfig,
}

/// Default number of retained in-memory snapshots, matching
/// [`sui_consistent_store::DbOptions::default`].
const DEFAULT_SNAPSHOT_CAPACITY: usize = 32;

impl DbConfig {
    /// A fully populated example mirroring the shipped defaults, so
    /// `generate-config` surfaces the real values an operator can edit.
    pub fn example() -> Self {
        Self {
            snapshot_capacity: Some(DEFAULT_SNAPSHOT_CAPACITY),
            rocksdb: default_rocksdb_config(),
        }
    }

    /// Resolve the effective [`DbOptions`] by layering this config
    /// over the crate's tuned defaults.
    pub fn to_db_options(&self) -> DbOptions {
        DbOptions {
            rocksdb: self.rocksdb.merge_over(&default_rocksdb_config()),
            snapshot_capacity: self.snapshot_capacity.unwrap_or(DEFAULT_SNAPSHOT_CAPACITY),
        }
    }
}

/// HTTP / HTTPS RPC server configuration.
///
/// Mirrors what `sui-node` exposes through `NodeConfig`: a plain
/// HTTP listen address paired with the policy-level
/// [`sui_rpc_api::Config`] (which carries TLS, the HTTPS listen
/// address, Move-rendering budgets, ledger-history tunables, etc.).
#[DefaultConfig]
#[serde(deny_unknown_fields)]
pub struct RpcConfig {
    /// Address to accept plain HTTP RPC connections on.
    pub listen_address: SocketAddr,

    /// Policy knobs forwarded into [`sui_rpc_api::RpcService`].
    /// Wrapping `sui_rpc_api::Config` (which is the re-exported
    /// `sui_config::RpcConfig`) keeps a single source of truth
    /// for HTTPS / TLS / JSON-budget defaults so the standalone
    /// rpc-node matches what `sui-node` ships.
    #[serde(flatten)]
    pub config: sui_rpc_api::Config,

    /// Pagination defaults exposed by the v1alpha
    /// [`ConsistentService`] endpoints (`ListBalances`,
    /// `ListOwnedObjects`, `ListObjectsByType`,
    /// `BatchGetBalances`).
    ///
    /// [`ConsistentService`]: sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha
    pub pagination: PaginationConfig,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            listen_address: "0.0.0.0:9000".parse().unwrap(),
            config: sui_rpc_api::Config::default(),
            pagination: PaginationConfig::default(),
        }
    }
}

/// Defaults / clamps for the [`ConsistentService`] paginated
/// endpoints. Mirrors
/// `sui_indexer_alt_consistent_store::rpc::pagination::PaginationConfig`
/// so the standalone rpc-node and the alt-consistent-store agree
/// on what pages a client should expect.
///
/// [`ConsistentService`]: sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha
#[DefaultConfig]
#[derive(Clone)]
#[serde(deny_unknown_fields)]
pub struct PaginationConfig {
    /// Page size returned when a request omits `page_size`.
    pub default_page_size: u32,

    /// Maximum number of `GetBalanceRequest`s allowed in a
    /// single `BatchGetBalances` call. Exceeding this returns
    /// `InvalidArgument`.
    pub max_batch_size: u32,

    /// Upper bound a request's `page_size` is clamped to.
    pub max_page_size: u32,
}

impl Default for PaginationConfig {
    fn default() -> Self {
        Self {
            default_page_size: 50,
            max_batch_size: 200,
            max_page_size: 200,
        }
    }
}

impl ServiceConfig {
    /// Configuration matching [`Self::default`] but with the
    /// committer layer initialised from [`CommitterConfig::default`]
    /// so the example output names every field explicitly.
    /// Suitable for surfacing as a TOML example.
    pub fn example() -> Self {
        Self {
            ingestion: IngestionConfig::default(),
            consistency: ConsistencyConfig::default(),
            committer: CommitterConfig::default().into(),
            rpc: RpcConfig::default(),
            db: DbConfig::example(),
            restore: RestoreConfig::default(),
            pruner: Some(PrunerConfig::default()),
        }
    }

    /// Configuration suitable for tests: tightens the
    /// collect / watermark / retry intervals from their
    /// production defaults (200–500ms) down to 50ms each, and
    /// pins ingestion concurrency at a fixed `1`. The caller
    /// supplies an HTTP listen address (typically from
    /// `get_available_port`) so multiple in-process clusters can
    /// run concurrently without colliding.
    ///
    /// Mirrors `sui_indexer_alt_consistent_store::ServiceConfig::for_test`.
    pub fn for_test(rpc_listen_address: SocketAddr) -> Self {
        let mut cfg = Self::example();
        cfg.ingestion.retry_interval_ms = 10;
        cfg.ingestion.ingest_concurrency = ConcurrencyConfig::Fixed { value: 1 };
        cfg.committer.write_concurrency = Some(1);
        cfg.committer.collect_interval_ms = Some(50);
        cfg.committer.watermark_interval_ms = Some(50);
        cfg.rpc.listen_address = rpc_listen_address;
        // Enable indexing-backed surfaces (v2alpha
        // `ListTransactions` / `ListEvents` / `ListCheckpoints`)
        // so the test harness can exercise them. The rpc-store
        // always materialises the bitmap CFs that back these
        // endpoints, so honouring the flag costs nothing here.
        cfg.rpc.config.enable_indexing = Some(true);
        cfg.rpc.config.ledger_history_indexing = Some(true);
        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_config_serializes_with_db_section() {
        // Also guards the TOML scalar-before-table ordering: the
        // `toml` serializer errors if a table is emitted before a
        // scalar at the same nesting level.
        let rendered = toml::to_string_pretty(&ServiceConfig::example())
            .expect("example config must serialize to TOML");
        assert!(rendered.contains("[db."), "missing db section:\n{rendered}");
        assert!(
            rendered.contains("parallelism"),
            "missing db-wide knobs:\n{rendered}"
        );
    }

    #[test]
    fn empty_db_config_resolves_to_tuned_defaults() {
        let opts = DbConfig::default().to_db_options();
        // Falls back to the crate defaults, not the RocksDB natives.
        assert_eq!(opts.rocksdb.db.parallelism, Some(8));
        assert_eq!(opts.snapshot_capacity, DEFAULT_SNAPSHOT_CAPACITY);
        opts.rocksdb.validate().expect("defaults validate");
    }

    #[test]
    fn partial_db_toml_overrides_defaults() {
        let cfg: ServiceConfig = toml::from_str(
            r#"
            [db]
            snapshot-capacity = 8

            [db.rocksdb.db]
            parallelism = 2
            "#,
        )
        .expect("partial config parses");
        let opts = cfg.db.to_db_options();
        // Overridden values win...
        assert_eq!(opts.rocksdb.db.parallelism, Some(2));
        assert_eq!(opts.snapshot_capacity, 8);
        // ...while unspecified ones fall back to the crate defaults.
        assert_eq!(opts.rocksdb.db.block_cache_size_mb, Some(1024));
    }

    #[test]
    fn restore_shard_concurrency_defaults_to_eight_and_overrides() {
        assert_eq!(RestoreConfig::default().shard_concurrency, 8);
        let cfg: ServiceConfig = toml::from_str(
            r#"
            [restore]
            shard-concurrency = 4
            "#,
        )
        .expect("partial restore config parses");
        assert_eq!(cfg.restore.shard_concurrency, 4);
    }

    #[test]
    fn bad_write_stall_ordering_is_rejected_after_merge() {
        // The default profile sets slowdown = 512; overriding only the
        // stop trigger to 100 yields the inconsistent (512, 100) pair.
        let cfg: ServiceConfig = toml::from_str(
            r#"
            [db.rocksdb.default-cf.write-stall]
            level0-stop-writes-trigger = 100
            "#,
        )
        .expect("config parses");
        assert!(cfg.db.to_db_options().rocksdb.validate().is_err());
    }
}
