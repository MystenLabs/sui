// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::owner_changes;
use crate::schema::owner_index;
use diesel::prelude::*;
use diesel_derive_enum::DbEnum;

#[derive(Queryable, Debug, Insertable, Clone)]
#[diesel(table_name = owner_changes)]
pub struct OwnerChange {
    pub object_id: String,
    pub version: i64,
    pub epoch: i64,
    pub checkpoint: i64,
    pub change_type: OwnerChangeType,
    pub owner_type: OwnerType,
    pub owner: Option<String>,
    pub initial_shared_version: Option<i64>,
    pub object_digest: String,
    pub object_type: Option<String>,
}

#[derive(DbEnum, Debug, Clone)]
#[ExistingTypePath = "crate::schema::sql_types::OwnerChangeType"]
pub enum OwnerChangeType {
    New,
    Modified,
    Deleted,
}

#[derive(DbEnum, Debug, Clone)]
#[ExistingTypePath = "crate::schema::sql_types::OwnerType"]
pub enum OwnerType {
    AddressOwner,
    ObjectOwner,
    Shared,
    Immutable,
}

#[derive(Queryable, Debug, Insertable)]
#[diesel(table_name = owner_index)]
pub struct OwnerIndex {
    pub object_id: String,
    pub version: i64,
    pub epoch: i64,
    pub owner_type: OwnerType,
    pub owner: Option<String>,
    pub initial_shared_version: Option<i64>,
    pub object_digest: String,
    pub object_type: String,
}
