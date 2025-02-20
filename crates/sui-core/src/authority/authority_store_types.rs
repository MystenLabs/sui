// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use sui_types::base_types::MoveObjectType;
use sui_types::base_types::TransactionDigest;
use sui_types::coin::Coin;
use sui_types::error::SuiError;
use sui_types::move_package::MovePackage;
use sui_types::object::{Data, MoveObject, Object, ObjectInner, Owner};
use sui_types::storage::ObjectKey;

// Versioning process:
//
// Object storage versioning is done lazily (at read time) - therefore we must always preserve the
// code for reading the very first storage version. For all versions, a migration function
//
//   f(V_n) -> V_(n+1)
//
// must be defined. This way we can iteratively migrate the very oldest version to the very newest
// version at any point in the future.
//
// To change the format of the object table value types (StoreObject and StoreMoveObject), use the
// following process:
// - Add a new variant to the enum to store the new version type.
// - Extend the `migrate` functions to migrate from the previous version to the new version.
// - Change `From<Object> for StoreObjectPair` to create the newest version only.
//
// Additionally, the first time we version these formats, we will need to:
// - Add a check in the `TryFrom<StoreObjectPair> for Object` to see if the object that was just
//   read is the latest version.
// - If it is not, use the migration function (as explained above) to migrate it to the next
//   version.
// - Repeat until we have arrive at the current version.

/// Enum wrapper for versioning
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub enum StoreObjectWrapper {
    V1(StoreObjectV1),
}

// always points to latest version.
pub type StoreObject = StoreObjectV1;

impl StoreObjectWrapper {
    pub fn migrate(self) -> Self {
        // TODO: when there are multiple versions, we must iteratively migrate from version N to
        // N+1 until we arrive at the latest version
        self
    }

    // Always returns the most recent version. Older versions are migrated to the latest version at
    // read time, so there is never a need to access older versions.
    pub fn inner(&self) -> &StoreObject {
        match self {
            Self::V1(v1) => v1,

            // can remove #[allow] when there are multiple versions
            #[allow(unreachable_patterns)]
            _ => panic!("object should have been migrated to latest version at read time"),
        }
    }
    pub fn into_inner(self) -> StoreObject {
        match self {
            Self::V1(v1) => v1,

            // can remove #[allow] when there are multiple versions
            #[allow(unreachable_patterns)]
            _ => panic!("object should have been migrated to latest version at read time"),
        }
    }
}

impl From<StoreObject> for StoreObjectWrapper {
    fn from(o: StoreObject) -> Self {
        StoreObjectWrapper::V1(o)
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub enum StoreObjectV1 {
    Value(StoreObjectValue),
    Deleted,
    Wrapped,
}

/// Forked version of [`sui_types::object::Object`]
/// Used for efficient storing of move objects in the database
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct StoreObjectValue {
    pub data: StoreData,
    pub owner: Owner,
    pub previous_transaction: TransactionDigest,
    pub storage_rebate: u64,
}

/// Forked version of [`sui_types::object::Data`]
/// Adds extra enum value `IndirectObject`, which represents a reference to an object stored separately
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub enum StoreData {
    Move(MoveObject),
    Package(MovePackage),
    IndirectObjectDeprecated,
    Coin(u64),
}

pub fn get_store_object(object: Object) -> StoreObjectWrapper {
    let object = object.into_inner();
    let data = match object.data {
        Data::Package(package) => StoreData::Package(package),
        Data::Move(move_obj) => {
            if move_obj.type_().is_gas_coin() {
                StoreData::Coin(
                    Coin::from_bcs_bytes(move_obj.contents())
                        .expect("failed to deserialize coin")
                        .balance
                        .value(),
                )
            } else {
                StoreData::Move(move_obj)
            }
        }
    };
    let store_object = StoreObjectValue {
        data,
        owner: object.owner,
        previous_transaction: object.previous_transaction,
        storage_rebate: object.storage_rebate,
    };
    StoreObject::Value(store_object).into()
}

pub(crate) fn try_construct_object(
    object_key: &ObjectKey,
    store_object: StoreObjectValue,
) -> Result<Object, SuiError> {
    let data = match store_object.data {
        StoreData::Move(object) => Data::Move(object),
        StoreData::Package(package) => Data::Package(package),
        StoreData::Coin(balance) => unsafe {
            Data::Move(MoveObject::new_from_execution_with_limit(
                MoveObjectType::gas_coin(),
                true,
                object_key.1,
                bcs::to_bytes(&(object_key.0, balance)).expect("serialization failed"),
                u64::MAX,
            )?)
        },
        _ => {
            return Err(SuiError::Storage(
                "corrupted field: inconsistent object representation".to_string(),
            ))
        }
    };

    Ok(ObjectInner {
        data,
        owner: store_object.owner,
        previous_transaction: store_object.previous_transaction,
        storage_rebate: store_object.storage_rebate,
    }
    .into())
}
