// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{marker::PhantomData, ops};

use diesel::{
    pg::Pg,
    query_builder::{AstPass, QueryFragment, QueryId},
    serialize::ToSql,
    sql_types::{HasSqlType, Untyped},
    QueryResult,
};

/// A full SQL query constructed from snippets of raw SQL and bindings.
///
/// This abstraction is similar to [`diesel::query_builder::BoxedSqlQuery`], with the following
/// differences:
///
/// - Binds are specified inline, and the abstraction deals with inserting a bind parameter into
///   the SQL output (similar to how [`diesel::dsl::sql`] works).
///
/// - It is possible to embed one query into another.
///
/// - Queries can be built using the [`sui_sql_macro::query!`] macro, using format strings.
#[derive(Default)]
pub struct Query<'f> {
    parts: Vec<Part<'f>>,
}

enum Part<'f> {
    Sql(String),
    Bind(Box<dyn QueryFragment<Pg> + Send + 'f>),
}

struct Bind<ST, U> {
    value: U,
    _data: PhantomData<ST>,
}

impl<'f> Query<'f> {
    /// Construct a new query starting with the `sql` snippet.
    pub fn new(sql: impl AsRef<str>) -> Self {
        Self {
            parts: vec![Part::Sql(sql.as_ref().to_owned())],
        }
    }

    /// Append `query` at the end of `self`.
    pub fn query(mut self, query: Query<'f>) -> Self {
        self.parts.extend(query.parts);
        self
    }

    /// Add a raw `sql` snippet to the end of the query.
    pub fn sql(mut self, sql: impl AsRef<str>) -> Self {
        self.parts.push(Part::Sql(sql.as_ref().to_owned()));
        self
    }

    /// Embed `value` into the query as a bind parameter, at the end of the query.
    pub fn bind<ST, V>(mut self, value: V) -> Self
    where
        Pg: HasSqlType<ST>,
        V: ToSql<ST, Pg> + Send + 'f,
        ST: Send + 'f,
    {
        self.parts.push(Part::Bind(Box::new(Bind {
            value,
            _data: PhantomData,
        })));

        self
    }
}

impl QueryFragment<Pg> for Query<'_> {
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, Pg>) -> QueryResult<()> {
        for part in &self.parts {
            match part {
                Part::Sql(sql) => out.push_sql(sql),
                Part::Bind(bind) => bind.walk_ast(out.reborrow())?,
            }
        }

        Ok(())
    }
}

impl<ST, U> QueryFragment<Pg> for Bind<ST, U>
where
    Pg: HasSqlType<ST>,
    U: ToSql<ST, Pg>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, Pg>) -> QueryResult<()> {
        out.push_bind_param(&self.value)
    }
}

impl QueryId for Query<'_> {
    type QueryId = ();
    const HAS_STATIC_QUERY_ID: bool = false;
}

impl diesel::query_builder::Query for Query<'_> {
    type SqlType = Untyped;
}

impl ops::Add for Query<'_> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        self.query(rhs)
    }
}

impl ops::AddAssign for Query<'_> {
    fn add_assign(&mut self, rhs: Self) {
        self.parts.extend(rhs.parts);
    }
}

#[cfg(test)]
mod tests {
    use diesel::sql_types::{BigInt, Text};

    use super::*;

    /// A query without any binds.
    #[test]
    fn basic_query() {
        let q = Query::new("SELECT 1");
        assert_eq!(diesel::debug_query(&q).to_string(), "SELECT 1 -- binds: []");
    }

    /// A query that has been extended with more SQL.
    #[test]
    fn query_extended() {
        let q = Query::new("SELECT 1").sql(" WHERE id = 1");
        assert_eq!(
            diesel::debug_query(&q).to_string(),
            "SELECT 1 WHERE id = 1 -- binds: []"
        );
    }

    /// A query that has some binds.
    #[test]
    fn query_with_binds() {
        let q = Query::new("SELECT 1 WHERE ")
            .sql("id = ")
            .bind::<BigInt, _>(42)
            .sql(" AND name = ")
            .bind::<Text, _>("foo");

        assert_eq!(
            diesel::debug_query(&q).to_string(),
            "SELECT 1 WHERE id = $1 AND name = $2 -- binds: [42, \"foo\"]"
        );
    }

    /// A query that has other queries embedded into it.
    #[test]
    fn query_embedded() {
        let r = Query::new("cursor >= ").bind::<BigInt, _>(10);
        let s = Query::new("cursor <= ").bind::<BigInt, _>(20);
        let q = Query::new("SELECT 1 WHERE ")
            .sql("id = ")
            .bind::<BigInt, _>(42)
            .sql(" AND ")
            .query(r)
            .sql(" AND name = ")
            .bind::<Text, _>("foo")
            .sql(" AND ")
            .query(s);

        assert_eq!(
            diesel::debug_query(&q).to_string(),
            "SELECT 1 WHERE id = $1 AND cursor >= $2 AND name = $3 AND cursor <= $4 \
            -- binds: [42, 10, \"foo\", 20]"
        );
    }
}
