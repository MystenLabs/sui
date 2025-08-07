// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::Context;
use prometheus::Registry;
use reqwest::Url;

use sui_futures::service::Service;
use sui_indexer_alt::{config::IndexerConfig as IndexerAltConfig, setup_indexer};
use sui_indexer_alt_framework::{
    IndexerArgs,
    ingestion::{ClientArgs, ingestion_client::IngestionClientArgs},
};
use sui_pg_db::DbArgs;

/// Configuration for the indexer.
pub(crate) struct IndexerConfig {
    database_url: Url,
    db_args: DbArgs,
    pub indexer_args: IndexerArgs,
    pub indexer_config: IndexerAltConfig,
    pub client_args: ClientArgs,
}

impl IndexerConfig {
    /// Create a new IndexerConfig with the given database URL and data ingestion path. All other
    /// indexer configurations are set to their default values.
    pub fn new(database_url: Url, data_ingestion_path: PathBuf) -> Self {
        let client_args = ClientArgs {
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(data_ingestion_path),
                ..Default::default()
            },
            ..Default::default()
        };

        let db_args = DbArgs::default();
        let indexer_args = IndexerArgs::default();
        let indexer_config = IndexerAltConfig::for_test();
        Self {
            database_url,
            db_args,
            indexer_args,
            indexer_config,
            client_args,
        }
    }
}

/// Start the indexer with the given configuration, Prometheus registry, and cancellation token.
pub(crate) async fn start_indexer(
    config: IndexerConfig,
    registry: &Registry,
) -> anyhow::Result<Service> {
    let IndexerConfig {
        database_url,
        db_args,
        indexer_args,
        indexer_config,
        client_args,
    } = config;

    let indexer = setup_indexer(
        database_url,
        db_args,
        indexer_args,
        client_args,
        indexer_config,
        None,
        registry,
    )
    .await?;

    let _pipelines: Vec<_> = indexer.pipelines().collect();
    let service = indexer.run().await.context("Failed to start indexer")?;

    Ok(service)
}
