// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::data::{Db, DbConnection, DieselBackend, DieselConn, QueryExecutor};
use crate::error::Error;
use diesel::query_builder::{AstPass, Query, QueryFragment, QueryId};
use diesel::sql_types::Bool;
use diesel::{QueryDsl, QueryResult, RunQueryDsl};

/// Generates a function: `check_all_tables` that runs a query against every table this GraphQL
/// service is aware of, to test for schema compatibility. Each query is of the form:
///
///   SELECT TRUE FROM (...) q WHERE FALSE
///
/// where `...` is a query selecting all of the fields from the given table. The query is expected
/// to return no results, but will complain if it relies on a column that doesn't exist.
macro_rules! generate_compatibility_check {
    ($($table:ident),*) => {
        pub(crate) async fn check_all_tables(db: &Db) -> Result<(), Error> {
            use futures::future::join_all;
            use sui_indexer::schema::*;

            let futures = vec![
                $(
                    db.execute(|conn| Ok::<_, diesel::result::Error>(
                        conn.results::<_, bool>(move || Check {
                            query: $table::table.select($table::all_columns)
                        })
                        .is_ok()
                    ))
                ),*
            ];

            let results = join_all(futures).await;
            if results.into_iter().all(|res| res.unwrap_or(false)) {
                Ok(())
            } else {
                Err(Error::Internal(
                    "One or more tables are missing expected columns".into(),
                ))
            }
        }
    };
}

sui_indexer::for_all_tables!(generate_compatibility_check);

#[derive(Debug, Clone, Copy, QueryId)]
struct Check<Q> {
    query: Q,
}

impl<Q: Query> Query for Check<Q> {
    type SqlType = Bool;
}

impl<Q> RunQueryDsl<DieselConn> for Check<Q> {}

impl<Q: QueryFragment<DieselBackend>> QueryFragment<DieselBackend> for Check<Q> {
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, DieselBackend>) -> QueryResult<()> {
        out.push_sql("SELECT TRUE FROM (");
        self.query.walk_ast(out.reborrow())?;
        out.push_sql(") q WHERE FALSE");
        Ok(())
    }
}
