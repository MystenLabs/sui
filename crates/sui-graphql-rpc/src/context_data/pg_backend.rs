// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    db_backend::{BalanceQuery, Explain, Explained, GenericQueryBuilder},
    db_data_provider::{DbValidationError, PageLimit, TypeFilterError},
};
use crate::{
    context_data::db_data_provider::PgManager,
    error::Error,
    types::{object::DeprecatedObjectFilter, sui_address::SuiAddress},
};
use async_trait::async_trait;
use diesel::{
    pg::Pg,
    query_builder::{AstPass, QueryFragment},
    BoolExpressionMethods, ExpressionMethods, PgConnection, QueryDsl, QueryResult, RunQueryDsl,
    TextExpressionMethods,
};
use std::str::FromStr;
use sui_indexer::{
    schema::{display, objects},
    types::OwnerType,
};
use sui_types::parse_sui_struct_tag;
use tap::TapFallible;
use tracing::{info, warn};

pub(crate) const EXPLAIN_COSTING_LOG_TARGET: &str = "gql-explain-costing";

pub(crate) struct PgQueryBuilder;

impl GenericQueryBuilder<Pg> for PgQueryBuilder {
    fn get_obj(address: Vec<u8>, version: Option<i64>) -> objects::BoxedQuery<'static, Pg> {
        let mut query = objects::dsl::objects.into_boxed();
        query = query.filter(objects::dsl::object_id.eq(address));

        if let Some(version) = version {
            query = query.filter(objects::dsl::object_version.eq(version));
        }
        query
    }
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

    fn multi_get_coins(
        before: Option<Vec<u8>>,
        after: Option<Vec<u8>>,
        limit: PageLimit,
        address: Option<Vec<u8>>,
        coin_type: String,
    ) -> objects::BoxedQuery<'static, Pg> {
        let mut query = order_objs(before, after, &limit);
        query = query.limit(limit.value() + 1);

        if let Some(address) = address {
            query = query
                .filter(objects::dsl::owner_id.eq(address))
                // Leverage index on objects table
                .filter(objects::dsl::owner_type.eq(OwnerType::Address as i16));
        }
        query = query.filter(objects::dsl::coin_type.eq(coin_type));

        query
    }
    fn multi_get_objs(
        before: Option<Vec<u8>>,
        after: Option<Vec<u8>>,
        limit: PageLimit,
        filter: Option<DeprecatedObjectFilter>,
        owner_type: Option<OwnerType>,
    ) -> Result<objects::BoxedQuery<'static, Pg>, Error> {
        let mut query = order_objs(before, after, &limit);
        query = query.limit(limit.value() + 1);

        let Some(filter) = filter else {
            return Ok(query);
        };

        if let Some(object_ids) = filter.object_ids {
            query = query.filter(
                objects::dsl::object_id.eq_any(
                    object_ids
                        .into_iter()
                        .map(|id| id.into_vec())
                        .collect::<Vec<_>>(),
                ),
            );
        }

        if let Some(owner) = filter.owner {
            query = query.filter(objects::dsl::owner_id.eq(owner.into_vec()));

            match owner_type {
                Some(OwnerType::Address) => {
                    query = query.filter(objects::dsl::owner_type.eq(OwnerType::Address as i16));
                }
                Some(OwnerType::Object) => {
                    query = query.filter(objects::dsl::owner_type.eq(OwnerType::Object as i16));
                }
                None => {
                    query = query.filter(
                        objects::dsl::owner_type
                            .eq(OwnerType::Address as i16)
                            .or(objects::dsl::owner_type.eq(OwnerType::Object as i16)),
                    );
                }
                _ => Err(DbValidationError::InvalidOwnerType)?,
            }
        }

        if let Some(object_type) = filter.type_ {
            let format = "package[::module[::type[<type_params>]]]";
            let parts: Vec<_> = object_type.splitn(3, "::").collect();

            if parts.iter().any(|&part| part.is_empty()) {
                return Err(DbValidationError::InvalidType(
                    TypeFilterError::MissingComponents(object_type, format).to_string(),
                ))?;
            }

            if parts.len() == 1 {
                // We check for a leading 0x to determine if it is an address
                // And otherwise process it as a primitive type
                if parts[0].starts_with("0x") {
                    let package = SuiAddress::from_str(parts[0])
                        .map_err(|e| DbValidationError::InvalidType(e.to_string()))?;
                    query = query.filter(objects::dsl::object_type.like(format!("{}::%", package)));
                } else {
                    query = query.filter(objects::dsl::object_type.eq(parts[0].to_string()));
                }
            } else if parts.len() == 2 {
                // Only package addresses are allowed if there are two or more parts
                let package = SuiAddress::from_str(parts[0])
                    .map_err(|e| DbValidationError::InvalidType(e.to_string()))?;
                query = query.filter(
                    objects::dsl::object_type.like(format!("{}::{}::%", package, parts[1])),
                );
            } else if parts.len() == 3 {
                let validated_type = parse_sui_struct_tag(&object_type)
                    .map_err(|e| DbValidationError::InvalidType(e.to_string()))?;

                if validated_type.type_params.is_empty() {
                    query = query.filter(
                        objects::dsl::object_type
                            .like(format!(
                                "{}<%",
                                validated_type.to_canonical_string(/* with_prefix */ true)
                            ))
                            .or(objects::dsl::object_type
                                .eq(validated_type.to_canonical_string(/* with_prefix */ true))),
                    );
                } else {
                    query = query.filter(
                        objects::dsl::object_type
                            .eq(validated_type.to_canonical_string(/* with_prefix */ true)),
                    );
                }
            } else {
                return Err(DbValidationError::InvalidType(
                    TypeFilterError::TooManyComponents(object_type, 3, format).to_string(),
                )
                .into());
            }
        }

        Ok(query)
    }
    fn multi_get_balances(address: Vec<u8>) -> BalanceQuery<'static, Pg> {
        let query = objects::dsl::objects
            .group_by(objects::dsl::coin_type)
            .select((
                diesel::dsl::sql::<diesel::sql_types::Nullable<diesel::sql_types::BigInt>>(
                    "CAST(SUM(coin_balance) AS BIGINT)",
                ),
                diesel::dsl::sql::<diesel::sql_types::Nullable<diesel::sql_types::BigInt>>(
                    "COUNT(*)",
                ),
                objects::dsl::coin_type,
            ))
            .filter(objects::dsl::owner_id.eq(address))
            .filter(objects::dsl::owner_type.eq(OwnerType::Address as i16))
            .filter(objects::dsl::coin_type.is_not_null())
            .into_boxed();

        query
    }
    fn get_balance(address: Vec<u8>, coin_type: String) -> BalanceQuery<'static, Pg> {
        let query = PgQueryBuilder::multi_get_balances(address);
        query.filter(objects::dsl::coin_type.eq(coin_type))
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
                            info!(target: EXPLAIN_COSTING_LOG_TARGET, cost,);
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

fn order_objs(
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
    limit: &PageLimit,
) -> objects::BoxedQuery<'static, Pg> {
    let mut query = objects::dsl::objects.into_boxed();
    match limit {
        PageLimit::First(_) => {
            if let Some(after) = after {
                query = query.filter(objects::dsl::object_id.gt(after));
            }
            query = query.order(objects::dsl::object_id.asc());
        }
        PageLimit::Last(_) => {
            if let Some(before) = before {
                query = query.filter(objects::dsl::object_id.lt(before));
            }
            query = query.order(objects::dsl::object_id.desc());
        }
    }
    query
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
