// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{
    backend::Backend, deserialize, expression::AsExpression, prelude::*, serialize,
    sql_types::SmallInt, FromSqlRow,
};
use sui_field_count::FieldCount;
use sui_types::base_types::ObjectID;

use crate::schema::{
    kv_objects, obj_info, obj_versions, sum_coin_balances, sum_obj_types, wal_coin_balances,
    wal_obj_types,
};

#[derive(Insertable, Debug, Clone, FieldCount)]
#[diesel(table_name = kv_objects, primary_key(object_id, object_version))]
pub struct StoredObject {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub serialized_object: Option<Vec<u8>>,
}

#[derive(Insertable, Debug, Clone, FieldCount)]
#[diesel(table_name = obj_versions, primary_key(object_id, object_version))]
pub struct StoredObjVersion {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub object_digest: Vec<u8>,
    pub cp_sequence_number: i64,
}

/// An insert/update or deletion of an object record, keyed on a particular Object ID and version.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StoredObjectUpdate<T> {
    pub object_id: ObjectID,
    pub object_version: u64,
    pub cp_sequence_number: u64,
    /// `None` means the object was deleted or wrapped at this version, `Some(x)` means it was
    /// changed to `x`.
    pub update: Option<T>,
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

#[derive(Insertable, Debug, Clone, FieldCount)]
#[diesel(table_name = sum_coin_balances, primary_key(object_id))]
pub struct StoredSumCoinBalance {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub owner_id: Vec<u8>,
    pub coin_type: Vec<u8>,
    pub coin_balance: i64,
}

#[derive(Insertable, Debug, Clone, FieldCount)]
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

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = wal_coin_balances, primary_key(object_id, object_version))]
pub struct StoredWalCoinBalance {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub owner_id: Option<Vec<u8>>,
    pub coin_type: Option<Vec<u8>>,
    pub coin_balance: Option<i64>,
    pub cp_sequence_number: i64,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = wal_obj_types, primary_key(object_id, object_version))]
pub struct StoredWalObjType {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub owner_kind: Option<StoredOwnerKind>,
    pub owner_id: Option<Vec<u8>>,
    pub package: Option<Vec<u8>>,
    pub module: Option<String>,
    pub name: Option<String>,
    pub instantiation: Option<Vec<u8>>,
    pub cp_sequence_number: i64,
}

/// StoredObjectUpdate is a wrapper type, we want to count the fields of the inner type.
impl<T: FieldCount> FieldCount for StoredObjectUpdate<T> {
    // Add one here for cp_sequence_number field, because StoredObjectUpdate is used for
    // wal_* handlers, where the actual type to commit has an additional field besides fields of T.
    const FIELD_COUNT: usize = T::FIELD_COUNT.saturating_add(1);
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

#[derive(Insertable, Debug, Clone, FieldCount)]
#[diesel(table_name = obj_info, primary_key(object_id, cp_sequence_number))]
pub struct StoredObjInfo {
    pub object_id: Vec<u8>,
    pub cp_sequence_number: i64,
    pub owner_kind: Option<StoredOwnerKind>,
    pub owner_id: Option<Vec<u8>>,
    pub package: Option<Vec<u8>>,
    pub module: Option<String>,
    pub name: Option<String>,
    pub instantiation: Option<Vec<u8>>,
}
