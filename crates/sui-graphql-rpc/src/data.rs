// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod apys;
pub(crate) mod package_resolver;
pub(crate) mod pg;

use std::sync::Arc;

use async_graphql::dataloader::DataLoader as AGDataLoader;
use async_trait::async_trait;
use diesel::{
    query_builder::{BoxedSelectStatement, FromClause, QueryFragment, QueryId},
    query_dsl::{methods::LimitDsl, LoadQuery},
    QueryResult,
};

use crate::error::Error;

/// Database Backend in use -- abstracting a specific implementation.
pub(crate) type Db = pg::PgExecutor;

/// Helper types to access associated types on `Db`.
pub(crate) type Conn<'c> = <Db as QueryExecutor>::DbConnection<'c>;
pub(crate) type DieselConn = <Db as QueryExecutor>::Connection;
pub(crate) type DieselBackend = <Db as QueryExecutor>::Backend;

/// Helper types for accessing a shared `DataLoader` instance.
#[derive(Clone)]
pub(crate) struct DataLoader(pub Arc<AGDataLoader<Db>>);

/// A generic boxed query (compatible with the return type of `into_boxed` on diesel's table DSL).
///
/// - ST is the SqlType of the rows selected.
/// - QS is the QuerySource (the table(s) being selected from).
/// - GB is the GroupBy clause.
///
/// These type parameters should usually be inferred by context.
pub(crate) type Query<ST, QS, GB> =
    BoxedSelectStatement<'static, ST, FromClause<QS>, DieselBackend, GB>;

/// Interface for accessing relational data written by the Indexer, agnostic of the database
/// back-end being used.
#[async_trait]
pub(crate) trait QueryExecutor {
    type Backend: diesel::backend::Backend;
    type Connection: diesel::Connection;

    type DbConnection<'c>: DbConnection<Connection = Self::Connection, Backend = Self::Backend>
    where
        Self: 'c;

    /// Execute `txn` with read committed isolation. `txn` is supplied a database connection to
    /// issue queries over.
    async fn execute<T, U, E>(&self, txn: T) -> Result<U, Error>
    where
        T: FnOnce(&mut Self::DbConnection<'_>) -> Result<U, E>,
        E: From<diesel::result::Error> + std::error::Error,
        T: Send + 'static,
        U: Send + 'static,
        E: Send + 'static;

    /// Execute `txn` with repeatable reads and no phantom reads -- multiple calls to the same query
    /// should produce the same results. `txn` is supplied a database connection to issue queries
    /// over.
    async fn execute_repeatable<T, U, E>(&self, txn: T) -> Result<U, Error>
    where
        T: FnOnce(&mut Self::DbConnection<'_>) -> Result<U, E>,
        E: From<diesel::result::Error> + std::error::Error,
        T: Send + 'static,
        U: Send + 'static,
        E: Send + 'static;
}

pub(crate) trait DbConnection {
    type Backend: diesel::backend::Backend;
    type Connection: diesel::Connection<Backend = Self::Backend>;

    /// Run a query that fetches a single value. `query` is a thunk that returns a query when
    /// called.
    fn result<Q, U>(&mut self, query: impl Fn() -> Q) -> QueryResult<U>
    where
        Q: diesel::query_builder::Query,
        Q: LoadQuery<'static, Self::Connection, U>,
        Q: QueryId + QueryFragment<Self::Backend>;

    /// Run a query that fetches multiple values. `query` is a thunk that returns a query when
    /// called.
    fn results<Q, U>(&mut self, query: impl Fn() -> Q) -> QueryResult<Vec<U>>
    where
        Q: diesel::query_builder::Query,
        Q: LoadQuery<'static, Self::Connection, U>,
        Q: QueryId + QueryFragment<Self::Backend>;

    /// Helper to limit a query that fetches multiple values to return only its first value. `query`
    /// is a thunk that returns a query when called.
    fn first<Q: LimitDsl, U>(&mut self, query: impl Fn() -> Q) -> QueryResult<U>
    where
        <Q as LimitDsl>::Output: diesel::query_builder::Query,
        <Q as LimitDsl>::Output: LoadQuery<'static, Self::Connection, U>,
        <Q as LimitDsl>::Output: QueryId + QueryFragment<Self::Backend>,
    {
        self.result(move || query().limit(1i64))
    }
}

impl DataLoader {
    pub(crate) fn new(db: Db) -> Self {
        Self(Arc::new(AGDataLoader::new(db, tokio::spawn)))
    }
}
