// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::{changed_objects, input_objects, move_calls, recipients};
use diesel::prelude::*;

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = input_objects)]
pub struct InputObject {
    pub id: Option<i64>,
    pub transaction_digest: String,
    pub checkpoint_sequence_number: i64,
    pub epoch: i64,
    pub object_id: String,
    pub object_version: Option<i64>,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = move_calls)]
pub struct MoveCall {
    pub id: Option<i64>,
    pub transaction_digest: String,
    pub checkpoint_sequence_number: i64,
    pub epoch: i64,
    pub sender: String,
    pub move_package: String,
    pub move_module: String,
    pub move_function: String,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = recipients)]
pub struct Recipient {
    pub id: Option<i64>,
    pub transaction_digest: String,
    pub checkpoint_sequence_number: i64,
    pub epoch: i64,
    pub sender: String,
    pub recipient: String,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = changed_objects)]
pub struct ChangedObject {
    pub id: Option<i64>,
    pub transaction_digest: String,
    pub checkpoint_sequence_number: i64,
    pub epoch: i64,
    pub object_id: String,
    // object_change_type could be `mutated`, `created` or `unwrapped`.
    pub object_change_type: String,
    pub object_version: i64,
}
