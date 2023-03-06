// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::move_calls;
use diesel::prelude::*;

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
