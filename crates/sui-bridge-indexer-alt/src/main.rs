// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::Context;
use clap::Parser;
use prometheus::Registry;
use std::net::SocketAddr;
use sui_bridge_indexer_alt::handlers::token_transfer_data_handler::TokenTransferDataHandler;
use sui_bridge_indexer_alt::handlers::token_transfer_handler::TokenTransferHandler;
use sui_bridge_schema::MIGRATIONS;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::postgres::DbArgs;
use sui_indexer_alt_framework::{Indexer, IndexerArgs};
use sui_indexer_alt_metrics::{MetricsArgs, MetricsService};
use tokio_util::sync::CancellationToken;
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

    let cancel = CancellationToken::new();
    let registry = Registry::new_custom(Some("bridge".into()), None)
        .context("Failed to create Prometheus registry.")?;
    let metrics = MetricsService::new(
        MetricsArgs { metrics_address },
        registry,
        cancel.child_token(),
    );

    let mut indexer = Indexer::new_from_pg(
        database_url,
        db_args,
        indexer_args,
        ClientArgs {
            remote_store_url: Some(remote_store_url),
            local_ingestion_path: None,
            rpc_api_url: None,
            rpc_username: None,
            rpc_password: None,
        },
        Default::default(),
        Some(&MIGRATIONS),
        metrics.registry(),
        cancel.clone(),
    )
    .await?;

    indexer
        .concurrent_pipeline(TokenTransferHandler::default(), Default::default())
        .await?;

    indexer
        .concurrent_pipeline(TokenTransferDataHandler::default(), Default::default())
        .await?;

    let h_indexer = indexer.run().await?;
    let h_metrics = metrics.run().await?;

    let _ = h_indexer.await;
    cancel.cancel();
    let _ = h_metrics.await;
    Ok(())
}
