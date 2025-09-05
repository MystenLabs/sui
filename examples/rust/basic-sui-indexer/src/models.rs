// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use sui_indexer_alt_framework::FieldCount;
use crate::schema::transaction_digests;

#[derive(Insertable, Debug, Clone, FieldCount)]
#[diesel(table_name = transaction_digests)]
pub struct StoredTransactionDigest {
    pub tx_digest: String,
    pub checkpoint_sequence_number: i64,
}
