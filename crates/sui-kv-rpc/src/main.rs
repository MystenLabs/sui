// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::Result;
use axum::Router;
use axum::routing::get;
use clap::Parser;
use prometheus::Registry;
use sui_kv_rpc::ConcurrencyConfig;
use sui_kv_rpc::KvRpcServer;
use sui_kv_rpc::PoolConfig;
use sui_kv_rpc::ServerConfig;
use sui_kv_rpc::default_service_info_watermark_pipelines;
use sui_kvstore::validate_pipeline_name;
use sui_rpc_api::ServerVersion;
use telemetry_subscribers::TelemetryConfig;
use tonic::transport::Identity;

#[derive(Parser)]
struct PoolArgs {
    /// Number of gRPC channels to create at startup
    #[clap(long = "bigtable-initial-pool-size", default_value_t = PoolConfig::default().initial_pool_size)]
    bigtable_initial_pool_size: usize,
    /// Minimum number of channels the pool will maintain
    #[clap(long = "bigtable-min-pool-size", default_value_t = PoolConfig::default().min_pool_size)]
    bigtable_min_pool_size: usize,
    /// Maximum number of channels the pool can scale to
    #[clap(long = "bigtable-max-pool-size", default_value_t = PoolConfig::default().max_pool_size)]
    bigtable_max_pool_size: usize,
}

impl From<PoolArgs> for PoolConfig {
    fn from(args: PoolArgs) -> Self {
        Self {
            initial_pool_size: args.bigtable_initial_pool_size,
            min_pool_size: args.bigtable_min_pool_size,
            max_pool_size: args.bigtable_max_pool_size,
            ..Self::default()
        }
    }
}

#[derive(Parser)]
struct ConcurrencyArgs {
    /// Per-request cap for active downstream BigTable reads.
    #[clap(
        long = "request-bigtable-concurrency",
        default_value_t = ConcurrencyConfig::default().request_bigtable_concurrency
    )]
    request_bigtable_concurrency: usize,
    /// Maximum total bitmap-literal fanout accepted in one filter request.
    /// Bitmap scans do not consume request-bigtable-concurrency permits.
    #[clap(long = "max-bitmap-filter-literals", default_value_t = ConcurrencyConfig::default().max_bitmap_filter_literals)]
    max_bitmap_filter_literals: usize,
}

impl From<ConcurrencyArgs> for ConcurrencyConfig {
    fn from(args: ConcurrencyArgs) -> Self {
        Self {
            request_bigtable_concurrency: args.request_bigtable_concurrency,
            max_bitmap_filter_literals: args.max_bitmap_filter_literals,
        }
    }
}

bin_version::bin_version!();

#[derive(Parser)]
struct App {
    /// Path to GCP service account JSON key file. If not provided, uses Application Default
    /// Credentials (GOOGLE_APPLICATION_CREDENTIALS or metadata server).
    #[clap(long)]
    credentials: Option<String>,
    instance_id: String,
    #[clap(default_value = "[::1]:8000")]
    address: String,
    #[clap(default_value = "127.0.0.1")]
    metrics_host: String,
    #[clap(default_value_t = 9184)]
    metrics_port: usize,
    #[clap(long = "tls-cert", default_value = "")]
    tls_cert: String,
    #[clap(long = "tls-key", default_value = "")]
    tls_key: String,
    /// GCP project ID for the BigTable instance (defaults to the token provider's project)
    #[clap(long = "bigtable-project")]
    bigtable_project: Option<String>,
    #[clap(long = "app-profile-id")]
    app_profile_id: Option<String>,
    #[clap(long = "checkpoint-bucket")]
    checkpoint_bucket: Option<String>,
    /// Enable v2alpha List APIs. These rely on experimental BigTable query indexes.
    #[clap(long = "enable-experimental-query-apis")]
    enable_experimental_query_apis: bool,
    /// Pipeline watermark to include when reporting GetServiceInfo checkpoint height. Repeat to
    /// include multiple pipelines.
    #[clap(
        long = "watermark-pipeline",
        value_name = "PIPELINE",
        value_delimiter = ',',
        value_parser = validate_pipeline_name
    )]
    watermark_pipeline: Vec<&'static str>,
    /// Channel-level timeout in milliseconds for BigTable gRPC calls (default: 60000)
    #[clap(long = "bigtable-channel-timeout-ms")]
    bigtable_channel_timeout_ms: Option<u64>,
    #[clap(flatten)]
    pool: PoolArgs,
    #[clap(flatten)]
    concurrency: ConcurrencyArgs,
}

async fn health_check() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = TelemetryConfig::new().with_env().init();
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install CryptoProvider");
    let app = App::parse();
    let server_version = Some(ServerVersion::new("sui-kv-rpc", VERSION));
    let registry_service = mysten_metrics::start_prometheus_server(
        format!("{}:{}", app.metrics_host, app.metrics_port).parse()?,
    );
    let registry: Registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&registry);
    let channel_timeout = app.bigtable_channel_timeout_ms.map(Duration::from_millis);
    let pool_config: PoolConfig = app.pool.into();
    let concurrency_config: ConcurrencyConfig = app.concurrency.into();
    let service_info_watermark_pipelines = if app.watermark_pipeline.is_empty() {
        default_service_info_watermark_pipelines(app.enable_experimental_query_apis)
    } else {
        app.watermark_pipeline
    };

    let server = KvRpcServer::new(
        app.instance_id,
        app.bigtable_project,
        app.app_profile_id,
        app.checkpoint_bucket,
        channel_timeout,
        server_version,
        &registry,
        app.credentials,
        pool_config,
        service_info_watermark_pipelines,
        concurrency_config,
    )
    .await?;

    let tls_identity = if !app.tls_cert.is_empty() && !app.tls_key.is_empty() {
        Some(Identity::from_pem(
            std::fs::read(app.tls_cert)?,
            std::fs::read(app.tls_key)?,
        ))
    } else {
        None
    };

    let config = ServerConfig {
        tls_identity,
        metrics_registry: Some(registry),
        enable_reflection: true,
        enable_experimental_query_apis: app.enable_experimental_query_apis,
    };

    tokio::spawn(async {
        let web_server = Router::new().route("/health", get(health_check));
        let listener = tokio::net::TcpListener::bind("0.0.0.0:8081")
            .await
            .expect("can't bind to the healthcheck port");
        axum::serve(listener, web_server.into_make_service())
            .await
            .expect("healh check service failed");
    });

    let addr = app.address.parse()?;
    server.start_service(addr, config).await?.main().await?;
    Ok(())
}
