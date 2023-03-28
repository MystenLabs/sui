// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;

use crate::schema::addresses;

#[derive(Queryable, Insertable, Debug)]
#[diesel(table_name = addresses, primary_key(account_address))]
pub struct Address {
    pub account_address: String,
    pub first_appearance_tx: String,
    pub first_appearance_time: i64,
}
