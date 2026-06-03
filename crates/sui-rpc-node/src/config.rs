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

use sui_default_config::DefaultConfig;
use sui_indexer_alt_framework::config::ConcurrencyConfig;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_rpc_store::CommitterLayer;
use sui_rpc_store::ConsistencyConfig;

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
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            listen_address: "0.0.0.0:9000".parse().unwrap(),
            config: sui_rpc_api::Config::default(),
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
        cfg
    }
}
