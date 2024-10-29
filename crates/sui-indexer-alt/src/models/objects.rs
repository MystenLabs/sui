// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{
    backend::Backend, deserialize, expression::AsExpression, prelude::*, serialize,
    sql_types::SmallInt, FromSqlRow,
};

use crate::schema::{kv_objects, sum_obj_types};

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = kv_objects, primary_key(object_id, object_version))]
pub struct StoredObject {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub serialized_object: Option<Vec<u8>>,
}

#[derive(AsExpression, FromSqlRow, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[diesel(sql_type = SmallInt)]
#[repr(i16)]
pub enum StoredOwnerKind {
    Immutable = 0,
    Address = 1,
    Object = 2,
    Shared = 3,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = sum_obj_types, primary_key(object_id))]
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

impl<DB: Backend> serialize::ToSql<SmallInt, DB> for StoredOwnerKind
where
    i16: serialize::ToSql<SmallInt, DB>,
{
    fn to_sql<'b>(&'b self, out: &mut serialize::Output<'b, '_, DB>) -> serialize::Result {
        match self {
            StoredOwnerKind::Immutable => 0.to_sql(out),
            StoredOwnerKind::Address => 1.to_sql(out),
            StoredOwnerKind::Object => 2.to_sql(out),
            StoredOwnerKind::Shared => 3.to_sql(out),
        }
    }
}

impl<DB: Backend> deserialize::FromSql<SmallInt, DB> for StoredOwnerKind
where
    i16: deserialize::FromSql<SmallInt, DB>,
{
    fn from_sql(raw: DB::RawValue<'_>) -> deserialize::Result<Self> {
        Ok(match i16::from_sql(raw)? {
            0 => StoredOwnerKind::Immutable,
            1 => StoredOwnerKind::Address,
            2 => StoredOwnerKind::Object,
            3 => StoredOwnerKind::Shared,
            o => return Err(format!("Unexpected StoredOwnerKind: {o}").into()),
        })
    }
}
