// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;

use crate::schema::objects_version;
/// Model types related to tables that support efficient execution of queries on the `objects`,
/// `objects_history` and `objects_snapshot` tables.

#[derive(Queryable, Insertable, Debug, Identifiable, Clone, QueryableByName, Selectable)]
#[diesel(table_name = objects_version, primary_key(object_id, object_version))]
pub struct StoredObjectVersion {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub cp_sequence_number: i64,
}
