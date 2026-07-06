// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use axum::Router;
use axum::routing::get;
use clap::Parser;
use sui_kv_rpc::KvRpcConfig;
use sui_kv_rpc::KvRpcServer;
use sui_kv_rpc::ServerConfig;
use sui_kvstore::validate_pipeline_name;
use sui_rpc_api::ServerVersion;
use telemetry_subscribers::TelemetryConfig;

bin_version::bin_version!();

#[derive(Parser)]
struct App {
    /// Path to a YAML config file ([`KvRpcConfig`]). New tunables (concurrency,
    /// scan budgets, per-endpoint list limits, experimental query APIs) are
    /// configured here. Run with --config-schema to print the file format.
    #[clap(long)]
    config_path: Option<PathBuf>,

    /// Print the JSON Schema for the --config-path file (with field docs) and
    /// exit.
    #[clap(long)]
    config_schema: bool,

    // The flags below are deprecated. They remain for backwards compatibility:
    // each takes precedence over the config file when set, and logs a
    // deprecation warning. Prefer setting them in the config file.
    /// (deprecated) Path to GCP service account JSON key file. If not provided,
    /// uses Application Default Credentials.
    #[clap(long)]
    credentials: Option<String>,
    /// (deprecated) BigTable instance id.
    instance_id: Option<String>,
    /// (deprecated) gRPC listen address.
    address: Option<String>,
    /// (deprecated) Prometheus metrics host.
    metrics_host: Option<String>,
    /// (deprecated) Prometheus metrics port.
    metrics_port: Option<u16>,
    /// (deprecated) PEM TLS certificate path.
    #[clap(long = "tls-cert")]
    tls_cert: Option<String>,
    /// (deprecated) PEM TLS private key path.
    #[clap(long = "tls-key")]
    tls_key: Option<String>,
    /// (deprecated) GCP project id for the BigTable instance.
    #[clap(long = "bigtable-project")]
    bigtable_project: Option<String>,
    /// (deprecated)
    #[clap(long = "app-profile-id")]
    app_profile_id: Option<String>,
    /// (deprecated) Pipeline watermark to include in GetServiceInfo checkpoint
    /// height. Repeat to include multiple pipelines.
    #[clap(
        long = "watermark-pipeline",
        value_name = "PIPELINE",
        value_delimiter = ',',
        value_parser = validate_pipeline_name
    )]
    watermark_pipeline: Vec<&'static str>,
    /// (deprecated) Channel-level timeout in milliseconds for BigTable gRPC calls.
    #[clap(long = "bigtable-channel-timeout-ms")]
    bigtable_channel_timeout_ms: Option<u64>,
    /// (deprecated) Number of gRPC channels to create at startup.
    #[clap(long = "bigtable-initial-pool-size")]
    bigtable_initial_pool_size: Option<usize>,
    /// (deprecated) Minimum number of channels the pool will maintain.
    #[clap(long = "bigtable-min-pool-size")]
    bigtable_min_pool_size: Option<usize>,
    /// (deprecated) Maximum number of channels the pool can scale to.
    #[clap(long = "bigtable-max-pool-size")]
    bigtable_max_pool_size: Option<usize>,
}

fn warn_deprecated(flag: &str) {
    tracing::warn!(
        "the `{flag}` CLI flag is deprecated; configure it via --config-path instead \
         (run with --config-schema to see the file format; the CLI value takes \
         precedence over the config file for now)"
    );
}

/// Apply a deprecated CLI override on top of the loaded config: when `src` is
/// set it wins over the config file and logs a deprecation warning.
fn override_field<T>(flag: &str, src: Option<T>, dst: &mut Option<T>) {
    if src.is_some() {
        warn_deprecated(flag);
        *dst = src;
    }
}

/// Apply the deprecated CLI flags on top of a loaded config: each set flag wins
/// over the config file and emits a deprecation warning.
fn apply_deprecated_overrides(app: App, config: &mut KvRpcConfig) {
    override_field("--credentials", app.credentials, &mut config.credentials);
    override_field("instance_id", app.instance_id, &mut config.instance_id);
    override_field("address", app.address, &mut config.address);
    override_field("metrics_host", app.metrics_host, &mut config.metrics_host);
    override_field("metrics_port", app.metrics_port, &mut config.metrics_port);
    override_field("--tls-cert", app.tls_cert, &mut config.tls_cert);
    override_field("--tls-key", app.tls_key, &mut config.tls_key);
    override_field(
        "--bigtable-project",
        app.bigtable_project,
        &mut config.bigtable_project,
    );
    override_field(
        "--app-profile-id",
        app.app_profile_id,
        &mut config.app_profile_id,
    );
    override_field(
        "--bigtable-channel-timeout-ms",
        app.bigtable_channel_timeout_ms,
        &mut config.bigtable_channel_timeout_ms,
    );
    override_field(
        "--bigtable-initial-pool-size",
        app.bigtable_initial_pool_size,
        &mut config.bigtable_initial_pool_size,
    );
    override_field(
        "--bigtable-min-pool-size",
        app.bigtable_min_pool_size,
        &mut config.bigtable_min_pool_size,
    );
    override_field(
        "--bigtable-max-pool-size",
        app.bigtable_max_pool_size,
        &mut config.bigtable_max_pool_size,
    );

    if !app.watermark_pipeline.is_empty() {
        warn_deprecated("--watermark-pipeline");
        config.watermark_pipeline = Some(
            app.watermark_pipeline
                .iter()
                .map(|s| s.to_string())
                .collect(),
        );
    }
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
    if app.config_schema {
        println!("{}", KvRpcConfig::schema_json()?);
        return Ok(());
    }
    let mut config = match &app.config_path {
        Some(path) => KvRpcConfig::load(path)?,
        None => KvRpcConfig::default(),
    };
    apply_deprecated_overrides(app, &mut config);
    config.validate()?;

    let instance_id = config.instance_id.clone().context(
        "instance_id must be set via the config file (--config-path) or the \
         deprecated positional argument",
    )?;
    let server_version = Some(ServerVersion::new("sui-kv-rpc", VERSION));
    let registry_service = mysten_metrics::start_prometheus_server(
        format!("{}:{}", config.metrics_host(), config.metrics_port()).parse()?,
    );
    let registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&registry);

    let server = KvRpcServer::new(
        instance_id,
        config.bigtable_project.clone(),
        config.app_profile_id.clone(),
        config.channel_timeout(),
        server_version,
        &registry,
        config.credentials.clone(),
        config.pool_config(),
        config.service_info_watermark_pipelines()?,
        config.ledger_history(),
        config.request_bigtable_concurrency(),
        config.stages(),
    )
    .await?;

    let server_config = ServerConfig {
        tls_identity: config.tls_identity()?,
        metrics_registry: Some(registry),
        enable_reflection: true,
        enable_experimental_query_apis: config.enable_experimental_query_apis(),
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

    let addr = config.address().parse()?;
    server
        .start_service(addr, server_config)
        .await?
        .main()
        .await?;
    Ok(())
}
