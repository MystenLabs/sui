// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod pg;

use async_trait::async_trait;
use diesel::{
    query_builder::{BoxedSelectStatement, FromClause, QueryFragment},
    query_dsl::{methods::LimitDsl, LoadQuery},
    Connection, QuerySource,
};

use crate::error::Error;

/// Database Backend in use -- abstracting a specific implementation.
pub(crate) type Db = pg::PgManager_;

/// Helper types to access associated types on `Db`.
pub(crate) type DbConnection = <Db as QueryExecutor>::Connection;
pub(crate) type DbBackend = <DbConnection as Connection>::Backend;

/// A generic boxed query (compatible with the return type of `into_boxed` on diesel's table DSL).
///
/// - ST is the SqlType of the rows selected.
/// - QS is the QuerySource (the table(s) being selected from).
/// - QE is an instance of `QueryExecutor`, wrapping a particular diesel `Connection` and `Backend`.
/// - GB is the GroupBy clause.
///
/// These type parameters should usually be inferred by context.
pub(crate) type BoxedQuery<ST, QS, QE, GB> = BoxedSelectStatement<
    'static,
    ST,
    FromClause<QS>,
    <<QE as QueryExecutor>::Connection as Connection>::Backend,
    GB,
>;

/// Interface for accessing relational data written by the Indexer, agnostic of the database
/// back-end being used.
#[async_trait]
pub(crate) trait QueryExecutor {
    type Connection: Connection;

    /// Run a query that fetches a single value. `query` is a thunk that returns a boxed query when
    /// called.
    async fn result<Q, ST, QS, GB, U>(&self, query: Q) -> Result<U, Error>
    where
        Q: Fn() -> BoxedQuery<ST, QS, Self, GB>,
        BoxedQuery<ST, QS, Self, GB>: LoadQuery<'static, Self::Connection, U>,
        BoxedQuery<ST, QS, Self, GB>: QueryFragment<<Self::Connection as Connection>::Backend>,
        QS: QuerySource,
        Q: Send + 'static,
        U: Send + 'static;

    /// Run a query that fetches multiple values. `query` is a thunk that returns a boxed query when
    /// called.
    async fn results<Q, ST, QS, GB, U>(&self, query: Q) -> Result<Vec<U>, Error>
    where
        Q: Fn() -> BoxedQuery<ST, QS, Self, GB>,
        BoxedQuery<ST, QS, Self, GB>: LoadQuery<'static, Self::Connection, U>,
        BoxedQuery<ST, QS, Self, GB>: QueryFragment<<Self::Connection as Connection>::Backend>,
        QS: QuerySource,
        Q: Send + 'static,
        U: Send + 'static;

    /// Run a query that fetches a single value that might not exist. `query` is a thunk that
    /// returns a boxed query when called.
    async fn optional<Q, ST, QS, GB, U>(&self, query: Q) -> Result<Option<U>, Error>
    where
        Q: Fn() -> BoxedQuery<ST, QS, Self, GB>,
        BoxedQuery<ST, QS, Self, GB>: LoadQuery<'static, Self::Connection, U>,
        BoxedQuery<ST, QS, Self, GB>: QueryFragment<<Self::Connection as Connection>::Backend>,
        QS: QuerySource,
        Q: Send + 'static,
        U: Send + 'static;

    /// Helper to limit a query that fetches multiple values to return only its first value. `query`
    /// is a thunk that returns a boxed query when called.
    async fn first<Q, ST, QS, GB, U>(&self, query: Q) -> Result<U, Error>
    where
        Q: Fn() -> BoxedQuery<ST, QS, Self, GB>,
        BoxedQuery<ST, QS, Self, GB>: LoadQuery<'static, Self::Connection, U>,
        BoxedQuery<ST, QS, Self, GB>: QueryFragment<<Self::Connection as Connection>::Backend>,
        BoxedQuery<ST, QS, Self, GB>: LimitDsl<Output = BoxedQuery<ST, QS, Self, GB>>,
        QS: QuerySource,
        Q: Send + 'static,
        U: Send + 'static,
    {
        self.result(move || query().limit(1i64)).await
    }
}
