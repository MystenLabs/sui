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
use sui_kv_rpc::LedgerHistoryMethodConfig;
use sui_kv_rpc::ListApiConfig;
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
    /// Per-request evaluated-bucket budget for filtered tx-bitmap scans,
    /// shared across all DNF dimensions. Caps buckets evaluated; observed
    /// BigTable bucket reads may exceed by up to max-bitmap-filter-literals
    /// (one fetched-and-discarded bucket per leaf at exhaustion). Filtered
    /// list_transactions and list_checkpoints requests stop scanning past
    /// this and return a SCAN_LIMIT cursor.
    #[clap(long = "bitmap-bucket-budget-tx", default_value_t = ConcurrencyConfig::default().bitmap_bucket_budget_tx)]
    bitmap_bucket_budget_tx: u64,
    /// Per-request evaluated-bucket budget for filtered event-bitmap scans.
    /// Tuned separately from tx because event-bitmap buckets cover far fewer
    /// source-domain positions. Same fetched-vs-evaluated slop as
    /// bitmap-bucket-budget-tx.
    #[clap(long = "bitmap-bucket-budget-event", default_value_t = ConcurrencyConfig::default().bitmap_bucket_budget_event)]
    bitmap_bucket_budget_event: u64,
}

impl From<ConcurrencyArgs> for ConcurrencyConfig {
    fn from(args: ConcurrencyArgs) -> Self {
        Self {
            request_bigtable_concurrency: args.request_bigtable_concurrency,
            max_bitmap_filter_literals: args.max_bitmap_filter_literals,
            bitmap_bucket_budget_tx: args.bitmap_bucket_budget_tx,
            bitmap_bucket_budget_event: args.bitmap_bucket_budget_event,
        }
    }
}

/// Per-endpoint tunables for the v2alpha list APIs. CLI-driven for now; a config
/// file may back these later.
#[derive(Parser)]
struct ListApiArgs {
    #[clap(long = "list-transactions-timeout-ms", default_value_t = ListApiConfig::default().list_transactions.timeout.as_millis() as u64)]
    list_transactions_timeout_ms: u64,
    #[clap(long = "list-transactions-default-limit", default_value_t = ListApiConfig::default().list_transactions.default_limit_items)]
    list_transactions_default_limit: u32,
    #[clap(long = "list-transactions-max-limit", default_value_t = ListApiConfig::default().list_transactions.max_limit_items)]
    list_transactions_max_limit: u32,
    #[clap(long = "list-transactions-chunk-max", default_value_t = ListApiConfig::default().list_transactions.chunk_max)]
    list_transactions_chunk_max: usize,

    #[clap(long = "list-events-timeout-ms", default_value_t = ListApiConfig::default().list_events.timeout.as_millis() as u64)]
    list_events_timeout_ms: u64,
    #[clap(long = "list-events-default-limit", default_value_t = ListApiConfig::default().list_events.default_limit_items)]
    list_events_default_limit: u32,
    #[clap(long = "list-events-max-limit", default_value_t = ListApiConfig::default().list_events.max_limit_items)]
    list_events_max_limit: u32,
    #[clap(long = "list-events-chunk-max", default_value_t = ListApiConfig::default().list_events.chunk_max)]
    list_events_chunk_max: usize,

    #[clap(long = "list-checkpoints-timeout-ms", default_value_t = ListApiConfig::default().list_checkpoints.timeout.as_millis() as u64)]
    list_checkpoints_timeout_ms: u64,
    #[clap(long = "list-checkpoints-default-limit", default_value_t = ListApiConfig::default().list_checkpoints.default_limit_items)]
    list_checkpoints_default_limit: u32,
    #[clap(long = "list-checkpoints-max-limit", default_value_t = ListApiConfig::default().list_checkpoints.max_limit_items)]
    list_checkpoints_max_limit: u32,
    #[clap(long = "list-checkpoints-chunk-max", default_value_t = ListApiConfig::default().list_checkpoints.chunk_max)]
    list_checkpoints_chunk_max: usize,
}

impl From<ListApiArgs> for ListApiConfig {
    fn from(args: ListApiArgs) -> Self {
        Self {
            list_transactions: LedgerHistoryMethodConfig {
                timeout: Duration::from_millis(args.list_transactions_timeout_ms),
                default_limit_items: args.list_transactions_default_limit,
                max_limit_items: args.list_transactions_max_limit,
                chunk_max: args.list_transactions_chunk_max,
            },
            list_events: LedgerHistoryMethodConfig {
                timeout: Duration::from_millis(args.list_events_timeout_ms),
                default_limit_items: args.list_events_default_limit,
                max_limit_items: args.list_events_max_limit,
                chunk_max: args.list_events_chunk_max,
            },
            list_checkpoints: LedgerHistoryMethodConfig {
                timeout: Duration::from_millis(args.list_checkpoints_timeout_ms),
                default_limit_items: args.list_checkpoints_default_limit,
                max_limit_items: args.list_checkpoints_max_limit,
                chunk_max: args.list_checkpoints_chunk_max,
            },
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
    #[clap(flatten)]
    list_api: ListApiArgs,
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
    let list_api_config: ListApiConfig = app.list_api.into();
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
        list_api_config,
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
