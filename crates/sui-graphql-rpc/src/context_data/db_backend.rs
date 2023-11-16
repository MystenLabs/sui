// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::backend::Backend;
use sui_indexer::{
    schema_v2::{checkpoints, epochs, objects, transactions},
    types_v2::OwnerType,
};

use crate::{
    error::Error,
    types::{object::ObjectFilter, transaction_block::TransactionBlockFilter},
};
use diesel::{
    pg::Pg,
    query_builder::{AstPass, BoxedSelectStatement, FromClause, QueryFragment, QueryId},
    sql_types::Text,
    PgConnection, QueryResult, RunQueryDsl,
};

pub(crate) type BalanceQuery<'a, DB> = BoxedSelectStatement<
    'a,
    (
        diesel::sql_types::Nullable<diesel::sql_types::BigInt>,
        diesel::sql_types::Nullable<diesel::sql_types::BigInt>,
        diesel::sql_types::Nullable<diesel::sql_types::Text>,
    ),
    FromClause<objects::table>,
    DB,
    objects::dsl::coin_type,
>;

pub(crate) trait GenericQueryBuilder<DB: Backend> {
    fn get_tx_by_digest(digest: Vec<u8>) -> transactions::BoxedQuery<'static, DB>;
    fn get_obj(address: Vec<u8>, version: Option<i64>) -> objects::BoxedQuery<'static, DB>;
    fn get_epoch(epoch_id: i64) -> epochs::BoxedQuery<'static, DB>;
    fn get_latest_epoch() -> epochs::BoxedQuery<'static, DB>;
    fn get_checkpoint_by_digest(digest: Vec<u8>) -> checkpoints::BoxedQuery<'static, DB>;
    fn get_checkpoint_by_sequence_number(
        sequence_number: i64,
    ) -> checkpoints::BoxedQuery<'static, DB>;
    fn get_latest_checkpoint() -> checkpoints::BoxedQuery<'static, DB>;
    fn multi_get_txs(
        cursor: Option<i64>,
        descending_order: bool,
        limit: i64,
        filter: Option<TransactionBlockFilter>,
        after_tx_seq_num: Option<i64>,
        before_tx_seq_num: Option<i64>,
    ) -> Result<transactions::BoxedQuery<'static, DB>, Error>;
    fn multi_get_coins(
        cursor: Option<Vec<u8>>,
        descending_order: bool,
        limit: i64,
        address: Vec<u8>,
        coin_type: String,
    ) -> objects::BoxedQuery<'static, DB>;
    fn multi_get_objs(
        cursor: Option<Vec<u8>>,
        descending_order: bool,
        limit: i64,
        filter: Option<ObjectFilter>,
        owner_type: Option<OwnerType>,
    ) -> Result<objects::BoxedQuery<'static, DB>, Error>;
    fn multi_get_balances(address: Vec<u8>) -> BalanceQuery<'static, DB>;
    fn get_balance(address: Vec<u8>, coin_type: String) -> BalanceQuery<'static, DB>;
    fn multi_get_checkpoints(
        cursor: Option<i64>,
        descending_order: bool,
        limit: i64,
        epoch: Option<i64>,
    ) -> checkpoints::BoxedQuery<'static, DB>;
}

/// Struct for custom diesel function
#[derive(Debug, Clone, Copy)]
pub struct Explained<T> {
    query: T,
}

/// Allows .explain() method on any Diesel query
pub trait Explain: Sized {
    fn explain(self) -> Explained<Self>;
}
impl<T> Explain for T {
    fn explain(self) -> Explained<Self> {
        Explained { query: self }
    }
}

/// All queries need to implement QueryId
impl<T: QueryId> QueryId for Explained<T> {
    type QueryId = (T::QueryId, std::marker::PhantomData<&'static str>);
    const HAS_STATIC_QUERY_ID: bool = T::HAS_STATIC_QUERY_ID;
}

/// Explained<T> is a fully structured query with return of type Text
impl<T: diesel::query_builder::Query> diesel::query_builder::Query for Explained<T> {
    type SqlType = Text;
}

// - The following is db-specific, and can be conditionally compiled

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
