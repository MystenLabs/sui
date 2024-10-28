// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::{kv_objects, sum_obj_types};
use diesel::{
    backend::Backend,
    expression::AsExpression,
    prelude::*,
    serialize::{Output, Result, ToSql},
    sql_types::SmallInt,
};

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = kv_objects, primary_key(object_id, object_version))]
pub struct StoredObject {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub serialized_object: Option<Vec<u8>>,
}

#[derive(AsExpression, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[diesel(sql_type = SmallInt)]
#[repr(i16)]
pub enum StoredOwnerKind {
    Immutable = 0,
    Address = 1,
    Object = 2,
    Shared = 3,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = sum_obj_types, primary_key(object_id, object_version))]
pub struct StoredSumObjType {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub owner_kind: StoredOwnerKind,
    pub owner_id: Option<Vec<u8>>,
    pub package: Option<Vec<u8>>,
    pub module: Option<String>,
    pub name: Option<String>,
    pub instantiation: Option<Vec<u8>>,
}

impl<DB: Backend> ToSql<SmallInt, DB> for StoredOwnerKind
where
    i16: ToSql<SmallInt, DB>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, DB>) -> Result {
        match self {
            StoredOwnerKind::Immutable => 0.to_sql(out),
            StoredOwnerKind::Address => 1.to_sql(out),
            StoredOwnerKind::Object => 2.to_sql(out),
            StoredOwnerKind::Shared => 3.to_sql(out),
        }
    }
}
