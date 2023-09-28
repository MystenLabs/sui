// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::models::objects::ObjectStatus;
use diesel::prelude::*;
use diesel_derive_enum::DbEnum;
use serde::{Deserialize, Serialize};

#[derive(Queryable, Debug, Clone)]
#[diesel(table_name = owner)]
pub struct ObjectOwner {
    pub object_id: String,
    pub version: i64,
    pub epoch: i64,
    pub checkpoint: i64,
    pub owner_type: OwnerType,
    pub owner_address: Option<String>,
    pub object_digest: String,
    pub object_status: ObjectStatus,
}

#[derive(DbEnum, Debug, Clone, Deserialize, Serialize)]
pub enum OwnerType {
    AddressOwner,
    ObjectOwner,
    Shared,
    Immutable,
}

impl OwnerType {
    pub fn to_string(&self) -> &'static str {
        match self {
            OwnerType::AddressOwner => "AddressOwner",
            OwnerType::ObjectOwner => "ObjectOwner",
            OwnerType::Shared => "Shared",
            OwnerType::Immutable => "Immutable",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s {
            "AddressOwner" => Some(OwnerType::AddressOwner),
            "ObjectOwner" => Some(OwnerType::ObjectOwner),
            "Shared" => Some(OwnerType::Shared),
            "Immutable" => Some(OwnerType::Immutable),
            _ => None,
        }
    }
}

#[derive(Queryable, Debug)]
pub struct OwnerHistory {
    pub object_id: String,
    pub version: i64,
    pub epoch: i64,
    pub checkpoint: i64,
    pub owner_type: OwnerType,
    pub owner_address: Option<String>,
    pub old_owner_type: Option<OwnerType>,
    pub old_owner_address: Option<String>,
    pub object_digest: String,
    pub object_status: ObjectStatus,
}
