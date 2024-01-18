// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::backend::Backend;
use sui_indexer::schema_v2::{display, objects};

use diesel::{query_builder::QueryId, sql_types::Text};

pub(crate) trait GenericQueryBuilder<DB: Backend> {
    fn get_obj_by_type(object_type: String) -> objects::BoxedQuery<'static, DB>;
    fn get_display_by_obj_type(object_type: String) -> display::BoxedQuery<'static, DB>;
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
