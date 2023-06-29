// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::watermarks;
use diesel::prelude::*;

#[derive(Queryable, AsChangeset, Insertable, Debug, Clone)]
#[diesel(table_name = watermarks)]
pub struct Watermark {
    pub name: String,
    pub checkpoint: Option<i64>,
    pub epoch: Option<i64>,
}
