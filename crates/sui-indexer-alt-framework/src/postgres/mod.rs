// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use diesel_migrations::EmbeddedMigrations;
use prometheus::Registry;
use sui_indexer_alt_metrics::db::DbConnectionStatsCollector;
use sui_pg_db::temp::TempDb;
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{
    Indexer, IndexerArgs,
    ingestion::{ClientArgs, IngestionConfig, ingestion_client::IngestionClientArgs},
};

pub use sui_pg_db::*;

pub mod handler;

/// An opinionated indexer implementation that uses a Postgres database as the store.
impl Indexer<Db> {
    /// Create a new instance of the indexer framework. `database_url`, `db_args`, `indexer_args,`,
    /// `client_args`, and `ingestion_config` contain configurations for the following,
    /// respectively:
    ///
    /// - Connecting to the database,
    /// - What is indexed (which checkpoints, which pipelines, whether to update the watermarks
    ///   table) and where to serve metrics from,
    /// - Where to download checkpoints from,
    /// - Concurrency and buffering parameters for downloading checkpoints.
    ///
    /// Optional `migrations` contains the SQL to run in order to bring the database schema up-to-date for
    /// the specific instance of the indexer, generated using diesel's `embed_migrations!` macro.
    /// These migrations will be run as part of initializing the indexer if provided.
    ///
    /// After initialization, at least one pipeline must be added using [Self::concurrent_pipeline]
    /// or [Self::sequential_pipeline], before the indexer is started using [Self::run].
    pub async fn new_from_pg(
        database_url: Url,
        db_args: DbArgs,
        indexer_args: IndexerArgs,
        client_args: ClientArgs,
        ingestion_config: IngestionConfig,
        migrations: Option<&'static EmbeddedMigrations>,
        metrics_prefix: Option<&str>,
        registry: &Registry,
        cancel: CancellationToken,
    ) -> Result<Self> {
        let store = Db::for_write(database_url, db_args) // I guess our store needs a constructor fn
            .await
            .context("Failed to connect to database")?;

        // At indexer initialization, we ensure that the DB schema is up-to-date.
        store
            .run_migrations(migrations)
            .await
            .context("Failed to run pending migrations")?;

        registry.register(Box::new(DbConnectionStatsCollector::new(
            Some("indexer_db"),
            store.clone(),
        )))?;

        Indexer::new(
            store,
            indexer_args,
            client_args,
            ingestion_config,
            metrics_prefix,
            registry,
            cancel,
        )
        .await
    }

    /// Create a new temporary database and runs provided migrations in tandem with the migrations
    /// necessary to support watermark operations on the indexer. The indexer is then instantiated
    /// and returned along with the temporary database.
    pub async fn new_for_testing(migrations: &'static EmbeddedMigrations) -> (Indexer<Db>, TempDb) {
        let temp_db = TempDb::new().unwrap();
        let store = Db::for_write(temp_db.database().url().clone(), DbArgs::default())
            .await
            .unwrap();
        store.run_migrations(Some(migrations)).await.unwrap();

        let indexer = Indexer::new(
            store,
            IndexerArgs::default(),
            ClientArgs {
                ingestion: IngestionClientArgs {
                    local_ingestion_path: Some(tempdir().unwrap().keep()),
                    ..Default::default()
                },
                ..Default::default()
            },
            IngestionConfig::default(),
            None,
            &Registry::new(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        (indexer, temp_db)
    }
}

#[cfg(test)]
pub mod tests {

    use async_trait::async_trait;
    use std::sync::Arc;
    use sui_indexer_alt_framework_store_traits::{CommitterWatermark, Connection as _};
    use sui_types::full_checkpoint_content::Checkpoint;

    use super::*;

    use crate::{ConcurrentConfig, pipeline::Processor};

    #[derive(FieldCount)]
    struct V {
        _v: u64,
    }

    macro_rules! define_test_concurrent_pipeline {
        ($name:ident) => {
            struct $name;
            #[async_trait]
            impl Processor for $name {
                const NAME: &'static str = stringify!($name);
                type Value = V;
                async fn process(
                    &self,
                    _checkpoint: &Arc<Checkpoint>,
                ) -> anyhow::Result<Vec<Self::Value>> {
                    todo!()
                }
            }

            #[async_trait]
            impl handler::Handler for $name {
                async fn commit<'a>(
                    _values: &[Self::Value],
                    _conn: &mut Connection<'a>,
                ) -> anyhow::Result<usize> {
                    todo!()
                }
            }
        };
    }

    define_test_concurrent_pipeline!(ConcurrentPipeline1);
    define_test_concurrent_pipeline!(ConcurrentPipeline2);

    #[tokio::test]
    async fn test_add_new_pipeline() {
        let (mut indexer, _temp_db) = Indexer::new_for_testing(&MIGRATIONS).await;
        indexer
            .concurrent_pipeline(ConcurrentPipeline1, ConcurrentConfig::default())
            .await
            .unwrap();
        assert_eq!(indexer.first_ingestion_checkpoint, 0);
    }

    #[tokio::test]
    async fn test_add_existing_pipeline() {
        let (mut indexer, _temp_db) = Indexer::new_for_testing(&MIGRATIONS).await;
        {
            let watermark = CommitterWatermark::new_for_testing(10);
            let mut conn = indexer.store().connect().await.unwrap();
            assert!(
                conn.set_committer_watermark(ConcurrentPipeline1::NAME, watermark)
                    .await
                    .unwrap()
            );
        }
        indexer
            .concurrent_pipeline(ConcurrentPipeline1, ConcurrentConfig::default())
            .await
            .unwrap();
        assert_eq!(indexer.first_ingestion_checkpoint, 11);
    }

    #[tokio::test]
    async fn test_add_multiple_pipelines() {
        let (mut indexer, _temp_db) = Indexer::new_for_testing(&MIGRATIONS).await;
        {
            let watermark1 = CommitterWatermark::new_for_testing(10);
            let mut conn = indexer.store().connect().await.unwrap();
            assert!(
                conn.set_committer_watermark(ConcurrentPipeline1::NAME, watermark1)
                    .await
                    .unwrap()
            );
            let watermark2 = CommitterWatermark::new_for_testing(20);
            assert!(
                conn.set_committer_watermark(ConcurrentPipeline2::NAME, watermark2)
                    .await
                    .unwrap()
            );
        }

        indexer
            .concurrent_pipeline(ConcurrentPipeline2, ConcurrentConfig::default())
            .await
            .unwrap();
        assert_eq!(indexer.first_ingestion_checkpoint, 21);
        indexer
            .concurrent_pipeline(ConcurrentPipeline1, ConcurrentConfig::default())
            .await
            .unwrap();
        assert_eq!(indexer.first_ingestion_checkpoint, 11);
    }
}
