// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;

use crate::schema::objects_version;

use super::objects::StoredDeletedObject;
use super::objects::StoredObject;

/// Model types related to tables that support efficient execution of queries on the `objects`,
/// `objects_history` and `objects_snapshot` tables.

#[derive(Queryable, Insertable, Debug, Identifiable, Clone, QueryableByName)]
#[diesel(table_name = objects_version, primary_key(object_id, object_version))]
pub struct StoredObjectVersion {
    pub object_id: Vec<u8>,
    pub object_version: i64,
    pub cp_sequence_number: i64,
}

impl From<&StoredObject> for StoredObjectVersion {
    fn from(o: &StoredObject) -> Self {
        Self {
            object_id: o.object_id.clone(),
            object_version: o.object_version,
            cp_sequence_number: o.checkpoint_sequence_number,
        }
    }
}

impl From<&StoredDeletedObject> for StoredObjectVersion {
    fn from(o: &StoredDeletedObject) -> Self {
        Self {
            object_id: o.object_id.clone(),
            object_version: o.object_version,
            cp_sequence_number: o.checkpoint_sequence_number,
        }
    }
}
