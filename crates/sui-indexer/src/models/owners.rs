// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use diesel_derive_enum::DbEnum;

#[derive(Queryable, Debug, Clone)]
pub struct ObjectOwner {
    pub object_id: String,
    pub version: i64,
    pub epoch: i64,
    pub checkpoint: i64,
    pub owner_type: OwnerType,
    pub owner_address: Option<String>,
    pub initial_shared_version: Option<i64>,
    pub object_digest: String,
}

#[derive(DbEnum, Debug, Clone)]
#[ExistingTypePath = "crate::schema::sql_types::ChangeType"]
pub enum ChangeType {
    New,
    Modify,
    Delete,
}

#[derive(DbEnum, Debug, Clone)]
#[ExistingTypePath = "crate::schema::sql_types::OwnerType"]
pub enum OwnerType {
    AddressOwner,
    ObjectOwner,
    Shared,
    Immutable,
}

#[derive(Queryable, Debug)]
pub struct OwnerHistory {
    pub object_id: String,
    pub version: i64,
    pub epoch: i64,
    pub owner_type: OwnerType,
    pub owner_address: Option<String>,
    pub initial_shared_version: Option<i64>,
    pub object_digest: String,
}
