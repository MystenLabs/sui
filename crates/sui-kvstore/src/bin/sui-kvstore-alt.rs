// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
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

#[derive(Parser)]
#[command(name = "sui-kvstore-alt")]
#[command(about = "KVStore indexer using sui-indexer-alt-framework")]
struct Args {
    /// Path to TOML config file
    #[arg(long)]
    config: PathBuf,

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

    /// Enable writing legacy data: watermark \[0\] row, epoch DEFAULT_COLUMN, and transaction tx column
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

    let args = Args::parse();

    let config_contents = tokio::fs::read_to_string(&args.config).await?;
    let config: IndexerConfig = toml::from_str(&config_contents)?;

    let is_bounded = args.indexer_args.last_checkpoint.is_some();
    set_write_legacy_data(args.write_legacy_data);

    info!("Starting sui-kvstore-alt indexer");
    info!(instance_id = %args.instance_id);
    info!("Config: {:#?}", config);

    let client = BigTableClient::new_remote(
        args.instance_id,
        args.bigtable_project,
        false,
        None,
        args.bigtable_max_decoding_message_size,
        "sui-kvstore-alt".to_string(),
        None,
        args.app_profile_id,
        config.bigtable_connection_pool_size,
    )
    .await?;

    let store = BigTableStore::new(client);

    let registry = prometheus::Registry::new_custom(Some("kvstore_alt".into()), None)?;
    let metrics_service =
        sui_indexer_alt_metrics::MetricsService::new(args.metrics_args, registry.clone());

    let committer = config.committer.finish(CommitterConfig::default());
    let bigtable_indexer = BigTableIndexer::new(
        store,
        args.indexer_args,
        args.client_args,
        config.ingestion,
        committer,
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
