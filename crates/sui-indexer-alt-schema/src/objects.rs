// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use diesel::{
    backend::Backend, deserialize, expression::AsExpression, prelude::*, serialize,
    sql_types::SmallInt, FromSqlRow,
};

use sui_field_count::FieldCount;
use sui_types::object::{Object, Owner};

use crate::schema::{coin_balance_buckets, kv_objects, obj_info, obj_versions};

#[derive(Insertable, Debug, Clone, FieldCount, Queryable)]
#[diesel(table_name = kv_objects, primary_key(object_id, object_version))]
#[diesel(treat_none_as_default_value = false)]
pub struct StoredObject {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub serialized_object: Option<Vec<u8>>,
}

#[derive(Insertable, Selectable, Debug, Clone, FieldCount, Queryable)]
#[diesel(table_name = obj_versions, primary_key(object_id, object_version))]
pub struct StoredObjVersion {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub object_digest: Vec<u8>,
    pub cp_sequence_number: i64,
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

#[derive(AsExpression, FromSqlRow, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[diesel(sql_type = SmallInt)]
#[repr(i16)]
pub enum StoredCoinOwnerKind {
    Fastpath = 0,
    Consensus = 1,
}

#[derive(Insertable, Debug, Clone, FieldCount, Queryable)]
#[diesel(table_name = obj_info, primary_key(object_id, cp_sequence_number))]
#[diesel(treat_none_as_default_value = false)]
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

#[derive(Insertable, Queryable, Debug, Clone, FieldCount, Eq, PartialEq)]
#[diesel(table_name = coin_balance_buckets, primary_key(object_id, cp_sequence_number))]
#[diesel(treat_none_as_default_value = false)]
pub struct StoredCoinBalanceBucket {
    pub object_id: Vec<u8>,
    pub cp_sequence_number: i64,
    pub owner_kind: Option<StoredCoinOwnerKind>,
    pub owner_id: Option<Vec<u8>>,
    pub coin_type: Option<Vec<u8>>,
    pub coin_balance_bucket: Option<i16>,
}

impl StoredObjInfo {
    pub fn from_object(object: &Object, cp_sequence_number: i64) -> anyhow::Result<Self> {
        let type_ = object.type_();
        Ok(Self {
            object_id: object.id().to_vec(),
            cp_sequence_number,
            owner_kind: Some(match object.owner() {
                Owner::AddressOwner(_) => StoredOwnerKind::Address,
                Owner::ObjectOwner(_) => StoredOwnerKind::Object,
                Owner::Shared { .. } => StoredOwnerKind::Shared,
                Owner::Immutable => StoredOwnerKind::Immutable,
                // We do not distinguish between fastpath owned and consensus v2 owned
                // objects. Also we only support single owner for now.
                // In the future, if we support more sophisticated authenticator,
                // this will be changed.
                Owner::ConsensusV2 { .. } => StoredOwnerKind::Address,
            }),

            owner_id: match object.owner() {
                Owner::AddressOwner(a) => Some(a.to_vec()),
                Owner::ObjectOwner(o) => Some(o.to_vec()),
                Owner::Shared { .. } | Owner::Immutable { .. } => None,
                Owner::ConsensusV2 { authenticator, .. } => {
                    Some(authenticator.as_single_owner().to_vec())
                }
            },

            package: type_.map(|t| t.address().to_vec()),
            module: type_.map(|t| t.module().to_string()),
            name: type_.map(|t| t.name().to_string()),
            instantiation: type_
                .map(|t| bcs::to_bytes(&t.type_params()))
                .transpose()
                .with_context(|| {
                    format!(
                        "Failed to serialize type parameters for {}",
                        object.id().to_canonical_display(/* with_prefix */ true),
                    )
                })?,
        })
    }
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

impl<DB: Backend> serialize::ToSql<SmallInt, DB> for StoredCoinOwnerKind
where
    i16: serialize::ToSql<SmallInt, DB>,
{
    fn to_sql<'b>(&'b self, out: &mut serialize::Output<'b, '_, DB>) -> serialize::Result {
        match self {
            StoredCoinOwnerKind::Fastpath => 0.to_sql(out),
            StoredCoinOwnerKind::Consensus => 1.to_sql(out),
        }
    }
}

impl<DB: Backend> deserialize::FromSql<SmallInt, DB> for StoredCoinOwnerKind
where
    i16: deserialize::FromSql<SmallInt, DB>,
{
    fn from_sql(raw: DB::RawValue<'_>) -> deserialize::Result<Self> {
        Ok(match i16::from_sql(raw)? {
            0 => StoredCoinOwnerKind::Fastpath,
            1 => StoredCoinOwnerKind::Consensus,
            o => return Err(format!("Unexpected StoredCoinOwnerKind: {o}").into()),
        })
    }
}
