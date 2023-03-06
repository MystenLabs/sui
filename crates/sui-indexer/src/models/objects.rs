// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::models::owners::OwnerType;
use crate::schema::objects;
use diesel::prelude::*;
use sui_json_rpc_types::SuiObjectData;
use sui_types::base_types::EpochId;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Owner;

#[derive(Queryable, Insertable, Debug, Identifiable, Clone)]
#[diesel(primary_key(object_id))]
pub struct Object {
    pub epoch: i64,
    pub checkpoint: i64,
    pub object_id: String,
    pub version: i64,
    pub object_digest: String,
    pub owner_type: OwnerType,
    pub owner_address: Option<String>,
    pub initial_shared_version: Option<i64>,
    pub previous_transaction: String,
    pub package_id: String,
    pub transaction_module: String,
    pub object_type: String,
}

impl Object {
    pub fn from(epoch: &EpochId, checkpoint: &CheckpointSequenceNumber, o: &SuiObjectData) -> Self {
        let (owner_type, owner_address, initial_shared_version) =
            owner_to_owner_info(&o.owner.expect("Expect the owner type to be non-empty"));
        Object {
            epoch: *epoch as i64,
            checkpoint: *checkpoint as i64,
            object_id: o.object_id.to_string(),
            version: o.version.value() as i64,
            object_digest: o.digest.base58_encode(),
            owner_type,
            owner_address,
            initial_shared_version,
            previous_transaction: o
                .previous_transaction
                .expect("Expect previous transaction to be non-empty")
                .base58_encode(),
            package_id: "".to_string(),
            transaction_module: "".to_string(),
            object_type: o
                .type_
                .as_ref()
                .expect("Expect the object type to be non-empty")
                .to_string(),
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
        Owner::Immutable => (OwnerType::Immutable, None, None),
    }
}
