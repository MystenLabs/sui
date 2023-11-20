// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    db_backend::{
        BalanceQuery, Cursor, Explain, Explained, GenericQueryBuilder, QueryDirection, SortOrder,
        Subqueried, Subquery,
    },
    db_data_provider::DbValidationError,
};
use crate::context_data::db_data_provider::PgManager;
use crate::{
    error::Error,
    types::{digest::Digest, object::ObjectFilter, transaction_block::TransactionBlockFilter},
};
use async_trait::async_trait;
use diesel::{
    associations::HasTable,
    helper_types::IntoBoxed,
    pg::Pg,
    query_builder::{
        AsQuery, AstPass, BoxedSelectStatement, FromClause, Query, QueryFragment, SelectStatement,
    },
    query_dsl::methods::BoxedDsl,
    BoolExpressionMethods, Expression, ExpressionMethods, PgConnection, QueryDsl, QueryResult,
    QuerySource, RunQueryDsl, Table,
};
use std::str::FromStr;
use sui_indexer::{
    schema_v2::{
        checkpoints, epochs, objects, transactions, tx_calls, tx_changed_objects, tx_input_objects,
        tx_recipients, tx_senders,
    },
    types_v2::OwnerType,
};

pub(crate) struct PgQueryBuilder;

impl PgQueryBuilder {
    fn handle_checkpoint_pagination_first(
        cursor: i64,
        cursor_type: Cursor,
        sort_order: SortOrder,
    ) -> checkpoints::BoxedQuery<'static, Pg> {
        let mut query = checkpoints::dsl::checkpoints.into_boxed();

        match (cursor_type, sort_order) {
            (Cursor::After, SortOrder::Asc) => {
                query = query
                    .filter(checkpoints::dsl::sequence_number.gt(cursor))
                    .order(checkpoints::dsl::sequence_number.asc());
            }
            (Cursor::Before, SortOrder::Asc) => {
                query = query
                    .filter(checkpoints::dsl::sequence_number.lt(cursor))
                    .order(checkpoints::dsl::sequence_number.asc());
            }
            (Cursor::After, SortOrder::Desc) => {
                query = query
                    .filter(checkpoints::dsl::sequence_number.lt(cursor))
                    .order(checkpoints::dsl::sequence_number.desc());
            }
            (Cursor::Before, SortOrder::Desc) => {
                query = query
                    .filter(checkpoints::dsl::sequence_number.gt(cursor))
                    .order(checkpoints::dsl::sequence_number.desc());
            }
        }
        query
    }

    fn handle_checkpoint_pagination_last(
        cursor: i64,
        cursor_type: Cursor,
        sort_order: SortOrder,
    ) -> checkpoints::BoxedQuery<'static, Pg> {
        let subquery = match (cursor_type, sort_order) {
            (Cursor::After, SortOrder::Asc) => PgQueryBuilder::handle_checkpoint_pagination_first(
                cursor,
                Cursor::Before,
                SortOrder::Desc,
            ), // outer asc
            (Cursor::Before, SortOrder::Asc) => PgQueryBuilder::handle_checkpoint_pagination_first(
                cursor,
                Cursor::After,
                SortOrder::Desc,
            ), // outer asc
            (Cursor::After, SortOrder::Desc) => PgQueryBuilder::handle_checkpoint_pagination_first(
                cursor,
                Cursor::Before,
                SortOrder::Asc,
            ), // outer desc
            (Cursor::Before, SortOrder::Desc) => {
                PgQueryBuilder::handle_checkpoint_pagination_first(
                    cursor,
                    Cursor::After,
                    SortOrder::Asc,
                )
            } // outer desc
        };

        match sort_order {
            SortOrder::Asc => subquery
                .subquery()
                .order(checkpoints::dsl::sequence_number.asc()),
            SortOrder::Desc => subquery
                .subquery()
                .order(checkpoints::dsl::sequence_number.desc()),
        }
    }
}

impl GenericQueryBuilder<Pg> for PgQueryBuilder {
    fn get_tx_by_digest(digest: Vec<u8>) -> transactions::BoxedQuery<'static, Pg> {
        transactions::dsl::transactions
            .filter(transactions::dsl::transaction_digest.eq(digest))
            .into_boxed()
    }
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
    fn get_epoch(epoch_id: i64) -> epochs::BoxedQuery<'static, Pg> {
        epochs::dsl::epochs
            .filter(epochs::dsl::epoch.eq(epoch_id))
            .into_boxed()
    }
    fn get_latest_epoch() -> epochs::BoxedQuery<'static, Pg> {
        epochs::dsl::epochs
            .order_by(epochs::dsl::epoch.desc())
            .limit(1)
            .into_boxed()
    }
    fn get_checkpoint_by_digest(digest: Vec<u8>) -> checkpoints::BoxedQuery<'static, Pg> {
        checkpoints::dsl::checkpoints
            .filter(checkpoints::dsl::checkpoint_digest.eq(digest))
            .into_boxed()
    }
    fn get_checkpoint_by_sequence_number(
        sequence_number: i64,
    ) -> checkpoints::BoxedQuery<'static, Pg> {
        checkpoints::dsl::checkpoints
            .filter(checkpoints::dsl::sequence_number.eq(sequence_number))
            .into_boxed()
    }
    fn get_latest_checkpoint() -> checkpoints::BoxedQuery<'static, Pg> {
        checkpoints::dsl::checkpoints
            .order_by(checkpoints::dsl::sequence_number.desc())
            .limit(1)
            .into_boxed()
    }
    fn multi_get_txs(
        cursor: Option<i64>,
        descending_order: bool,
        limit: i64,
        filter: Option<TransactionBlockFilter>,
        after_tx_seq_num: Option<i64>,
        before_tx_seq_num: Option<i64>,
    ) -> Result<transactions::BoxedQuery<'static, Pg>, Error> {
        let mut query = transactions::dsl::transactions.into_boxed();

        if let Some(cursor_val) = cursor {
            if descending_order {
                let filter_value =
                    before_tx_seq_num.map_or(cursor_val, |b| std::cmp::min(b, cursor_val));
                query = query.filter(transactions::dsl::tx_sequence_number.lt(filter_value));
            } else {
                let filter_value =
                    after_tx_seq_num.map_or(cursor_val, |a| std::cmp::max(a, cursor_val));
                query = query.filter(transactions::dsl::tx_sequence_number.gt(filter_value));
            }
        } else {
            if let Some(av) = after_tx_seq_num {
                query = query.filter(transactions::dsl::tx_sequence_number.gt(av));
            }
            if let Some(bv) = before_tx_seq_num {
                query = query.filter(transactions::dsl::tx_sequence_number.lt(bv));
            }
        }

        if descending_order {
            query = query.order(transactions::dsl::tx_sequence_number.desc());
        } else {
            query = query.order(transactions::dsl::tx_sequence_number.asc());
        }

        query = query.limit(limit + 1);

        if let Some(filter) = filter {
            // Filters for transaction table
            // at_checkpoint mutually exclusive with before_ and after_checkpoint
            if let Some(checkpoint) = filter.at_checkpoint {
                query = query
                    .filter(transactions::dsl::checkpoint_sequence_number.eq(checkpoint as i64));
            }
            if let Some(transaction_ids) = filter.transaction_ids {
                let digests = transaction_ids
                    .into_iter()
                    .map(|id| Ok::<Vec<u8>, Error>(Digest::from_str(&id)?.into_vec()))
                    .collect::<Result<Vec<_>, _>>()?;
                query = query.filter(transactions::dsl::transaction_digest.eq_any(digests));
            }

            // Queries on foreign tables
            match (filter.package, filter.module, filter.function) {
                (Some(p), None, None) => {
                    let subquery = tx_calls::dsl::tx_calls
                        .filter(tx_calls::dsl::package.eq(p.into_vec()))
                        .select(tx_calls::dsl::tx_sequence_number);

                    query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
                }
                (Some(p), Some(m), None) => {
                    let subquery = tx_calls::dsl::tx_calls
                        .filter(tx_calls::dsl::package.eq(p.into_vec()))
                        .filter(tx_calls::dsl::module.eq(m))
                        .select(tx_calls::dsl::tx_sequence_number);

                    query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
                }
                (Some(p), Some(m), Some(f)) => {
                    let subquery = tx_calls::dsl::tx_calls
                        .filter(tx_calls::dsl::package.eq(p.into_vec()))
                        .filter(tx_calls::dsl::module.eq(m))
                        .filter(tx_calls::dsl::func.eq(f))
                        .select(tx_calls::dsl::tx_sequence_number);

                    query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
                }
                _ => {}
            }

            if let Some(signer) = filter.sign_address {
                if let Some(sender) = filter.sent_address {
                    let subquery = tx_senders::dsl::tx_senders
                        .filter(
                            tx_senders::dsl::sender
                                .eq(signer.into_vec())
                                .or(tx_senders::dsl::sender.eq(sender.into_vec())),
                        )
                        .select(tx_senders::dsl::tx_sequence_number);

                    query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
                } else {
                    let subquery = tx_senders::dsl::tx_senders
                        .filter(tx_senders::dsl::sender.eq(signer.into_vec()))
                        .select(tx_senders::dsl::tx_sequence_number);

                    query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
                }
            } else if let Some(sender) = filter.sent_address {
                let subquery = tx_senders::dsl::tx_senders
                    .filter(tx_senders::dsl::sender.eq(sender.into_vec()))
                    .select(tx_senders::dsl::tx_sequence_number);

                query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
            }
            if let Some(recipient) = filter.recv_address {
                let subquery = tx_recipients::dsl::tx_recipients
                    .filter(tx_recipients::dsl::recipient.eq(recipient.into_vec()))
                    .select(tx_recipients::dsl::tx_sequence_number);

                query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
            }
            if filter.paid_address.is_some() {
                return Err(Error::Internal(
                    "Paid address filter not supported".to_string(),
                ));
            }

            if let Some(input_object) = filter.input_object {
                let subquery = tx_input_objects::dsl::tx_input_objects
                    .filter(tx_input_objects::dsl::object_id.eq(input_object.into_vec()))
                    .select(tx_input_objects::dsl::tx_sequence_number);

                query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
            }
            if let Some(changed_object) = filter.changed_object {
                let subquery = tx_changed_objects::dsl::tx_changed_objects
                    .filter(tx_changed_objects::dsl::object_id.eq(changed_object.into_vec()))
                    .select(tx_changed_objects::dsl::tx_sequence_number);

                query = query.filter(transactions::dsl::tx_sequence_number.eq_any(subquery));
            }
        };

        Ok(query)
    }
    fn multi_get_coins(
        cursor: Option<Vec<u8>>,
        descending_order: bool,
        limit: i64,
        address: Option<Vec<u8>>,
        coin_type: String,
    ) -> objects::BoxedQuery<'static, Pg> {
        let mut query = objects::dsl::objects.into_boxed();
        if let Some(cursor) = cursor {
            if descending_order {
                query = query.filter(objects::dsl::object_id.lt(cursor));
            } else {
                query = query.filter(objects::dsl::object_id.gt(cursor));
            }
        }
        if descending_order {
            query = query.order(objects::dsl::object_id.desc());
        } else {
            query = query.order(objects::dsl::object_id.asc());
        }
        query = query.limit(limit + 1);

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
        cursor: Option<Vec<u8>>,
        descending_order: bool,
        limit: i64,
        filter: Option<ObjectFilter>,
        owner_type: Option<OwnerType>,
    ) -> Result<objects::BoxedQuery<'static, Pg>, Error> {
        let mut query = objects::dsl::objects.into_boxed();

        if let Some(cursor) = cursor {
            if descending_order {
                query = query.filter(objects::dsl::object_id.lt(cursor));
            } else {
                query = query.filter(objects::dsl::object_id.gt(cursor));
            }
        }

        if descending_order {
            query = query.order(objects::dsl::object_id.desc());
        } else {
            query = query.order(objects::dsl::object_id.asc());
        }

        query = query.limit(limit + 1);

        if let Some(filter) = filter {
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
                        query =
                            query.filter(objects::dsl::owner_type.eq(OwnerType::Address as i16));
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

            if let Some(object_type) = filter.ty {
                query = query.filter(objects::dsl::object_type.eq(object_type));
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
    fn multi_get_checkpoints(
        cursor: Option<i64>,
        cursor_type: Option<Cursor>,
        limit: i64,
        sort_order: SortOrder,
        query_direction: QueryDirection,
        epoch: Option<i64>,
    ) -> checkpoints::BoxedQuery<'static, Pg> {
        let mut query = checkpoints::dsl::checkpoints.into_boxed();

        if let (Some(cursor), Some(cursor_type)) = (cursor, cursor_type) {
            query = match query_direction {
                QueryDirection::First(_) => PgQueryBuilder::handle_checkpoint_pagination_first(
                    cursor,
                    cursor_type,
                    sort_order,
                ),
                QueryDirection::Last(_) => PgQueryBuilder::handle_checkpoint_pagination_last(
                    cursor,
                    cursor_type,
                    sort_order,
                ),
            };
        } else {
            query = match sort_order {
                SortOrder::Asc => query.order(checkpoints::dsl::sequence_number.asc()),
                SortOrder::Desc => query.order(checkpoints::dsl::sequence_number.desc()),
            };
        }

        query = query.limit(limit + 1);

        query
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

/// Allows methods like load(), get_result(), etc. on a Subqueried query
impl<T> RunQueryDsl<PgConnection> for Subqueried<T> {}

/// Implement logic wrapping queries in a SELECT * FROM (query) AS SUB
impl<T> QueryFragment<Pg> for Subqueried<T>
where
    T: QueryFragment<Pg>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, Pg>) -> QueryResult<()> {
        out.push_sql("SELECT * FROM (");
        self.query.walk_ast(out.reborrow())?;
        out.push_sql(") AS sub");
        Ok(())
    }
}

// impl<T: Query> AsQuery for Subqueried<T> {
//     type SqlType = <T as Query>::SqlType;
//     type Query = T;

//     fn as_query(self) -> <T as AsQuery>::Query {
//         self
//     }
// }

impl<T> BoxedDsl<'_, Pg> for Subqueried<T>
where
    T: Table + HasTable<Table = T> + AsQuery + QueryFragment<Pg> + 'static,
{
    type Output = BoxedSelectStatement<'static, T::SqlType, Self, Pg>;

    fn internal_into_boxed(self) -> Self::Output {
        self.as_query().internal_into_boxed()
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
                let explain_result: String = this
                    .run_query(|conn| query.explain().get_result(conn))
                    .map_err(|e| Error::Internal(e.to_string()))?;
                let cost = extract_cost(&explain_result)?;
                if cost > max_db_query_cost as f64 {
                    return Err(DbValidationError::QueryCostExceeded(
                        cost as u64,
                        max_db_query_cost,
                    )
                    .into());
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
