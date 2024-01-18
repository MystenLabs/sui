// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::db_backend::{Explain, Explained, GenericQueryBuilder};
use crate::{context_data::db_data_provider::PgManager, error::Error};
use async_trait::async_trait;
use diesel::{
    pg::Pg,
    query_builder::{AstPass, QueryFragment},
    ExpressionMethods, PgConnection, QueryDsl, QueryResult, RunQueryDsl,
};
use sui_indexer::schema_v2::{display, objects};
use tap::TapFallible;
use tracing::{info, warn};

pub(crate) const EXPLAIN_COSTING_LOG_TARGET: &str = "gql-explain-costing";

pub(crate) struct PgQueryBuilder;

impl GenericQueryBuilder<Pg> for PgQueryBuilder {
    fn get_obj_by_type(object_type: String) -> objects::BoxedQuery<'static, Pg> {
        objects::dsl::objects
            .filter(objects::dsl::object_type.eq(object_type))
            .limit(1) // Fetches for a single object and as such has a limit of 1
            .into_boxed()
    }

    fn get_display_by_obj_type(object_type: String) -> display::BoxedQuery<'static, Pg> {
        display::dsl::display
            .filter(display::dsl::object_type.eq(object_type))
            .limit(1)
            .into_boxed()
    }
}

/// Allows methods like load(), get_result(), etc. on an Explained query
impl<T> RunQueryDsl<PgConnection> for Explained<T> {}

/// Implement logic for prefixing queries with "EXPLAIN"
impl<T> QueryFragment<Pg> for Explained<T>
where
    T: QueryFragment<Pg>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, Pg>) -> QueryResult<()> {
        out.push_sql("EXPLAIN (FORMAT JSON) ");
        self.query.walk_ast(out.reborrow())?;
        Ok(())
    }
}

#[async_trait]
pub trait PgQueryExecutor {
    async fn run_query_async<T, E, F>(&self, query: F) -> Result<T, Error>
    where
        F: FnOnce(&mut PgConnection) -> Result<T, E> + Send + 'static,
        E: From<diesel::result::Error> + std::error::Error + Send + 'static,
        T: Send + 'static;

    async fn run_query_async_with_cost<T, Q, QResult, EF, E, F>(
        &self,
        mut query_builder_fn: Q,
        execute_fn: EF,
    ) -> Result<T, Error>
    where
        Q: FnMut() -> Result<QResult, Error> + Send + 'static,
        QResult: diesel::query_builder::QueryFragment<diesel::pg::Pg>
            + diesel::query_builder::Query
            + diesel::query_builder::QueryId
            + Send
            + 'static,
        EF: FnOnce(QResult) -> F + Send + 'static,
        F: FnOnce(&mut PgConnection) -> Result<T, E> + Send + 'static,
        E: From<diesel::result::Error> + std::error::Error + Send + 'static,
        T: Send + 'static;
}

#[async_trait]
impl PgQueryExecutor for PgManager {
    async fn run_query_async<T, E, F>(&self, query: F) -> Result<T, Error>
    where
        F: FnOnce(&mut PgConnection) -> Result<T, E> + Send + 'static,
        E: From<diesel::result::Error> + std::error::Error + Send + 'static,
        T: Send + 'static,
    {
        self.inner
            .run_query_async(query)
            .await
            .map_err(|e| Error::Internal(e.to_string()))
    }

    /// Takes a query_builder_fn that returns Result<QueryFragment> and a lambda to execute the query
    /// Spawns a blocking task that determines the cost of the query fragment
    /// And if within limits, then executes the query
    async fn run_query_async_with_cost<T, Q, QResult, EF, E, F>(
        &self,
        mut query_builder_fn: Q,
        execute_fn: EF,
    ) -> Result<T, Error>
    where
        Q: FnMut() -> Result<QResult, Error> + Send + 'static,
        QResult: diesel::query_builder::QueryFragment<diesel::pg::Pg>
            + diesel::query_builder::Query
            + diesel::query_builder::QueryId
            + Send
            + 'static,
        EF: FnOnce(QResult) -> F + Send + 'static,
        F: FnOnce(&mut PgConnection) -> Result<T, E> + Send + 'static,
        E: From<diesel::result::Error> + std::error::Error + Send + 'static,
        T: Send + 'static,
    {
        let max_db_query_cost = self.limits.max_db_query_cost;
        self.inner
            .spawn_blocking(move |this| {
                let query = query_builder_fn()?;
                let explain_result: Option<String> = this
                    .run_query(|conn| query.explain().get_result(conn))
                    .tap_err(|e| {
                        warn!(
                            target: EXPLAIN_COSTING_LOG_TARGET,
                            "Failed to get explain result: {}", e
                        )
                    })
                    .ok(); // Fine to not propagate this error as explain-based costing is not critical today

                if let Some(explain_result) = explain_result {
                    let cost = extract_cost(&explain_result)
                        .tap_err(|e| {
                            warn!(
                                target: EXPLAIN_COSTING_LOG_TARGET,
                                "Failed to get cost from explain result: {}", e
                            )
                        })
                        .ok(); // Fine to not propagate this error as explain-based costing is not critical today

                    if let Some(cost) = cost {
                        if cost > max_db_query_cost as f64 {
                            warn!(
                                target: EXPLAIN_COSTING_LOG_TARGET,
                                cost,
                                max_db_query_cost,
                                exceeds = true
                            );
                        } else {
                            info!(
                                target: EXPLAIN_COSTING_LOG_TARGET,
                                cost,
                            );
                        }
                    }
                }

                let query = query_builder_fn()?;
                let execute_closure = execute_fn(query);
                this.run_query(execute_closure)
                    .map_err(|e| Error::Internal(e.to_string()))
            })
            .await
    }
}

pub fn extract_cost(explain_result: &str) -> Result<f64, Error> {
    let parsed: serde_json::Value =
        serde_json::from_str(explain_result).map_err(|e| Error::Internal(e.to_string()))?;
    if let Some(cost) = parsed
        .get(0)
        .and_then(|entry| entry.get("Plan"))
        .and_then(|plan| plan.get("Total Cost"))
        .and_then(|cost| cost.as_f64())
    {
        Ok(cost)
    } else {
        Err(Error::Internal(
            "Failed to get cost from query plan".to_string(),
        ))
    }
}

pub(crate) type QueryBuilder = PgQueryBuilder;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_json() {
        let explain_result = "invalid json";
        let result = extract_cost(explain_result);
        assert!(matches!(result, Err(Error::Internal(_))));
    }

    #[test]
    fn test_missing_entry_at_0() {
        let explain_result = "[]";
        let result = extract_cost(explain_result);
        assert!(matches!(result, Err(Error::Internal(_))));
    }

    #[test]
    fn test_missing_plan() {
        let explain_result = r#"[{}]"#;
        let result = extract_cost(explain_result);
        assert!(matches!(result, Err(Error::Internal(_))));
    }

    #[test]
    fn test_missing_total_cost() {
        let explain_result = r#"[{"Plan": {}}]"#;
        let result = extract_cost(explain_result);
        assert!(matches!(result, Err(Error::Internal(_))));
    }

    #[test]
    fn test_failure_on_conversion_to_f64() {
        let explain_result = r#"[{"Plan": {"Total Cost": "string_instead_of_float"}}]"#;
        let result = extract_cost(explain_result);
        assert!(matches!(result, Err(Error::Internal(_))));
    }

    #[test]
    fn test_happy_scenario() {
        let explain_result = r#"[{"Plan": {"Total Cost": 1.0}}]"#;
        let result = extract_cost(explain_result).unwrap();
        assert_eq!(result, 1.0);
    }
}
