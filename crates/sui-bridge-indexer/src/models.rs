// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::indexer::schema::tokens;
use diesel::prelude::*;

#[derive(Queryable, Selectable, Insertable, AsChangeset, Debug)]
#[diesel(table_name = tokens)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct TokenTxn {
    pub message_key: Vec<u8>,
    pub checkpoint: i64,
    pub epoch: i64,
    pub token_type: i32,
    pub source_chain: i32,
    pub destination_chain: i32,
}
