// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::dataloader::DataLoader;
use diesel::deserialize::FromSqlRow;
use diesel::expression::QueryMetadata;
use diesel::pg::Pg;
use diesel::query_builder::{Query, QueryFragment, QueryId};
use diesel::query_dsl::methods::LimitDsl;
use diesel::query_dsl::CompatibleType;
use diesel::result::Error as DieselError;
use diesel_async::RunQueryDsl;
use prometheus::Registry;
use sui_indexer_alt_metrics::db::DbConnectionStatsCollector;
use sui_pg_db as db;
use tracing::debug;

use crate::metrics::RpcMetrics;

/// This wrapper type exists to perform error conversion between the data fetching layer and the
/// RPC layer, metrics collection, and debug logging of database queries.
#[derive(Clone)]
pub(crate) struct Reader {
    db: db::Db,
    metrics: Arc<RpcMetrics>,
}

pub(crate) struct Connection<'p> {
    conn: db::Connection<'p>,
    metrics: Arc<RpcMetrics>,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum ReadError {
    #[error(transparent)]
    Create(anyhow::Error),

    #[error(transparent)]
    Connect(anyhow::Error),

    #[error(transparent)]
    RunQuery(#[from] DieselError),
}

impl Reader {
    pub(crate) async fn new(
        db_args: db::DbArgs,
        metrics: Arc<RpcMetrics>,
        registry: &Registry,
    ) -> Result<Self, ReadError> {
        let db = db::Db::for_read(db_args).await.map_err(ReadError::Create)?;

        registry
            .register(Box::new(DbConnectionStatsCollector::new(
                Some("rpc_db"),
                db.clone(),
            )))
            .map_err(|e| ReadError::Create(e.into()))?;

        Ok(Self { db, metrics })
    }

    /// Create a data loader backed by this reader.
    pub(crate) fn as_data_loader(&self) -> DataLoader<Self> {
        DataLoader::new(self.clone(), tokio::spawn)
    }

    pub(crate) async fn connect(&self) -> Result<Connection<'_>, ReadError> {
        Ok(Connection {
            conn: self.db.connect().await.map_err(ReadError::Connect)?,
            metrics: self.metrics.clone(),
        })
    }
}

impl<'p> Connection<'p> {
    pub(crate) async fn first<'q, Q, ST, U>(&mut self, query: Q) -> Result<U, ReadError>
    where
        Q: LimitDsl,
        Q::Output: Query + QueryFragment<Pg> + QueryId + Send + 'q,
        <Q::Output as Query>::SqlType: CompatibleType<U, Pg, SqlType = ST>,
        U: Send + FromSqlRow<ST, Pg> + 'static,
        Pg: QueryMetadata<<Q::Output as Query>::SqlType>,
        ST: 'static,
    {
        let query = query.limit(1);
        debug!("{}", diesel::debug_query(&query));

        let _guard = self.metrics.db_latency.start_timer();
        let res = query.get_result(&mut self.conn).await;

        if res.is_ok() {
            self.metrics.db_requests_succeeded.inc();
        } else {
            self.metrics.db_requests_failed.inc();
        }

        Ok(res?)
    }

    pub(crate) async fn results<'q, Q, ST, U>(&mut self, query: Q) -> Result<Vec<U>, ReadError>
    where
        Q: Query + QueryFragment<Pg> + QueryId + Send + 'q,
        Q::SqlType: CompatibleType<U, Pg, SqlType = ST>,
        U: Send + FromSqlRow<ST, Pg> + 'static,
        Pg: QueryMetadata<Q::SqlType>,
        ST: 'static,
    {
        debug!("{}", diesel::debug_query(&query));

        let _guard = self.metrics.db_latency.start_timer();
        let res = query.get_results(&mut self.conn).await;

        if res.is_ok() {
            self.metrics.db_requests_succeeded.inc();
        } else {
            self.metrics.db_requests_failed.inc();
        }

        Ok(res?)
    }
}
