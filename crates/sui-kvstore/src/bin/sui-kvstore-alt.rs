// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::service::Error;
use sui_indexer_alt_framework::service::Service;
use sui_indexer_alt_metrics::MetricsArgs;
use sui_kvstore::ALPHA_PIPELINE_NAMES;
use sui_kvstore::BigTableClient;
use sui_kvstore::BigTableIndexer;
use sui_kvstore::IndexerConfig;
use sui_kvstore::default_committer_config;
use sui_kvstore::readiness::ReadinessArgs;
use sui_kvstore::readiness::wait_until_caught_up;
use sui_kvstore::rpc::RpcArgs;
use sui_kvstore::rpc::RpcService;
use sui_kvstore::rpc::proto::FILE_DESCRIPTOR_SET;
use sui_kvstore::rpc::proto::v1alpha::watermark_service_server::WatermarkServiceServer;
use sui_kvstore::rpc::watermark_service::WatermarkService;
use sui_kvstore::set_write_legacy_data;
use sui_kvstore::streamed_watermarks;
use sui_kvstore::validate_pipeline_name;
use sui_protocol_config::Chain;
use telemetry_subscribers::TelemetryConfig;
use tracing::info;
use tracing::warn;

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

    /// Register an alpha pipeline, which is otherwise never registered (not even when selected
    /// with `--pipeline`). Repeatable. Names of pipelines that are always registered are
    /// accepted and ignored.
    #[arg(
        long = "enable-alpha-pipeline",
        value_name = "PIPELINE_NAME",
        value_parser = validate_pipeline_name
    )]
    enable_alpha_pipelines: Vec<&'static str>,

    #[command(flatten)]
    metrics_args: MetricsArgs,

    #[command(flatten)]
    rpc_args: RpcArgs,

    #[command(flatten)]
    readiness_args: ReadinessArgs,

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
    let (alpha_pipelines, always_registered_pipelines): (Vec<_>, Vec<_>) = args
        .enable_alpha_pipelines
        .into_iter()
        .partition(|name| ALPHA_PIPELINE_NAMES.contains(name));
    if !always_registered_pipelines.is_empty() {
        warn!(
            pipelines = ?always_registered_pipelines,
            "--enable-alpha-pipeline ignored for pipelines that are always registered",
        );
    }
    info!(?alpha_pipelines, "Enabled alpha pipelines");

    let channel_timeout = config
        .bigtable_channel_timeout_ms
        .map(Duration::from_millis);

    let pool_config = config
        .bigtable_pool
        .clone()
        .finish(config.bigtable_connection_pool_size);
    pool_config.validate()?;
    let read_rows_retry = config.read_rows_retry();
    read_rows_retry.validate(channel_timeout)?;

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
        config.batch_write_flow_control,
    )
    .await?
    .with_read_rows_retry(read_rows_retry);

    // The gRPC server reads the stored watermarks at startup with its own client handle.
    let rpc_setup = args
        .rpc_args
        .rpc_listen_address
        .map(|rpc_listen_address| (rpc_listen_address, client.clone()));
    let indexer_config = config.clone();
    let committer = config.committer.finish(default_committer_config());
    let bigtable_indexer = BigTableIndexer::new(
        client,
        args.indexer_args,
        args.client_args,
        config.ingestion,
        committer,
        indexer_config,
        config.pipeline,
        &alpha_pipelines,
        args.chain,
        &registry,
    )
    .await?;

    let rpc_handle = match rpc_setup {
        Some((rpc_listen_address, stored_watermark_client)) => Some(
            start_rpc(
                rpc_listen_address,
                args.readiness_args,
                &bigtable_indexer,
                stored_watermark_client,
            )
            .await?,
        ),
        None => None,
    };

    let metrics_handle = metrics_service.run().await?;
    let mut service = bigtable_indexer.run().await?.attach(metrics_handle);
    if let Some(rpc_handle) = rpc_handle {
        service = service.attach(rpc_handle);
    }

    match service.main().await {
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

/// Starts the kvstore gRPC server on `rpc_listen_address`. Its health checks report ready once
/// every pipeline has caught up to the stored watermarks read here, at startup, and to the peers
/// in `readiness_args`. Its watermark stream starts from the same stored watermarks.
async fn start_rpc(
    rpc_listen_address: SocketAddr,
    readiness_args: ReadinessArgs,
    bigtable_indexer: &BigTableIndexer,
    mut stored_watermark_client: BigTableClient,
) -> Result<Service> {
    let pipeline_tasks = bigtable_indexer.pipeline_tasks().to_vec();
    let watermark_keys: Vec<&str> = pipeline_tasks.iter().map(String::as_str).collect();
    let startup_watermarks = stored_watermark_client
        .get_pipeline_watermarks(&watermark_keys)
        .await
        .context("Failed to read stored pipeline watermarks")?;
    let (peer_watermarks, peer_watermarks_handle) =
        streamed_watermarks::follow(&readiness_args.peer_urls)?;

    let durable_progress = bigtable_indexer.durable_progress();
    let rpc_service = RpcService::new(rpc_listen_address);
    let (watermark_service, watermark_relay) = WatermarkService::new(
        &startup_watermarks,
        durable_progress.subscribe(),
        rpc_service.shutdown_token(),
    );
    let caught_up = wait_until_caught_up(
        pipeline_tasks,
        durable_progress.subscribe(),
        startup_watermarks,
        peer_watermarks,
        readiness_args.readiness_max_lag_ms,
    );

    let rpc_handle = rpc_service
        .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
        .add_service(WatermarkServiceServer::new(watermark_service))
        .run(caught_up)
        .await?;

    Ok(rpc_handle
        .attach(watermark_relay)
        .attach(peer_watermarks_handle))
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;

    use super::*;

    fn parse(extra: &[&str]) -> Result<Args, clap::Error> {
        let required = [
            "sui-kvstore-alt",
            "instance",
            "--chain",
            "mainnet",
            "--local-ingestion-path",
            "/tmp",
        ];
        Args::try_parse_from(required.iter().chain(extra))
    }

    #[test]
    fn grpc_server_is_off_by_default() {
        let args = parse(&[]).unwrap();
        assert_eq!(args.rpc_args.rpc_listen_address, None);
    }

    #[test]
    fn readiness_flags_require_grpc_server() {
        let server = ["--rpc-listen-address", "0.0.0.0:7001"];
        for flag in [
            ["--peer-url", "http://peer:7001"],
            ["--readiness-max-lag-ms", "1000"],
        ] {
            let error = parse(&flag).err().expect("accepted without the server");
            assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument, "{flag:?}");
            parse(&[flag, server].concat()).unwrap();
        }
    }
}
