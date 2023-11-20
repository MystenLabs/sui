// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{backend::Backend, query_builder::AsQuery, query_dsl::methods::BoxedDsl};
use sui_indexer::{
    schema_v2::{checkpoints, epochs, objects, transactions},
    types_v2::OwnerType,
};

use crate::{
    error::Error,
    types::{object::ObjectFilter, transaction_block::TransactionBlockFilter},
};
use diesel::{
    query_builder::{BoxedSelectStatement, FromClause, QueryId},
    sql_types::Text,
};

#[derive(Clone, Copy)]
pub(crate) enum Cursor {
    After,
    Before,
}

/// Controls how many records the query fetches before or after some cursor
#[derive(Clone, Copy)]
pub(crate) enum QueryDirection {
    /// Fetch first n records after some cursor (exclusive) or from the beginning.
    /// The edge closest to the cursor or beginning comes first in the result set.
    First(i64),
    /// Fetch last n records before some cursor (exclusive) or from the end.
    /// The edge closest to the cursor or end comes last in the result set.
    Last(i64),
}

/// Controls the final ordering of the result set
#[derive(Clone, Copy)]
pub(crate) enum SortOrder {
    /// Preserves the original order of the result set.
    /// This is typically a query ordered by some key in ascending order.
    Asc,
    /// Reverses the order of the result set.
    /// This is typically a query ordered by some key in descending order.
    Desc,
}

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
    fn get_obj_by_type(object_type: String) -> objects::BoxedQuery<'static, DB>;
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
        address: Option<Vec<u8>>,
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
        cursor_type: Option<Cursor>,
        limit: i64,
        edge_order: SortOrder,
        query_direction: QueryDirection,
        epoch: Option<i64>,
    ) -> checkpoints::BoxedQuery<'static, DB>;
}

/// The struct returned for query.explain()
#[derive(Debug, Clone, Copy)]
pub struct Explained<T> {
    pub query: T,
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

/// The struct returned for query.subquery()
#[derive(Debug, Clone, Copy)]
pub struct Subqueried<T> {
    pub query: T,
}

/// Allows .subquery() method on any Diesel query
pub trait Subquery: AsQuery + Sized {
    fn subquery(self) -> Subqueried<Self>;
}
impl<T: AsQuery> Subquery for T {
    fn subquery(self) -> Subqueried<Self> {
        Subqueried { query: self }
    }
}

/// All queries need to implement QueryId
impl<T: QueryId> QueryId for Subqueried<T> {
    type QueryId = (T::QueryId, std::marker::PhantomData<&'static str>);
    const HAS_STATIC_QUERY_ID: bool = T::HAS_STATIC_QUERY_ID;
}

/// Subqueried<T> wraps the query in a SELECT * FROM (query) AS SUB
impl<T: diesel::query_builder::Query> diesel::query_builder::Query for Subqueried<T> {
    type SqlType = T::SqlType;
}
