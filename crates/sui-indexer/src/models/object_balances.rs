// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::object_balances;
use diesel::prelude::*;

#[derive(Queryable, Insertable, Debug, Clone)]
#[diesel(table_name = object_balances)]
pub struct ObjectBalance {
    pub id: String,
    pub version: i64,
    pub coin_type: String,
    pub balance: i64,
}
