// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::kv_objects;
use diesel::prelude::*;

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = kv_objects, primary_key(object_id, object_version))]
pub struct StoredObject {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub serialized_object: Option<Vec<u8>>,
}
