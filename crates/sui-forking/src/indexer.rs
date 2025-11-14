use anyhow::Context;
use prometheus::Registry;
use reqwest::Url;
use std::path::PathBuf;
use sui_indexer_alt::{config::IndexerConfig as IndexerAltConfig, setup_indexer};
use sui_indexer_alt_framework::{
    IndexerArgs,
    ingestion::{self, ClientArgs},
};
use sui_pg_db::DbArgs;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

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
            local_ingestion_path: Some(data_ingestion_path),
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
    cancel: CancellationToken,
) -> anyhow::Result<()> {
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
        cancel.child_token(),
    )
    .await?;

    let pipelines: Vec<_> = indexer.pipelines().collect();
    tokio::spawn(async move {
        let _ = indexer
            .run()
            .await
            .context("Failed to start indexer")
            .unwrap();
    });

    Ok(())
}
