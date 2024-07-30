// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::time::Duration;

use diesel::connection::BoxableConnection;
use diesel::r2d2::{Pool, PooledConnection, R2D2Connection};
use diesel::{r2d2::ConnectionManager, sql_query, RunQueryDsl};

use crate::errors::IndexerError;

pub type ConnectionPool<T> = Pool<ConnectionManager<T>>;
pub type PoolConnection<T> = PooledConnection<ConnectionManager<T>>;

#[derive(Debug, Clone, Copy)]
pub struct ConnectionPoolConfig {
    pub pool_size: u32,
    pub connection_timeout: Duration,
    pub statement_timeout: Duration,
}

impl ConnectionPoolConfig {
    const DEFAULT_POOL_SIZE: u32 = 100;
    const DEFAULT_CONNECTION_TIMEOUT: u64 = 3600;
    const DEFAULT_STATEMENT_TIMEOUT: u64 = 3600;

    fn connection_config(&self) -> ConnectionConfig {
        ConnectionConfig {
            statement_timeout: self.statement_timeout,
            read_only: false,
        }
    }

    pub fn set_pool_size(&mut self, size: u32) {
        self.pool_size = size;
    }

    pub fn set_connection_timeout(&mut self, timeout: Duration) {
        self.connection_timeout = timeout;
    }

    pub fn set_statement_timeout(&mut self, timeout: Duration) {
        self.statement_timeout = timeout;
    }
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        let db_pool_size = std::env::var("DB_POOL_SIZE")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(Self::DEFAULT_POOL_SIZE);
        let conn_timeout_secs = std::env::var("DB_CONNECTION_TIMEOUT")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(Self::DEFAULT_CONNECTION_TIMEOUT);
        let statement_timeout_secs = std::env::var("DB_STATEMENT_TIMEOUT")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(Self::DEFAULT_STATEMENT_TIMEOUT);

        Self {
            pool_size: db_pool_size,
            connection_timeout: Duration::from_secs(conn_timeout_secs),
            statement_timeout: Duration::from_secs(statement_timeout_secs),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConnectionConfig {
    pub statement_timeout: Duration,
    pub read_only: bool,
}

impl<T: R2D2Connection + 'static> diesel::r2d2::CustomizeConnection<T, diesel::r2d2::Error>
    for ConnectionConfig
{
    fn on_acquire(&self, conn: &mut T) -> std::result::Result<(), diesel::r2d2::Error> {
        #[cfg(feature = "postgres-feature")]
        {
            conn.as_any_mut()
                .downcast_mut::<diesel::PgConnection>()
                .map_or_else(
                    || {
                        Err(diesel::r2d2::Error::QueryError(
                            diesel::result::Error::DeserializationError(
                                "Failed to downcast connection to PgConnection"
                                    .to_string()
                                    .into(),
                            ),
                        ))
                    },
                    |pg_conn| {
                        sql_query(format!(
                            "SET statement_timeout = {}",
                            self.statement_timeout.as_millis(),
                        ))
                        .execute(pg_conn)
                        .map_err(diesel::r2d2::Error::QueryError)?;

                        if self.read_only {
                            sql_query("SET default_transaction_read_only = 't'")
                                .execute(pg_conn)
                                .map_err(diesel::r2d2::Error::QueryError)?;
                        }
                        Ok(())
                    },
                )?;
            Ok(())
        }
        #[cfg(not(feature = "postgres-feature"))]
        {
            Ok(())
        }
    }
}

pub fn new_connection_pool<T: R2D2Connection + 'static>(
    db_url: &str,
    pool_size: Option<u32>,
) -> Result<ConnectionPool<T>, IndexerError> {
    let pool_config = ConnectionPoolConfig::default();
    new_connection_pool_with_config(db_url, pool_size, pool_config)
}

pub fn new_connection_pool_with_config<T: R2D2Connection + 'static>(
    db_url: &str,
    pool_size: Option<u32>,
    pool_config: ConnectionPoolConfig,
) -> Result<ConnectionPool<T>, IndexerError> {
    let manager = ConnectionManager::<T>::new(db_url);

    let pool_size = pool_size.unwrap_or(pool_config.pool_size);
    Pool::builder()
        .max_size(pool_size)
        .connection_timeout(pool_config.connection_timeout)
        .connection_customizer(Box::new(pool_config.connection_config()))
        .build(manager)
        .map_err(|e| {
            IndexerError::PgConnectionPoolInitError(format!(
                "Failed to initialize connection pool for {db_url} with error: {e:?}"
            ))
        })
}

pub fn get_pool_connection<T: R2D2Connection + Send + 'static>(
    pool: &ConnectionPool<T>,
) -> Result<PoolConnection<T>, IndexerError> {
    pool.get().map_err(|e| {
        IndexerError::PgPoolConnectionError(format!(
            "Failed to get connection from PG connection pool with error: {:?}",
            e
        ))
    })
}

pub fn reset_database<T: R2D2Connection + Send + 'static>(
    conn: &mut PoolConnection<T>,
    drop_all: bool,
) -> Result<(), anyhow::Error> {
    #[cfg(feature = "postgres-feature")]
    {
        conn.as_any_mut()
            .downcast_mut::<PoolConnection<diesel::PgConnection>>()
            .map_or_else(
                || Err(anyhow!("Failed to downcast connection to PgConnection")),
                |pg_conn| {
                    setup_postgres::reset_database(pg_conn, drop_all)?;
                    Ok(())
                },
            )?;
    }
    #[cfg(feature = "mysql-feature")]
    #[cfg(not(feature = "postgres-feature"))]
    {
        conn.as_any_mut()
            .downcast_mut::<PoolConnection<diesel::MysqlConnection>>()
            .map_or_else(
                || Err(anyhow!("Failed to downcast connection to PgConnection")),
                |mysql_conn| {
                    setup_mysql::reset_database(mysql_conn, drop_all)
                        .map_err(diesel::r2d2::Error::QueryError)?;
                    Ok(())
                },
            )?;
    }
    Ok(())
}

#[cfg(feature = "postgres-feature")]
pub mod setup_postgres {
    use crate::db::{get_pool_connection, new_connection_pool, PoolConnection};
    use crate::errors::IndexerError;
    use crate::indexer::Indexer;
    use crate::metrics::IndexerMetrics;
    use crate::store::PgIndexerStore;
    use crate::IndexerConfig;
    use anyhow::anyhow;
    use diesel::migration::MigrationSource;
    use diesel::{PgConnection, RunQueryDsl};
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
    use prometheus::Registry;
    use secrecy::ExposeSecret;
    use tracing::{error, info};

    const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

    pub fn reset_database(
        conn: &mut PoolConnection<PgConnection>,
        drop_all: bool,
    ) -> Result<(), anyhow::Error> {
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

    pub async fn setup(
        indexer_config: IndexerConfig,
        registry: Registry,
    ) -> Result<(), IndexerError> {
        let db_url_secret = indexer_config.get_db_url().map_err(|e| {
            IndexerError::PgPoolConnectionError(format!(
                "Failed parsing database url with error {:?}",
                e
            ))
        })?;
        let db_url = db_url_secret.expose_secret();
        let blocking_cp = new_connection_pool::<PgConnection>(db_url, None).map_err(|e| {
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
            return Indexer::start_writer::<PgIndexerStore<PgConnection>, PgConnection>(
                &indexer_config,
                store,
                indexer_metrics,
            )
            .await;
        } else if indexer_config.rpc_server_worker {
            return Indexer::start_reader::<PgConnection>(
                &indexer_config,
                &registry,
                db_url.to_string(),
            )
            .await;
        }
        Ok(())
    }
}

#[cfg(feature = "mysql-feature")]
#[cfg(not(feature = "postgres-feature"))]
pub mod setup_mysql {
    use crate::db::PoolConnection;
    use crate::errors::IndexerError;
    use crate::IndexerConfig;
    use diesel::MysqlConnection;
    use prometheus::Registry;

    pub fn reset_database(
        _conn: &mut PoolConnection<MysqlConnection>,
        _drop_all: bool,
    ) -> Result<(), anyhow::Error> {
        todo!()
    }
    pub async fn setup(
        _indexer_config: IndexerConfig,
        registry: Registry,
    ) -> Result<(), IndexerError> {
        todo!()
    }
}
