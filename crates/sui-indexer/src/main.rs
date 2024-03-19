// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use tracing::{error, info};
use sui_indexer::base_db::{get_pg_pool_connection, new_pg_connection_pool};
use sui_indexer::db::new_connection_pool;

use sui_indexer::errors::IndexerError;
use sui_indexer::indexer::Indexer;
use sui_indexer::metrics::start_prometheus_server;
use sui_indexer::metrics::IndexerMetrics;
use sui_indexer::store::PgIndexerAnalyticalStore;
use sui_indexer::store::PgIndexerStore;
use sui_indexer::IndexerConfig;

#[tokio::main]
async fn main() -> Result<(), IndexerError> {
    // NOTE: this is to print out tracing like info, warn & error.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let indexer_config = IndexerConfig::parse();
    info!("Parsed indexer config: {:#?}", indexer_config);

    #[cfg(feature = "postgres-feature")]
    setup_postgres::setup(indexer_config).await
}


#[cfg(feature = "postgres-feature")]
mod setup_postgres {
    use anyhow::anyhow;
    use diesel::backend::DieselReserveSpecialization;
    use diesel::pg::Pg;
    use diesel::{PgConnection, RunQueryDsl};
    use diesel::migration::MigrationSource;
    use diesel::r2d2::R2D2Connection;
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
    use tracing::{error, info};
    use sui_indexer::db::{get_pool_connection, new_connection_pool, PooledConnection};
    use sui_indexer::errors::IndexerError;
    use sui_indexer::indexer::Indexer;
    use sui_indexer::IndexerConfig;
    use sui_indexer::metrics::{IndexerMetrics, start_prometheus_server};
    use sui_indexer::store::{PgIndexerAnalyticalStore, PgIndexerStore};

    const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

    pub fn reset_database(conn: &mut PooledConnection<PgConnection>, drop_all: bool) -> Result<(), anyhow::Error> {
        info!("Resetting database ...");
        if drop_all {
            drop_all_tables(conn)
                .map_err(|e| anyhow!("Encountering error when dropping all tables {e}"))?;
        } else {
            conn.revert_all_migrations(MIGRATIONS)
                .map_err(|e| anyhow!("Error reverting all migrations {e}"))?;
        }
        conn.run_migrations(&MIGRATIONS.migrations().unwrap())
            .map_err(|e| anyhow!("Failed to run migrations {e}"))?;
        info!("Reset database complete.");
        Ok(())
    }

    fn drop_all_tables(conn: &mut PgConnection) -> Result<(), diesel::result::Error> {
        info!("Dropping all tables in the database");
        let table_names: Vec<String> = diesel::dsl::sql::<diesel::sql_types::Text>(
            "
        SELECT tablename FROM pg_tables WHERE schemaname = 'public'
    ",
        )
            .load(conn)?;

        for table_name in table_names {
            let drop_table_query = format!("DROP TABLE IF EXISTS {} CASCADE", table_name);
            diesel::sql_query(drop_table_query).execute(conn)?;
        }

        // Recreate the __diesel_schema_migrations table
        diesel::sql_query(
            "
        CREATE TABLE __diesel_schema_migrations (
            version VARCHAR(50) PRIMARY KEY,
            run_on TIMESTAMP NOT NULL DEFAULT NOW()
        )
    ",
        )
            .execute(conn)?;
        info!("Dropped all tables in the database");
        Ok(())
    }

    pub async fn setup(indexer_config: IndexerConfig) -> Result<(), IndexerError> {
        let db_url = indexer_config.get_db_url().map_err(|e| {
            IndexerError::PgPoolConnectionError(format!(
                "Failed parsing database url with error {:?}",
                e
            ))
        })?;
        let blocking_cp = new_connection_pool::<PgConnection>(&db_url, None).map_err(|e| {
            error!(
            "Failed creating Postgres connection pool with error {:?}",
            e
        );
            e
        })?;
        if indexer_config.reset_db {
            let mut conn = get_pool_connection(&blocking_cp).map_err(|e| {
                error!(
                "Failed getting Postgres connection from connection pool with error {:?}",
                e
            );
                e
            })?;
            reset_database(&mut conn, /* drop_all */ true).map_err(|e| {
                let db_err_msg = format!(
                    "Failed resetting database with url: {:?} and error: {:?}",
                    db_url, e
                );
                error!("{}", db_err_msg);
                IndexerError::PostgresResetError(db_err_msg)
            })?;
        }
        let (_registry_service, registry) = start_prometheus_server(
            // NOTE: this parses the input host addr and port number for socket addr,
            // so unwrap() is safe here.
            format!(
                "{}:{}",
                indexer_config.client_metric_host, indexer_config.client_metric_port
            )
                .parse()
                .unwrap(),
            indexer_config.rpc_client_url.as_str(),
        )?;
        let indexer_metrics = IndexerMetrics::new(&registry);
        mysten_metrics::init_metrics(&registry);

        let report_cp = blocking_cp.clone();
        let report_metrics = indexer_metrics.clone();
        tokio::spawn(async move {
            loop {
                let cp_state = report_cp.state();
                info!(
                "DB connection pool size: {}, with idle conn: {}.",
                cp_state.connections, cp_state.idle_connections
            );
                report_metrics
                    .db_conn_pool_size
                    .set(cp_state.connections as i64);
                report_metrics
                    .idle_db_conn
                    .set(cp_state.idle_connections as i64);
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            }
        });
        if indexer_config.fullnode_sync_worker {
            let store = PgIndexerStore::<PgConnection>::new(blocking_cp, indexer_metrics.clone());
            return Indexer::start_writer::<PgIndexerStore<PgConnection>, PgConnection>(&indexer_config, store, indexer_metrics).await;
        } else if indexer_config.rpc_server_worker {
            return Indexer::start_reader::<PgConnection>(&indexer_config, &registry, db_url).await;
        } else if indexer_config.analytical_worker {
            let store = PgIndexerAnalyticalStore::new(blocking_cp);
            return Indexer::start_analytical_worker::<PgConnection>(store, indexer_metrics.clone()).await;
        }
        Ok(())
    }
}

#[cfg(feature = "mysql-feature")]
mod setup_mysql {}