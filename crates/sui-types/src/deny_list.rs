// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::SequenceNumber;
use crate::collection_types::{Bag, Table};
use crate::dynamic_field::get_dynamic_field_from_store;
use crate::id::UID;
use crate::object::{Object, Owner};
use crate::storage::ObjectStore;
use crate::SUI_DENY_LIST_OBJECT_ID;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use serde::{Deserialize, Serialize};
use tracing::error;

pub const DENY_LIST_MODULE: &IdentStr = ident_str!("deny_list");
pub const DENY_LIST_CREATE_FUNC: &IdentStr = ident_str!("create");

pub const DENY_LIST_COIN_TYPE_INDEX: u64 = 0;

/// Rust representation of the Move type 0x2::deny_list::DenyList.
/// It has a bag that contains the deny lists for different system types.
/// At creation, there is only one type (at key 0), which is the Coin type.
/// We also take advantage of the dynamic nature of Bag to add more types in the future,
/// as well as making changes to the deny lists for existing types.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DenyList {
    pub id: UID,
    pub lists: Bag,
}

/// Rust representation of the Move type 0x2::deny_list::PerTypeDenyList.
/// denied_count is a table that stores the number of denied addresses for each coin template type.
/// It can be used as a quick check to see if an address is denied for any coin template type.
/// denied_addresses is a table that stores all the addresses that are denied for each coin template type.
/// The key to the table is the coin template type in string form: package_id::module_name::struct_name.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PerTypeDenyList {
    pub id: UID,
    // Table<address, u64>
    pub denied_count: Table,
    // Table<vec<u8>, VecSet<address>>
    pub denied_addresses: Table,
}

pub fn get_deny_list_root_object(object_store: &dyn ObjectStore) -> Option<Object> {
    match object_store.get_object(&SUI_DENY_LIST_OBJECT_ID) {
        Ok(Some(obj)) => Some(obj),
        Ok(None) => {
            error!("Deny list object not found");
            None
        }
        Err(err) => {
            error!("Failed to get deny list object: {}", err);
            None
        }
    }
}

pub fn get_coin_deny_list(object_store: &dyn ObjectStore) -> Option<PerTypeDenyList> {
    get_deny_list_root_object(object_store).and_then(|obj| {
        let deny_list: DenyList = obj
            .to_rust()
            .expect("DenyList object type must be consistent");
        match get_dynamic_field_from_store(
            object_store,
            *deny_list.lists.id.object_id(),
            &DENY_LIST_COIN_TYPE_INDEX,
        ) {
            Ok(deny_list) => Some(deny_list),
            Err(err) => {
                error!("Failed to get deny list inner state: {}", err);
                None
            }
        }
    })
}

pub fn get_deny_list_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> Option<SequenceNumber> {
    get_deny_list_root_object(object_store).map(|obj| match obj.owner {
        Owner::Shared {
            initial_shared_version,
        } => initial_shared_version,
        _ => unreachable!("Deny list object must be shared"),
    })
}
