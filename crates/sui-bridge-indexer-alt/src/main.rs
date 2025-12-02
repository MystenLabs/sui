// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::Context;
use clap::Parser;
use prometheus::Registry;
use std::net::SocketAddr;
use sui_bridge_indexer_alt::handlers::error_handler::ErrorTransactionHandler;
use sui_bridge_indexer_alt::handlers::governance_action_handler::GovernanceActionHandler;
use sui_bridge_indexer_alt::handlers::token_transfer_data_handler::TokenTransferDataHandler;
use sui_bridge_indexer_alt::handlers::token_transfer_handler::TokenTransferHandler;
use sui_bridge_indexer_alt::metrics::BridgeIndexerMetrics;
use sui_bridge_schema::MIGRATIONS;
use sui_indexer_alt_framework::ingestion::{ClientArgs, ingestion_client::IngestionClientArgs};
use sui_indexer_alt_framework::postgres::DbArgs;
use sui_indexer_alt_framework::service::Error;
use sui_indexer_alt_framework::{Indexer, IndexerArgs};
use sui_indexer_alt_metrics::{MetricsArgs, MetricsService};
use url::Url;

#[derive(Parser)]
#[clap(rename_all = "kebab-case", author, version)]
struct Args {
    #[command(flatten)]
    db_args: DbArgs,
    #[command(flatten)]
    indexer_args: IndexerArgs,
    #[clap(env, long, default_value = "0.0.0.0:9184")]
    metrics_address: SocketAddr,
    #[clap(
        env,
        long,
        default_value = "postgres://postgres:postgrespw@localhost:5432/bridge"
    )]
    database_url: Url,
    #[clap(env, long, default_value = "https://checkpoints.mainnet.sui.io")]
    remote_store_url: Url,
}
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let Args {
        db_args,
        indexer_args,
        metrics_address,
        database_url,
        remote_store_url,
    } = Args::parse();

    let is_bounded_job = indexer_args.last_checkpoint.is_some();
    let registry = Registry::new_custom(Some("bridge".into()), None)
        .context("Failed to create Prometheus registry.")?;

    // Initialize bridge-specific metrics
    let bridge_metrics = BridgeIndexerMetrics::new(&registry);

    let metrics = MetricsService::new(MetricsArgs { metrics_address }, registry);

    let metrics_prefix = None;
    let mut indexer = Indexer::new_from_pg(
        database_url,
        db_args,
        indexer_args,
        ClientArgs {
            ingestion: IngestionClientArgs {
                remote_store_url: Some(remote_store_url),
                ..Default::default()
            },
            ..Default::default()
        },
        Default::default(),
        Some(&MIGRATIONS),
        metrics_prefix,
        metrics.registry(),
    )
    .await?;

    indexer
        .concurrent_pipeline(
            TokenTransferHandler::new(bridge_metrics.clone()),
            Default::default(),
        )
        .await?;

    indexer
        .concurrent_pipeline(TokenTransferDataHandler::default(), Default::default())
        .await?;

    indexer
        .concurrent_pipeline(
            GovernanceActionHandler::new(bridge_metrics.clone()),
            Default::default(),
        )
        .await?;

    indexer
        .concurrent_pipeline(ErrorTransactionHandler, Default::default())
        .await?;

    let s_indexer = indexer.run().await?;
    let s_metrics = metrics.run().await?;

    match s_indexer.attach(s_metrics).main().await {
        Ok(()) => Ok(()),
        Err(Error::Terminated) => {
            if is_bounded_job {
                std::process::exit(1);
            } else {
                Ok(())
            }
        }
        Err(Error::Aborted) => {
            std::process::exit(1);
        }
        Err(Error::Task(_)) => {
            std::process::exit(2);
        }
    }
}
