// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use sui_indexer::backfill::backfill_runner::BackfillRunner;
use sui_indexer::config::{Command, UploadOptions};
use sui_indexer::database::ConnectionPool;
use sui_indexer::db::{
    check_db_migration_consistency, check_prunable_tables_valid, reset_database, run_migrations,
};
use sui_indexer::indexer::Indexer;
use sui_indexer::metrics::{
    spawn_connection_pool_metric_collector, start_prometheus_server, IndexerMetrics,
};
use sui_indexer::restorer::formal_snapshot::IndexerFormalSnapshotRestorer;
use sui_indexer::store::PgIndexerStore;
use tokio_util::sync::CancellationToken;
use tracing::warn;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = sui_indexer::config::IndexerConfig::parse();

    // NOTE: this is to print out tracing like info, warn & error.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();
    warn!("WARNING: Sui indexer is still experimental and we expect occasional breaking changes that require backfills.");

    let (_registry_service, registry) = start_prometheus_server(opts.metrics_address)?;
    mysten_metrics::init_metrics(&registry);
    let indexer_metrics = IndexerMetrics::new(&registry);

    let pool = ConnectionPool::new(
        opts.database_url.clone(),
        opts.connection_pool_config.clone(),
    )
    .await?;
    spawn_connection_pool_metric_collector(indexer_metrics.clone(), pool.clone());

    match opts.command {
        Command::Indexer {
            ingestion_config,
            snapshot_config,
            pruning_options,
            upload_options,
        } => {
            // Make sure to run all migrations on startup, and also serve as a compatibility check.
            run_migrations(pool.dedicated_connection().await?).await?;
            let retention_config = pruning_options.load_from_file();
            if retention_config.is_some() {
                check_prunable_tables_valid(&mut pool.get().await?).await?;
            }

            let store = PgIndexerStore::new(pool, upload_options, indexer_metrics.clone());

            Indexer::start_writer(
                &ingestion_config,
                store,
                indexer_metrics,
                snapshot_config,
                retention_config,
                CancellationToken::new(),
            )
            .await?;
        }
        Command::JsonRpcService(json_rpc_config) => {
            check_db_migration_consistency(&mut pool.get().await?).await?;

            Indexer::start_reader(&json_rpc_config, &registry, pool, CancellationToken::new())
                .await?;
        }
        Command::ResetDatabase { force } => {
            if !force {
                return Err(anyhow::anyhow!(
                    "Resetting the DB requires use of the `--force` flag",
                ));
            }

            reset_database(pool.dedicated_connection().await?).await?;
        }
        Command::RunMigrations => {
            run_migrations(pool.dedicated_connection().await?).await?;
        }
        Command::RunBackFill {
            start,
            end,
            runner_kind,
            backfill_config,
        } => {
            let total_range = start..=end;
            BackfillRunner::run(runner_kind, pool, backfill_config, total_range).await;
        }
        Command::Restore(restore_config) => {
            let store =
                PgIndexerStore::new(pool, UploadOptions::default(), indexer_metrics.clone());
            let mut formal_restorer =
                IndexerFormalSnapshotRestorer::new(store, restore_config).await?;
            formal_restorer.restore().await?;
        }
    }

    Ok(())
}
