// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::anyhow;
use async_graphql::dataloader::DataLoader;
use diesel::deserialize::FromSqlRow;
use diesel::expression::QueryMetadata;
use diesel::pg::Pg;
use diesel::query_builder::{Query, QueryFragment, QueryId};
use diesel::query_dsl::methods::LimitDsl;
use diesel::query_dsl::CompatibleType;
use diesel_async::RunQueryDsl;
use prometheus::Registry;
use sui_indexer_alt_metrics::db::DbConnectionStatsCollector;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use url::Url;

use crate::{error::Error, metrics::ReaderMetrics};

pub use sui_pg_db as db;

/// This wrapper type exists to perform error conversion between the data fetching layer and the
/// RPC layer, metrics collection, and debug logging of database queries.
#[derive(Clone)]
pub struct PgReader {
    db: Option<db::Db>,
    metrics: Arc<ReaderMetrics>,
    cancel: CancellationToken,
}

pub struct Connection<'p> {
    conn: db::Connection<'p>,
    metrics: Arc<ReaderMetrics>,
}

impl PgReader {
    /// Create a new database reader. If `database_url` is `None`, the reader will not accept any
    /// connection requests (they will all fail).
    pub async fn new(
        database_url: Option<Url>,
        db_args: db::DbArgs,
        registry: &Registry,
        cancel: CancellationToken,
    ) -> Result<Self, Error> {
        let db = if let Some(database_url) = database_url {
            let db = db::Db::for_read(database_url, db_args)
                .await
                .map_err(Error::PgCreate)?;

            registry
                .register(Box::new(DbConnectionStatsCollector::new(
                    Some("rpc_db"),
                    db.clone(),
                )))
                .map_err(|e| Error::PgCreate(e.into()))?;

            Some(db)
        } else {
            None
        };

        let metrics = ReaderMetrics::new(registry);

        Ok(Self {
            db,
            metrics,
            cancel,
        })
    }

    /// Create a data loader backed by this reader.
    pub fn as_data_loader(&self) -> DataLoader<Self> {
        DataLoader::new(self.clone(), tokio::spawn)
    }

    /// Acquire a connection to the database. This can potentially fail if the service is cancelled
    /// while the connection is being acquired.
    pub async fn connect(&self) -> Result<Connection<'_>, Error> {
        let Some(db) = &self.db else {
            return Err(Error::PgConnect(anyhow!("No database to connect to")));
        };

        tokio::select! {
            _ = self.cancel.cancelled() => {
                Err(Error::PgConnect(anyhow!("Cancelled while connecting to the database")))
            }

            conn = db.connect() => {
                Ok(Connection {
                    conn: conn.map_err(Error::PgConnect)?,
                    metrics: self.metrics.clone(),
                })
            }
        }
    }
}

impl Connection<'_> {
    pub async fn first<'q, Q, ST, U>(&mut self, query: Q) -> Result<U, Error>
    where
        Q: LimitDsl,
        Q::Output: Query + QueryFragment<Pg> + QueryId + Send + 'q,
        <Q::Output as Query>::SqlType: CompatibleType<U, Pg, SqlType = ST>,
        U: Send + FromSqlRow<ST, Pg> + 'static,
        Pg: QueryMetadata<<Q::Output as Query>::SqlType>,
        ST: 'static,
    {
        let query = query.limit(1);
        let query_debug = diesel::debug_query(&query).to_string();
        debug!("{query_debug}");

        self.metrics.db_requests_received.inc();
        let _guard = self.metrics.db_latency.start_timer();

        let res = query.get_result(&mut self.conn).await;
        if res.as_ref().is_err_and(is_timeout) {
            warn!(query = query_debug, "Query timed out");
        }

        if res.is_ok() {
            self.metrics.db_requests_succeeded.inc();
        } else {
            self.metrics.db_requests_failed.inc();
        }

        Ok(res?)
    }

    pub async fn results<'q, Q, ST, U>(&mut self, query: Q) -> Result<Vec<U>, Error>
    where
        Q: Query + QueryFragment<Pg> + QueryId + Send + 'q,
        Q::SqlType: CompatibleType<U, Pg, SqlType = ST>,
        U: Send + FromSqlRow<ST, Pg> + 'static,
        Pg: QueryMetadata<Q::SqlType>,
        ST: 'static,
    {
        let query_debug = diesel::debug_query(&query).to_string();
        debug!("{query_debug}");

        self.metrics.db_requests_received.inc();
        let _guard = self.metrics.db_latency.start_timer();

        let res = query.get_results(&mut self.conn).await;
        if res.as_ref().is_err_and(is_timeout) {
            warn!(query = query_debug, "Query timed out");
        }

        if res.is_ok() {
            self.metrics.db_requests_succeeded.inc();
        } else {
            self.metrics.db_requests_failed.inc();
        }

        Ok(res?)
    }
}

/// Detect whether the error is due to a timeout.
fn is_timeout(err: &diesel::result::Error) -> bool {
    let diesel::result::Error::DatabaseError(_, info) = err else {
        return false;
    };

    info.message() == "canceling statement due to statement timeout"
}
