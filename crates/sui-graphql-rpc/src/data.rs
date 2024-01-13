// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod pg;

use async_trait::async_trait;
use diesel::{
    query_builder::{BoxedSelectStatement, FromClause, QueryFragment},
    query_dsl::{methods::LimitDsl, LoadQuery},
    QueryResult, QuerySource,
};

use crate::error::Error;

/// Database Backend in use -- abstracting a specific implementation.
pub(crate) type Db = pg::PgExecutor;

/// Helper types to access associated types on `Db`.
pub(crate) type Conn<'c> = <Db as QueryExecutor>::DbConnection<'c>;
pub(crate) type DieselConn = <Db as QueryExecutor>::Connection;
pub(crate) type DieselBackend = <Db as QueryExecutor>::Backend;

/// A generic boxed query (compatible with the return type of `into_boxed` on diesel's table DSL).
///
/// - ST is the SqlType of the rows selected.
/// - QS is the QuerySource (the table(s) being selected from).
/// - GB is the GroupBy clause.
///
/// These type parameters should usually be inferred by context.
pub(crate) type Query<ST, QS, GB> = Query_<ST, QS, DieselBackend, GB>;

/// Generic boxed query type that is also generic over the database backend. Used in the signatures
/// of `QueryExecutor` and `DbConnection` and their implementations. (Clients can use the `Query`
/// type above, which is specialised to the DB Backend in use.)
pub(crate) type Query_<ST, QS, DB, GB> = BoxedSelectStatement<'static, ST, FromClause<QS>, DB, GB>;

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

    /// Run a query that fetches a single value. `query` is a thunk that returns a boxed query when
    /// called.
    fn result<Q, ST, QS, GB, U>(&mut self, query: Q) -> QueryResult<U>
    where
        Q: Fn() -> Query_<ST, QS, Self::Backend, GB>,
        Query_<ST, QS, Self::Backend, GB>: LoadQuery<'static, Self::Connection, U>,
        Query_<ST, QS, Self::Backend, GB>: QueryFragment<Self::Backend>,
        QS: QuerySource;

    /// Run a query that fetches multiple values. `query` is a thunk that returns a boxed query when
    /// called.
    fn results<Q, ST, QS, GB, U>(&mut self, query: Q) -> QueryResult<Vec<U>>
    where
        Q: Fn() -> Query_<ST, QS, Self::Backend, GB>,
        Query_<ST, QS, Self::Backend, GB>: LoadQuery<'static, Self::Connection, U>,
        Query_<ST, QS, Self::Backend, GB>: QueryFragment<Self::Backend>,
        QS: QuerySource;

    /// Helper to limit a query that fetches multiple values to return only its first value. `query`
    /// is a thunk that returns a boxed query when called.
    fn first<Q, ST, QS, GB, U>(&mut self, query: Q) -> QueryResult<U>
    where
        Q: Fn() -> Query_<ST, QS, Self::Backend, GB>,
        Query_<ST, QS, Self::Backend, GB>: LoadQuery<'static, Self::Connection, U>,
        Query_<ST, QS, Self::Backend, GB>: QueryFragment<Self::Backend>,
        Query_<ST, QS, Self::Backend, GB>: LimitDsl<Output = Query_<ST, QS, Self::Backend, GB>>,
        QS: QuerySource,
    {
        self.result(move || query().limit(1i64))
    }
}
