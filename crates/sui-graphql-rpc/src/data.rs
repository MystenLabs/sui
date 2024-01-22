// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod pg;

use async_trait::async_trait;
use diesel::{
    deserialize::FromSqlRow,
    query_builder::{BoxedSelectStatement, FromClause, QueryFragment, QueryId},
    query_dsl::{methods::LimitDsl, LoadQuery},
    sql_types::Untyped,
    QueryResult,
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
pub(crate) type Query<ST, QS, GB> =
    BoxedSelectStatement<'static, ST, FromClause<QS>, DieselBackend, GB>;

#[derive(Clone)]
pub(crate) struct RawSqlQuery {
    // typically more complex queries, alias to help
    pub alias: String,
    pub select: String,
    pub where_clause: Option<String>,
    pub order_by: Option<String>,
    pub limit: Option<i64>,
}

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

    fn raw_results<U>(&mut self, query: String) -> QueryResult<Vec<U>>
    where
        U: FromSqlRow<Untyped, Self::Backend> + 'static;

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

impl RawSqlQuery {
    pub(crate) fn new_with_select(alias: String, select: String) -> Self {
        Self {
            select,
            alias,
            where_clause: None,
            order_by: None,
            limit: None,
        }
    }

    pub(crate) fn and_filter(&mut self, sql: String) {
        match &mut self.where_clause {
            Some(where_clause) => {
                *where_clause = format!("({}) AND ({})", where_clause, sql);
            }
            None => {
                self.where_clause = Some(sql);
            }
        }
    }

    pub(crate) fn or_filter(&mut self, sql: String) {
        match &mut self.where_clause {
            Some(where_clause) => {
                *where_clause = format!("({}) OR ({})", where_clause, sql);
            }
            None => {
                self.where_clause = Some(sql);
            }
        }
    }

    pub(crate) fn order_by(&mut self, field: &str, asc: bool) {
        let order = if asc { "ASC" } else { "DESC" };
        match &mut self.order_by {
            Some(order_by) => {
                order_by.push_str(", ");
                order_by.push_str(field);
                order_by.push(' ');
                order_by.push_str(order);
            }
            None => {
                let mut clause = String::from("");
                clause.push_str(field);
                clause.push(' ');
                clause.push_str(order);
                self.order_by = Some(clause);
            }
        }
    }

    pub(crate) fn limit(&mut self, limit: i64) {
        self.limit = Some(limit);
    }

    pub(crate) fn build(&self) -> String {
        let mut sql = self.select.clone();
        if let Some(where_clause) = &self.where_clause {
            sql.push_str(" WHERE ");
            sql.push_str(where_clause);
        }
        if let Some(order_by) = &self.order_by {
            sql.push_str(" ORDER BY ");
            sql.push_str(order_by);
        }
        if let Some(limit) = &self.limit {
            sql.push_str(" LIMIT ");
            sql.push_str(&format!("{limit}"));
        }

        sql
    }

    pub(crate) fn finish(&self) -> String {
        let mut sql = self.build();
        sql.push(';');

        sql
    }
}
