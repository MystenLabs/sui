// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::backend::Backend;
use sui_indexer::{
    schema_v2::{display, objects},
    types_v2::OwnerType,
};

use crate::{error::Error, types::object::ObjectFilter};
use diesel::{
    query_builder::{BoxedSelectStatement, FromClause, QueryId},
    sql_types::Text,
};

use super::db_data_provider::PageLimit;

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
    fn get_obj(address: Vec<u8>, version: Option<i64>) -> objects::BoxedQuery<'static, DB>;
    fn get_obj_by_type(object_type: String) -> objects::BoxedQuery<'static, DB>;
    fn get_display_by_obj_type(object_type: String) -> display::BoxedQuery<'static, DB>;
    fn multi_get_coins(
        before: Option<Vec<u8>>,
        after: Option<Vec<u8>>,
        limit: PageLimit,
        address: Option<Vec<u8>>,
        coin_type: String,
    ) -> objects::BoxedQuery<'static, DB>;
    fn multi_get_objs(
        before: Option<Vec<u8>>,
        after: Option<Vec<u8>>,
        limit: PageLimit,
        filter: Option<ObjectFilter>,
        owner_type: Option<OwnerType>,
    ) -> Result<objects::BoxedQuery<'static, DB>, Error>;
    fn multi_get_balances(address: Vec<u8>) -> BalanceQuery<'static, DB>;
    fn get_balance(address: Vec<u8>, coin_type: String) -> BalanceQuery<'static, DB>;
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
