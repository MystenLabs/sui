// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use sui_field_count::FieldCount;

use crate::schema::kv_objects;
use crate::schema::obj_versions;

#[derive(Insertable, Debug, Clone, FieldCount, Queryable)]
#[diesel(table_name = kv_objects, primary_key(object_id, object_version))]
#[diesel(treat_none_as_default_value = false)]
pub struct StoredObject {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub serialized_object: Option<Vec<u8>>,
}

#[derive(
    Insertable, Selectable, Debug, Clone, PartialEq, Eq, FieldCount, Queryable, QueryableByName,
)]
#[diesel(table_name = obj_versions, primary_key(object_id, object_version))]
pub struct StoredObjVersion {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub object_digest: Option<Vec<u8>>,
    pub cp_sequence_number: i64,
}
