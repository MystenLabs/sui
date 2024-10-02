// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::domains;
use diesel::prelude::*;

#[derive(Queryable, Selectable, Insertable, AsChangeset, Debug)]
#[diesel(table_name = domains)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct VerifiedDomain {
    pub field_id: String,
    pub name: String,
    pub parent: String,
    pub expiration_timestamp_ms: i64,
    pub nft_id: String,
    pub target_address: Option<String>,
    pub data: serde_json::Value,
    pub last_checkpoint_updated: i64,
    pub subdomain_wrapper_id: Option<String>,
}
