// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{
    query_builder::{BoxedSqlQuery, SqlQuery},
    sql_query,
};

use crate::data::DieselBackend;

pub(crate) type RawSqlQuery = BoxedSqlQuery<'static, DieselBackend, SqlQuery>;

/// `RawQuery` is a utility for building and managing `diesel::query_builder::BoxedSqlQuery` queries
/// dynamically.
///
/// 1. **Dynamic Value Binding**: Allows binding string values dynamically to the query, bypassing
///    the need to specify types explicitly, as is typically required with Diesel's
///    `sql_query.bind`.
///
/// 2. **Query String Merging**: Can be used to represent and merge query strings and their
///    associated bindings. Placeholder strings and bindings are applied in sequential order.
///
/// Note: `RawQuery` only supports binding string values, as interpolating raw strings directly
/// increases exposure to SQL injection attacks.
#[derive(Clone)]
pub(crate) struct RawQuery {
    /// The `SELECT` and `FROM` clauses of the query.
    select: String,
    /// The `WHERE` clause of the query.
    where_: Option<String>,
    /// The `ORDER BY` clause of the query.
    order_by: Vec<String>,
    /// The `GROUP BY` clause of the query.
    group_by: Vec<String>,
    /// The `LIMIT` clause of the query.
    limit: Option<i64>,
    /// The list of string binds for this query.
    binds: Vec<String>,
}

impl RawQuery {
    /// Constructs a new `RawQuery` with the given `SELECT` clause and binds.
    pub(crate) fn new(select: impl Into<String>, binds: Vec<String>) -> Self {
        Self {
            select: select.into(),
            where_: None,
            order_by: Vec::new(),
            group_by: Vec::new(),
            limit: None,
            binds,
        }
    }

    /// Adds a `WHERE` condition to the query, combining it with existing conditions using `AND`.
    pub(crate) fn filter<T: std::fmt::Display>(mut self, condition: T) -> Self {
        self.where_ = match self.where_ {
            Some(where_) => Some(format!("({}) AND {}", where_, condition)),
            None => Some(condition.to_string()),
        };

        self
    }

    /// Adds a `WHERE` condition to the query, combining it with existing conditions using `OR`.
    #[allow(dead_code)]
    pub(crate) fn or_filter<T: std::fmt::Display>(mut self, condition: T) -> Self {
        self.where_ = match self.where_ {
            Some(where_) => Some(format!("({}) OR {}", where_, condition)),
            None => Some(condition.to_string()),
        };

        self
    }

    /// Adds an `ORDER BY` clause to the query.
    pub(crate) fn order_by<T: ToString>(mut self, order: T) -> Self {
        self.order_by.push(order.to_string());
        self
    }

    /// Adds a `GROUP BY` clause to the query.
    pub(crate) fn group_by<T: ToString>(mut self, group: T) -> Self {
        self.group_by.push(group.to_string());
        self
    }

    /// Adds a `LIMIT` clause to the query.
    pub(crate) fn limit(mut self, limit: i64) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Adds the `String` value to the list of binds for this query.
    pub(crate) fn bind_value(&mut self, condition: String) {
        self.binds.push(condition);
    }

    /// Constructs the query string and returns it along with the list of binds for this query. This
    /// function is not intended to be called directly, and instead should be used through the
    /// `query!` macro.
    pub(crate) fn finish(self) -> (String, Vec<String>) {
        let mut select = self.select;

        if let Some(where_) = self.where_ {
            select.push_str(" WHERE ");
            select.push_str(&where_);
        }

        let mut prefix = " GROUP BY ";
        for group in self.group_by.iter() {
            select.push_str(prefix);
            select.push_str(group);
            prefix = ", ";
        }

        let mut prefix = " ORDER BY ";
        for order in self.order_by.iter() {
            select.push_str(prefix);
            select.push_str(order);
            prefix = ", ";
        }

        if let Some(limit) = self.limit {
            select.push_str(" LIMIT ");
            select.push_str(&limit.to_string());
        }

        (select, self.binds)
    }

    /// Converts this `RawQuery` into a `diesel::query_builder::BoxedSqlQuery`. Consumes `self` into
    /// a raw sql string and bindings, if any. A `BoxedSqlQuery` is constructed from the raw sql
    /// string, and bindings are added using `sql_query.bind()`.
    pub(crate) fn into_boxed(self) -> RawSqlQuery {
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

        let mut diesel_query = sql_query(result).into_boxed();

        for bind in binds {
            diesel_query = diesel_query.bind::<diesel::sql_types::Text, _>(bind);
        }

        diesel_query
    }
}

/// Applies the `AND` condition to the given `RawQuery` and binds input string values, if any.
#[macro_export]
macro_rules! filter {
    ($query:expr, $condition:expr $(,$binds:expr)*) => {{
        let mut query = $query;
        query = query.filter($condition);
        $(query.bind_value($binds.to_string());)*
        query
    }};
}

/// Applies the `OR` condition to the given `RawQuery` and binds input string values, if any.
#[macro_export]
macro_rules! or_filter {
    ($query:expr, $condition:expr $(,$binds:expr)*) => {{
        let mut query = $query;
        query = query.or_filter($condition);
        $(query.bind_value($binds.to_string());)*
        query
    }};
}

/// Accepts two `RawQuery` instances and a third expression consisting of which columns to join on.
#[macro_export]
macro_rules! inner_join {
    ($lhs:expr, $alias:expr => $rhs_query:expr, using: [$using:expr $(, $more_using:expr)*]) => {{
        use $crate::raw_query::RawQuery;

        let (lhs_sql, mut binds) = $lhs.finish();
        let (rhs_sql, rhs_binds) = $rhs_query.finish();

        binds.extend(rhs_binds);

        let sql = format!(
            "{lhs_sql} INNER JOIN ({rhs_sql}) AS {} USING ({})",
            $alias,
            stringify!($using $(, $more_using)*),
        );

        RawQuery::new(sql, binds)
    }};
}

/// Accepts a `SELECT FROM` format string and optional subqueries. If subqueries are provided, there
/// should be curly braces `{}` in the format string to interpolate each subquery's sql string into.
/// Concatenates subqueries to the `SELECT FROM` clause, and creates a new `RawQuery` from the
/// concatenated sql string. The binds from each subquery are added in the order they appear in the
/// macro parameter. Subqueries are consumed into the new `RawQuery`.
#[macro_export]
macro_rules! query {
    // Matches the case where no subqueries are provided. A `RawQuery` is constructed from the given
    // select clause.
    ($select:expr) => {
        $crate::raw_query::RawQuery::new($select, vec![])
    };

    // Expects a select clause and one or more subqueries. The select clause should contain curly
    // braces for subqueries to be interpolated into. Use when the subqueries can be aliased
    // directly in the select statement.
    ($select:expr $(,$subquery:expr)+) => {{
        use $crate::raw_query::RawQuery;
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
