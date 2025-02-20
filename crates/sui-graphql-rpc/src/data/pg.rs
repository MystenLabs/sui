// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::QueryExecutor;
use crate::{config::Limits, error::Error, metrics::Metrics};
use async_trait::async_trait;
use diesel::{
    pg::Pg,
    query_builder::{Query, QueryFragment, QueryId},
    QueryResult,
};
use diesel_async::{methods::LoadQuery, scoped_futures::ScopedBoxFuture};
use diesel_async::{scoped_futures::ScopedFutureExt, RunQueryDsl};
use std::fmt;
use std::time::Instant;
use sui_indexer::indexer_reader::IndexerReader;

use tracing::error;

#[derive(Clone)]
pub(crate) struct PgExecutor {
    pub inner: IndexerReader,
    pub limits: Limits,
    pub metrics: Metrics,
}

pub(crate) struct PgConnection<'c> {
    max_cost: u32,
    conn: &'c mut diesel_async::AsyncPgConnection,
}

pub(crate) struct ByteaLiteral<'a>(pub &'a [u8]);

impl PgExecutor {
    pub(crate) fn new(inner: IndexerReader, limits: Limits, metrics: Metrics) -> Self {
        Self {
            inner,
            limits,
            metrics,
        }
    }
}

#[async_trait]
impl QueryExecutor for PgExecutor {
    type Connection = diesel_async::AsyncPgConnection;
    type Backend = Pg;
    type DbConnection<'c> = PgConnection<'c>;

    async fn execute<'c, T, U, E>(&self, txn: T) -> Result<U, Error>
    where
        T: for<'r> FnOnce(
                &'r mut Self::DbConnection<'_>,
            ) -> ScopedBoxFuture<'static, 'r, Result<U, E>>
            + Send
            + 'c,
        E: From<diesel::result::Error> + std::error::Error,
        T: Send + 'static,
        U: Send + 'static,
        E: Send + 'static,
    {
        let max_cost = self.limits.max_db_query_cost;
        let instant = Instant::now();
        let mut connection = self
            .inner
            .pool()
            .get()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;

        let result = connection
            .build_transaction()
            .read_only()
            .run(|conn| {
                async move {
                    let mut connection = PgConnection { max_cost, conn };
                    txn(&mut connection).await
                }
                .scope_boxed()
            })
            .await;

        self.metrics
            .observe_db_data(instant.elapsed(), result.is_ok());
        if let Err(e) = &result {
            error!("DB query error: {e:?}");
        }
        result.map_err(|e| Error::Internal(e.to_string()))
    }

    async fn execute_repeatable<'c, T, U, E>(&self, txn: T) -> Result<U, Error>
    where
        T: for<'r> FnOnce(
                &'r mut Self::DbConnection<'_>,
            ) -> ScopedBoxFuture<'static, 'r, Result<U, E>>
            + Send
            + 'c,
        E: From<diesel::result::Error> + std::error::Error,
        T: Send + 'static,
        U: Send + 'static,
        E: Send + 'static,
    {
        let max_cost = self.limits.max_db_query_cost;
        let instant = Instant::now();

        let mut connection = self
            .inner
            .pool()
            .get()
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;

        let result = connection
            .build_transaction()
            .read_only()
            .repeatable_read()
            .run(|conn| {
                async move {
                    //
                    txn(&mut PgConnection { max_cost, conn }).await
                }
                .scope_boxed()
            })
            .await;

        self.metrics
            .observe_db_data(instant.elapsed(), result.is_ok());
        if let Err(e) = &result {
            error!("DB query error: {e:?}");
        }
        result.map_err(|e| Error::Internal(e.to_string()))
    }
}

#[async_trait]
impl<'c> super::DbConnection for PgConnection<'c> {
    type Connection = diesel_async::AsyncPgConnection;
    type Backend = Pg;

    async fn result<T, Q, U>(&mut self, query: T) -> QueryResult<U>
    where
        T: Fn() -> Q + Send,
        Q: diesel::query_builder::Query + Send + 'static,
        Q: LoadQuery<'static, Self::Connection, U>,
        Q: QueryId + QueryFragment<Self::Backend>,
        U: Send,
    {
        query_cost::log(self.conn, self.max_cost, query()).await;
        query().get_result(self.conn).await
    }

    async fn results<T, Q, U>(&mut self, query: T) -> QueryResult<Vec<U>>
    where
        T: Fn() -> Q + Send,
        Q: diesel::query_builder::Query + Send + 'static,
        Q: LoadQuery<'static, Self::Connection, U>,
        Q: QueryId + QueryFragment<Self::Backend>,
        U: Send,
    {
        query_cost::log(self.conn, self.max_cost, query()).await;
        query().get_results(self.conn).await
    }
}

impl fmt::Display for ByteaLiteral<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "'\\x{}'::bytea", hex::encode(self.0))
    }
}

pub(crate) fn bytea_literal(slice: &[u8]) -> ByteaLiteral<'_> {
    ByteaLiteral(slice)
}

/// Support for calculating estimated query cost using EXPLAIN and then logging it.
mod query_cost {
    use super::*;

    use diesel::{query_builder::AstPass, sql_types::Text, QueryResult};
    use diesel_async::AsyncPgConnection;
    use serde_json::Value;
    use tap::{TapFallible, TapOptional};
    use tracing::{debug, info, warn};

    #[derive(Debug, Clone, Copy, QueryId)]
    struct Explained<Q> {
        query: Q,
    }

    impl<Q: Query> Query for Explained<Q> {
        type SqlType = Text;
    }

    impl<Q: QueryFragment<Pg>> QueryFragment<Pg> for Explained<Q> {
        fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, Pg>) -> QueryResult<()> {
            out.push_sql("EXPLAIN (FORMAT JSON) ");
            self.query.walk_ast(out.reborrow())?;
            Ok(())
        }
    }

    /// Run `EXPLAIN` on the `query`, and log the estimated cost.
    pub(crate) async fn log<Q>(conn: &mut AsyncPgConnection, max_db_query_cost: u32, query: Q)
    where
        Q: Query + QueryId + QueryFragment<Pg> + RunQueryDsl<AsyncPgConnection> + Send,
    {
        debug!("Estimating: {}", diesel::debug_query(&query).to_string());

        let Some(cost) = explain(conn, query).await else {
            warn!("Failed to extract cost from EXPLAIN.");
            return;
        };

        if cost > max_db_query_cost as f64 {
            warn!(cost, max_db_query_cost, exceeds = true, "Estimated cost");
        } else {
            info!(cost, max_db_query_cost, exceeds = false, "Estimated cost");
        }
    }

    pub(crate) async fn explain<Q>(conn: &mut AsyncPgConnection, query: Q) -> Option<f64>
    where
        Q: Query + QueryId + QueryFragment<Pg> + RunQueryDsl<AsyncPgConnection> + Send,
    {
        let result: String = Explained { query }
            .get_result(conn)
            .await
            .tap_err(|e| warn!("Failed to run EXPLAIN: {e}"))
            .ok()?;

        let parsed = serde_json::from_str(&result)
            .tap_err(|e| warn!("Failed to parse EXPLAIN result: {e}"))
            .ok()?;

        extract_cost(&parsed).tap_none(|| warn!("Failed to extract cost from EXPLAIN"))
    }

    fn extract_cost(parsed: &Value) -> Option<f64> {
        parsed.get(0)?.get("Plan")?.get("Total Cost")?.as_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::QueryDsl;
    use sui_framework::BuiltInFramework;
    use sui_indexer::{
        database::Connection, db::reset_database, models::objects::StoredObject, schema::objects,
        types::IndexedObject,
    };
    use sui_pg_db::temp::TempDb;

    #[tokio::test]
    async fn test_query_cost() {
        let database = TempDb::new().unwrap();
        reset_database(
            Connection::dedicated(database.database().url())
                .await
                .unwrap(),
        )
        .await
        .unwrap();
        let mut connection = Connection::dedicated(database.database().url())
            .await
            .unwrap();

        let objects: Vec<StoredObject> = BuiltInFramework::iter_system_packages()
            .map(|pkg| IndexedObject::from_object(1, pkg.genesis_object(), None).into())
            .collect();

        let expect = objects.len();
        let actual = diesel::insert_into(objects::dsl::objects)
            .values(objects)
            .execute(&mut connection)
            .await
            .unwrap();

        assert_eq!(expect, actual, "Failed to write objects");

        use objects::dsl;
        let query_one = dsl::objects.select(dsl::objects.star()).limit(1);
        let query_all = dsl::objects.select(dsl::objects.star());

        // Test estimating query costs
        let cost_one = query_cost::explain(&mut connection, query_one)
            .await
            .unwrap();
        let cost_all = query_cost::explain(&mut connection, query_all)
            .await
            .unwrap();

        assert!(
            cost_one < cost_all,
            "cost_one = {cost_one} >= {cost_all} = cost_all"
        );
    }
}
