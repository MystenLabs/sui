// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::time::Duration;

use std::io::Write;

use anyhow::Result;
use axum::Router;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use clap::Parser;
use flate2::Compression;
use flate2::write::GzEncoder;
// pprof pins prost 0.12; re-exports its `Message` trait. Import from
// pprof so the trait matches the Profile's impl (workspace prost is
// 0.14, a different trait).
use pprof::protos::Message;
use serde::Deserialize;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_indexer_alt_framework::service::Error;
use sui_indexer_alt_metrics::MetricsArgs;
use sui_kvstore::BigTableClient;
use sui_kvstore::BigTableIndexer;
use sui_kvstore::BigTableStore;
use sui_kvstore::IndexerConfig;
use sui_kvstore::set_write_legacy_data;
use sui_protocol_config::Chain;
use telemetry_subscribers::TelemetryConfig;
use tracing::info;
use tracing::warn;

/// Port for the in-process CPU profiler. Held separate from the metrics
/// server so scraping infra can't accidentally trigger long samples.
const PROFILE_PORT: u16 = 6060;
const PROFILE_DEFAULT_SECONDS: u64 = 30;
const PROFILE_MAX_SECONDS: u64 = 120;
const PROFILE_DEFAULT_HZ: i32 = 99;

#[derive(Deserialize)]
struct ProfileQuery {
    seconds: Option<u64>,
    hz: Option<i32>,
}

/// GET /debug/pprof/flamegraph?seconds=N[&hz=N] → SVG flamegraph.
async fn flamegraph_handler(Query(q): Query<ProfileQuery>) -> axum::response::Response {
    let seconds = q
        .seconds
        .unwrap_or(PROFILE_DEFAULT_SECONDS)
        .min(PROFILE_MAX_SECONDS);
    let hz = q.hz.unwrap_or(PROFILE_DEFAULT_HZ).clamp(1, 1000);

    let guard = match pprof::ProfilerGuardBuilder::default()
        .frequency(hz)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()
    {
        Ok(g) => g,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("profiler start failed: {e}"),
            )
                .into_response();
        }
    };

    tokio::time::sleep(Duration::from_secs(seconds)).await;

    let report = match guard.report().build() {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("report build failed: {e}"),
            )
                .into_response();
        }
    };

    let mut buf = Vec::with_capacity(256 * 1024);
    if let Err(e) = report.flamegraph(&mut buf) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("flamegraph encode failed: {e}"),
        )
            .into_response();
    }

    (
        [(
            axum::http::header::CONTENT_TYPE,
            "image/svg+xml; charset=utf-8",
        )],
        buf,
    )
        .into_response()
}

/// GET /debug/pprof/profile?seconds=N[&hz=N] → gzipped pprof protobuf.
/// Consume with `go tool pprof -http=:8000 <file>` or `pprof` / `speedscope`.
/// Uses `prost-codec` — call graph + symbolication are fully preserved
/// (unlike the SVG flamegraph path, whose built-in unwinder loses
/// Rust-async call stacks).
async fn profile_handler(Query(q): Query<ProfileQuery>) -> axum::response::Response {
    let seconds = q
        .seconds
        .unwrap_or(PROFILE_DEFAULT_SECONDS)
        .min(PROFILE_MAX_SECONDS);
    let hz = q.hz.unwrap_or(PROFILE_DEFAULT_HZ).clamp(1, 1000);

    let guard = match pprof::ProfilerGuardBuilder::default()
        .frequency(hz)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()
    {
        Ok(g) => g,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("profiler start failed: {e}"),
            )
                .into_response();
        }
    };

    tokio::time::sleep(Duration::from_secs(seconds)).await;

    let report = match guard.report().build() {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("report build failed: {e}"),
            )
                .into_response();
        }
    };

    let profile = match report.pprof() {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("pprof encode failed: {e}"),
            )
                .into_response();
        }
    };

    let mut pb = Vec::with_capacity(256 * 1024);
    if let Err(e) = profile.encode(&mut pb) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("prost encode failed: {e}"),
        )
            .into_response();
    }

    let mut gz = GzEncoder::new(Vec::with_capacity(pb.len() / 2), Compression::default());
    if let Err(e) = gz.write_all(&pb) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("gzip write failed: {e}"),
        )
            .into_response();
    }
    let body = match gz.finish() {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("gzip finish failed: {e}"),
            )
                .into_response();
        }
    };

    (
        [
            (axum::http::header::CONTENT_TYPE, "application/octet-stream"),
            (axum::http::header::CONTENT_ENCODING, "gzip"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"profile.pb.gz\"",
            ),
        ],
        body,
    )
        .into_response()
}

fn spawn_pprof_server() {
    tokio::spawn(async move {
        let app = Router::new()
            .route("/debug/pprof/flamegraph", get(flamegraph_handler))
            .route("/debug/pprof/profile", get(profile_handler));
        let addr = std::net::SocketAddr::from(([0, 0, 0, 0], PROFILE_PORT));
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                info!(port = PROFILE_PORT, "pprof endpoint listening");
                if let Err(e) = axum::serve(listener, app).await {
                    warn!(%e, "pprof endpoint exited");
                }
            }
            Err(e) => warn!(%e, port = PROFILE_PORT, "failed to bind pprof endpoint"),
        }
    });
}

#[derive(Parser)]
#[command(name = "sui-kvstore-alt")]
#[command(about = "KVStore indexer using sui-indexer-alt-framework")]
struct Args {
    /// Path to TOML config file. If not provided, defaults are used.
    #[arg(long)]
    config: Option<PathBuf>,

    /// BigTable instance ID
    instance_id: String,

    /// GCP project ID for the BigTable instance (defaults to the token provider's project)
    #[arg(long)]
    bigtable_project: Option<String>,

    /// BigTable app profile ID
    #[arg(long)]
    app_profile_id: Option<String>,

    /// Maximum gRPC decoding message size for Bigtable responses, in bytes.
    #[arg(long)]
    bigtable_max_decoding_message_size: Option<usize>,

    /// Chain identifier for resolving protocol configs (mainnet, testnet, or unknown)
    #[arg(long)]
    chain: Chain,

    /// Enable writing legacy data: deprecated combined transaction tx column
    #[arg(long)]
    write_legacy_data: bool,

    #[command(flatten)]
    metrics_args: MetricsArgs,

    #[command(flatten)]
    client_args: ClientArgs,

    #[command(flatten)]
    indexer_args: IndexerArgs,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Install ring as the default rustls crypto provider. Required because hyper-rustls
    // (via gcp_auth) enables aws-lc-rs by default, and we also use ring elsewhere.
    // With both providers compiled in, rustls can't auto-detect which to use.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let _guard = TelemetryConfig::new().with_env().init();

    spawn_pprof_server();

    let args = Args::parse();

    let config: IndexerConfig = if let Some(config_path) = &args.config {
        let config_contents = tokio::fs::read_to_string(config_path).await?;
        toml::from_str(&config_contents)?
    } else {
        IndexerConfig::default()
    };

    let is_bounded = args.indexer_args.last_checkpoint.is_some();
    set_write_legacy_data(args.write_legacy_data);

    info!("Starting sui-kvstore-alt indexer");
    info!(instance_id = %args.instance_id);
    info!("Config: {:#?}", config);

    let channel_timeout = config
        .bigtable_channel_timeout_ms
        .map(Duration::from_millis);

    let pool_config = config
        .bigtable_pool
        .clone()
        .finish(config.bigtable_connection_pool_size);

    let registry = prometheus::Registry::new();
    let metrics_service =
        sui_indexer_alt_metrics::MetricsService::new(args.metrics_args, registry.clone());

    let client = BigTableClient::new_remote(
        args.instance_id,
        args.bigtable_project,
        false,
        channel_timeout,
        args.bigtable_max_decoding_message_size,
        "sui-kvstore-alt".to_string(),
        Some(&registry),
        args.app_profile_id,
        pool_config,
    )
    .await?;

    let store = BigTableStore::new(client);

    let indexer_config = config.clone();
    let committer = config.committer.finish(CommitterConfig::default());
    let bigtable_indexer = BigTableIndexer::new(
        store,
        args.indexer_args,
        args.client_args,
        config.ingestion,
        committer,
        indexer_config,
        config.pipeline,
        args.chain,
        &registry,
    )
    .await?;

    let metrics_handle = metrics_service.run().await?;
    let service = bigtable_indexer.indexer.run().await?;

    match service.attach(metrics_handle).main().await {
        Ok(()) => {}
        Err(Error::Terminated) => {
            if is_bounded {
                std::process::exit(1);
            }
        }
        Err(Error::Aborted) => {
            std::process::exit(1);
        }
        Err(Error::Task(_)) => {
            std::process::exit(2);
        }
    }

    Ok(())
}
