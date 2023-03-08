// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::recipients;
use diesel::prelude::*;

#[derive(Queryable, Insertable, Debug, Clone, Default)]
pub struct Recipient {
    pub id: Option<i64>,
    pub transaction_digest: String,
    pub checkpoint_sequence_number: i64,
    pub epoch: i64,
    pub recipient: String,
}
