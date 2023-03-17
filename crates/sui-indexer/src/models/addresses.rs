// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::models::transactions::Transaction;
use crate::schema::addresses;

use diesel::prelude::*;

#[derive(Queryable, Insertable, Debug)]
#[diesel(table_name = addresses, primary_key(account_address))]
pub struct Address {
    pub account_address: String,
    pub first_appearance_tx: String,
    pub first_appearance_time: i64,
}

impl From<&Transaction> for Address {
    fn from(txn: &Transaction) -> Self {
        Address {
            account_address: txn.sender.clone(),
            first_appearance_tx: txn.transaction_digest.clone(),
            first_appearance_time: txn.timestamp_ms,
        }
    }
}
