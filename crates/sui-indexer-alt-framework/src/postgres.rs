// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{
    migration::{self, Migration, MigrationSource},
    pg::Pg,
};
use diesel_migrations::EmbeddedMigrations;
use prometheus::Registry;
use sui_indexer_alt_framework_store_pg;
use sui_pg_db::temp::TempDb;
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;

use crate::{
    ingestion::{ClientArgs, IngestionConfig},
    Indexer, IndexerArgs,
};

pub use sui_field_count::FieldCount;
pub use sui_indexer_alt_framework_store_pg::pg_store::PgStore;
pub use sui_indexer_alt_framework_store_pg::schema;
pub use sui_pg_db::*;

const MIGRATIONS: EmbeddedMigrations = sui_indexer_alt_framework_store_pg::MIGRATIONS;

#[async_trait::async_trait]
pub trait IndexerPostgresExt {
    async fn new_for_testing(migrations: &'static EmbeddedMigrations)
        -> (Indexer<PgStore>, TempDb);

    fn migrations(
        migrations: Option<&'static EmbeddedMigrations>,
    ) -> impl MigrationSource<Pg> + Send + Sync + 'static;
}

#[async_trait::async_trait]
impl IndexerPostgresExt for Indexer<PgStore> {
    async fn new_for_testing(
        migrations: &'static EmbeddedMigrations,
    ) -> (Indexer<PgStore>, TempDb) {
        let temp_db = TempDb::new().unwrap();
        let store = PgStore(
            Db::for_write(temp_db.database().url().clone(), DbArgs::default())
                .await
                .unwrap(),
        );
        store
            .0
            .run_migrations(Indexer::<PgStore>::migrations(Some(migrations)))
            .await
            .unwrap();

        let indexer = Indexer::new(
            store,
            IndexerArgs::default(),
            ClientArgs {
                remote_store_url: None,
                local_ingestion_path: Some(tempdir().unwrap().into_path()),
                rpc_api_url: None,
                rpc_username: None,
                rpc_password: None,
            },
            IngestionConfig::default(),
            &Registry::new(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        (indexer, temp_db)
    }

    fn migrations(
        migrations: Option<&'static EmbeddedMigrations>,
    ) -> impl MigrationSource<Pg> + Send + Sync + 'static {
        struct Migrations(Option<&'static EmbeddedMigrations>);
        impl MigrationSource<Pg> for Migrations {
            fn migrations(&self) -> migration::Result<Vec<Box<dyn Migration<Pg>>>> {
                let mut migrations = MIGRATIONS.migrations()?;
                if let Some(more_migrations) = self.0 {
                    migrations.extend(more_migrations.migrations()?);
                }
                Ok(migrations)
            }
        }

        Migrations(migrations)
    }
}

#[cfg(test)]
pub mod tests {
    use crate::{pipeline::Processor, ConcurrentConfig};
    use async_trait::async_trait;
    use std::sync::Arc;
    use sui_field_count::FieldCount;
    use sui_indexer_alt_framework_store_traits::{CommitterWatermark, DbConnection, Store};
    use sui_types::full_checkpoint_content::CheckpointData;

    use super::*;
    use crate::pipeline::concurrent;

    #[derive(FieldCount)]
    struct V {
        _v: u64,
    }

    macro_rules! define_test_concurrent_pipeline {
        ($name:ident) => {
            define_test_concurrent_pipeline!($name, false);
        };
        ($name:ident, $pruning_requires_processed_values:expr) => {
            struct $name;
            impl Processor for $name {
                const NAME: &'static str = stringify!($name);
                type Value = V;
                fn process(
                    &self,
                    _checkpoint: &Arc<CheckpointData>,
                ) -> anyhow::Result<Vec<Self::Value>> {
                    todo!()
                }
            }

            #[async_trait]
            impl concurrent::Handler for $name {
                type Store = PgStore;

                const PRUNING_REQUIRES_PROCESSED_VALUES: bool = $pruning_requires_processed_values;
                async fn commit<'a>(
                    _values: &[Self::Value],
                    _conn: &mut <Self::Store as Store>::Connection<'a>,
                ) -> anyhow::Result<usize> {
                    todo!()
                }
            }
        };
    }

    define_test_concurrent_pipeline!(ConcurrentPipeline1);
    define_test_concurrent_pipeline!(ConcurrentPipeline2);
    define_test_concurrent_pipeline!(ConcurrentPipeline3, true);

    #[tokio::test]
    async fn test_add_new_pipeline() {
        let (mut indexer, _temp_db) = Indexer::<PgStore>::new_for_testing(&MIGRATIONS).await;
        indexer
            .concurrent_pipeline(ConcurrentPipeline1, ConcurrentConfig::default())
            .await
            .unwrap();
        assert_eq!(indexer.first_checkpoint_from_watermark, 0);
    }

    #[tokio::test]
    async fn test_add_existing_pipeline() {
        let (mut indexer, _temp_db) = Indexer::<PgStore>::new_for_testing(&MIGRATIONS).await;
        {
            let watermark = CommitterWatermark::new_for_testing(10);
            let mut conn = indexer.store().connect().await.unwrap();
            assert!(conn
                .set_committer_watermark(ConcurrentPipeline1::NAME, watermark)
                .await
                .unwrap());
        }
        indexer
            .concurrent_pipeline(ConcurrentPipeline1, ConcurrentConfig::default())
            .await
            .unwrap();
        assert_eq!(indexer.first_checkpoint_from_watermark, 11);
    }

    #[tokio::test]
    async fn test_add_multiple_pipelines() {
        let (mut indexer, _temp_db) = Indexer::<PgStore>::new_for_testing(&MIGRATIONS).await;
        {
            let watermark1 = CommitterWatermark::new_for_testing(10);
            let mut conn = indexer.store().connect().await.unwrap();
            assert!(conn
                .set_committer_watermark(ConcurrentPipeline1::NAME, watermark1)
                .await
                .unwrap());
            let watermark2 = CommitterWatermark::new_for_testing(20);
            assert!(conn
                .set_committer_watermark(ConcurrentPipeline2::NAME, watermark2)
                .await
                .unwrap());
        }

        indexer
            .concurrent_pipeline(ConcurrentPipeline2, ConcurrentConfig::default())
            .await
            .unwrap();
        assert_eq!(indexer.first_checkpoint_from_watermark, 21);
        indexer
            .concurrent_pipeline(ConcurrentPipeline1, ConcurrentConfig::default())
            .await
            .unwrap();
        assert_eq!(indexer.first_checkpoint_from_watermark, 11);
    }

    #[tokio::test]
    async fn test_add_multiple_pipelines_pruning_requires_processed_values() {
        let (mut indexer, _temp_db) = Indexer::<PgStore>::new_for_testing(&MIGRATIONS).await;
        {
            let watermark1 = CommitterWatermark::new_for_testing(10);
            let mut conn = indexer.store().connect().await.unwrap();
            assert!(conn
                .set_committer_watermark(ConcurrentPipeline1::NAME, watermark1)
                .await
                .unwrap());
        }
        indexer
            .concurrent_pipeline(ConcurrentPipeline1, ConcurrentConfig::default())
            .await
            .unwrap();
        assert_eq!(indexer.first_checkpoint_from_watermark, 11);

        {
            let watermark3 = CommitterWatermark::new_for_testing(20);
            let mut conn = indexer.store().connect().await.unwrap();
            assert!(conn
                .set_committer_watermark(ConcurrentPipeline3::NAME, watermark3)
                .await
                .unwrap());
            assert!(conn
                .set_pruner_watermark(ConcurrentPipeline3::NAME, 5)
                .await
                .unwrap());
        }
        indexer
            .concurrent_pipeline(ConcurrentPipeline3, ConcurrentConfig::default())
            .await
            .unwrap();

        assert_eq!(indexer.first_checkpoint_from_watermark, 5);
    }
}
