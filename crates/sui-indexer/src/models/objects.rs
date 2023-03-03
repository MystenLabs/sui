// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::models::owners::OwnerType;
use crate::schema::objects;
use diesel::prelude::*;
use sui_json_rpc_types::{SuiData, SuiParsedObject};
use sui_types::object::Owner;

#[derive(Queryable, Insertable, Debug, Identifiable, Clone)]
#[diesel(primary_key(object_id))]
pub struct Object {
    #[diesel(deserialize_as = i64)]
    pub id: Option<i64>,
    pub object_id: String,
    pub version: i64,
    pub owner_type: OwnerType,
    pub owner_address: Option<String>,
    pub initial_shared_version: Option<i64>,
    pub package_id: String,
    pub transaction_module: String,
    pub object_type: Option<String>,
    pub object_status: String,
}

impl From<SuiParsedObject> for Object {
    fn from(o: SuiParsedObject) -> Self {
        let (owner_type, owner_address, initial_shared_version) = owner_to_owner_info(&o.owner);
        Object {
            id: None,
            object_id: o.id().to_string(),
            version: o.version().value() as i64,
            owner_type,
            owner_address,
            initial_shared_version,
            package_id: "".to_string(),
            transaction_module: "".to_string(),
            object_type: o.data.type_().map(|t| t.to_string()),
            object_status: "".to_string(),
        }
    }
}

// return owner_type, owner_address and initial_shared_version
pub fn owner_to_owner_info(owner: &Owner) -> (OwnerType, Option<String>, Option<i64>) {
    match owner {
        Owner::AddressOwner(address) => (OwnerType::AddressOwner, Some(address.to_string()), None),
        Owner::ObjectOwner(address) => (OwnerType::ObjectOwner, Some(address.to_string()), None),
        Owner::Shared {
            initial_shared_version,
        } => (
            OwnerType::Shared,
            None,
            Some(initial_shared_version.value() as i64),
        ),
        Owner::Immutable => (OwnerType::Shared, None, None),
    }
}
