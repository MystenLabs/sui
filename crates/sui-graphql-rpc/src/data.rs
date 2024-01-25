// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod pg;

use async_trait::async_trait;
use diesel::{
    query_builder::{
        BoxedSelectStatement, BoxedSqlQuery, FromClause, QueryFragment, QueryId, SqlQuery,
    },
    query_dsl::{methods::LimitDsl, LoadQuery},
    sql_query, QueryResult,
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

pub(crate) type RawSqlQuery = BoxedSqlQuery<'static, DieselBackend, SqlQuery>;

pub(crate) struct RawQuery {
    select: String,
    where_: Option<String>,
    order_by: Vec<String>,
    group_by: Vec<String>,
    limit: Option<i64>,
    binds: Vec<String>,
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

impl RawQuery {
    pub(crate) fn new(select: String, binds: Vec<String>) -> Self {
        Self {
            select,
            where_: None,
            order_by: Vec::new(),
            group_by: Vec::new(),
            limit: None,
            binds,
        }
    }

    pub fn filter<T: std::fmt::Display>(mut self, condition: T) -> Self {
        self.where_ = match self.where_ {
            Some(where_) => Some(format!("({}) AND {}", where_, condition)),
            None => Some(condition.to_string()),
        };

        self
    }

    #[allow(dead_code)]
    pub fn or_filter<T: std::fmt::Display>(mut self, condition: T) -> Self {
        self.where_ = match self.where_ {
            Some(where_) => Some(format!("({}) OR {}", where_, condition)),
            None => Some(condition.to_string()),
        };

        self
    }

    pub fn order_by<T: ToString>(mut self, order: T) -> Self {
        self.order_by.push(order.to_string());
        self
    }

    pub fn group_by<T: ToString>(mut self, group: T) -> Self {
        self.group_by.push(group.to_string());
        self
    }

    pub fn limit(mut self, limit: i64) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn bind_value(&mut self, condition: String) {
        self.binds.push(condition);
    }

    pub fn finish(self) -> (String, Vec<String>) {
        let mut select = self.select;

        if let Some(where_) = self.where_ {
            select.push_str(" WHERE ");
            select.push_str(&where_);
        }

        let mut group_by = self.group_by.iter();

        if let Some(first) = group_by.next() {
            select.push_str(" GROUP BY ");
            select.push_str(first);
        }

        for group in group_by {
            select.push_str(", ");
            select.push_str(group);
        }

        let mut order_by = self.order_by.iter();

        if let Some(first) = order_by.next() {
            select.push_str(" ORDER BY ");
            select.push_str(first);
        }

        for order in order_by {
            select.push_str(", ");
            select.push_str(order);
        }

        if let Some(limit) = self.limit {
            select.push_str(" LIMIT ");
            select.push_str(&limit.to_string());
        }

        (select, self.binds)
    }

    pub fn into_boxed(self) -> RawSqlQuery {
        let (raw_sql_string, binds) = self.finish();

        let mut result = String::with_capacity(raw_sql_string.len());

        let mut sql_components = raw_sql_string.split("{}").enumerate();

        if let Some((_, first)) = sql_components.next() {
            result.push_str(first);
        }

        for (i, sql) in sql_components {
            result.push_str(&format!("${}", i));
            result.push_str(sql);
        }

        result.push_str(";");

        let mut diesel_query = sql_query(result).into_boxed();

        for bind in binds {
            diesel_query = diesel_query.bind::<diesel::sql_types::Text, _>(bind);
        }

        diesel_query
    }
}

#[macro_export]
macro_rules! filter {
    ($query:expr, $condition:expr $(,$binds:expr)*) => {{
        let mut query = $query;

        query = query.filter($condition);

        $(
            query.bind_value($binds.to_string());
        )*

        query
    }};
}

#[macro_export]
macro_rules! or_filter {
    ($query:expr, $condition:expr $(,$binds:expr)*) => {{
        let mut query = $query;

        query = query.or_filter($condition);

        $(
            query.bind_value($binds.to_string());
        )*

        query
    }};
}

#[macro_export]
macro_rules! query {
    ($select:expr $(,$subquery:expr)*) => {{
        use $crate::data::RawQuery;
        // Rust will complain when query! is used on a statement with no subqueries.
        #[allow(unused_mut)]
        let mut binds = vec![];

        let select = format!(
            $select,
            $({
                let (sub_sql, sub_binds) = $subquery.finish();
                binds.extend(sub_binds);
                sub_sql
            }),*
        );

        RawQuery::new(select, binds)
    }};
}

pub(crate) fn build_candidates(
    snapshot_objs: RawQueryBuilder,
    history_objs: RawQueryBuilder,
) -> RawQueryBuilder {
    let mut candidates = query!(
        r#"SELECT DISTINCT ON (object_id) * FROM (
        ({})
        UNION
        ({})
    ) o"#,
        snapshot_objs,
        history_objs
    );

    candidates
        .order_by("object_id")
        .order_by("object_version DESC")
}

pub(crate) fn build_newer(lhs: i64, rhs: i64) -> RawQueryBuilder {
    let mut newer = query!(r#"SELECT object_id, object_version FROM objects_history"#);
    filter!(
        newer,
        format!(r#"checkpoint_sequence_number BETWEEN {} AND {}"#, lhs, rhs)
    )
}

pub(crate) fn build_join(candidates: RawQueryBuilder, newer: RawQueryBuilder) -> RawQueryBuilder {
    let mut final_ = query!(
        r#"
    SELECT CAST(SUM(candidates.coin_balance) AS TEXT) as balance, COUNT(*) as count, candidates.coin_type as coin_type
        FROM ({}) candidates
        LEFT JOIN ({}) newer
        ON (
            candidates.object_id = newer.object_id
            AND candidates.object_version < newer.object_version
        )"#,
        candidates,
        newer
    );
    filter!(final_, "newer.object_version IS NULL")
}

pub(crate) fn consistent_object_read(
    snapshot_objs: RawQueryBuilder,
    history_objs: RawQueryBuilder,
    lhs: i64,
    rhs: i64,
) -> RawQueryBuilder {
    let candidates = build_candidates(snapshot_objs, history_objs);
    let newer = build_newer(lhs, rhs);
    build_join(candidates, newer)
}
