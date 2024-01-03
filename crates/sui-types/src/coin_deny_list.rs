// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{ObjectID, SequenceNumber};
use crate::error::SuiResult;
use crate::object::Owner;
use crate::storage::ObjectStore;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;

pub const COIN_DENY_LIST_OBJECT_ID: ObjectID = ObjectID::from_address(coin_deny_list_addr());

pub const COIN_DENY_LIST_MODULE: &IdentStr = ident_str!("coin");
pub const COIN_DENY_LIST_CREATE_FUNC: &IdentStr = ident_str!("create_deny_list_object");

/// Returns 0x404
const fn coin_deny_list_addr() -> AccountAddress {
    let mut addr = [0u8; AccountAddress::LENGTH];
    addr[AccountAddress::LENGTH - 2] = 0x4;
    addr[AccountAddress::LENGTH - 1] = 0x4;
    AccountAddress::new(addr)
}

pub fn get_coin_deny_list_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> SuiResult<Option<SequenceNumber>> {
    Ok(object_store
        .get_object(&COIN_DENY_LIST_OBJECT_ID)?
        .map(|obj| match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => initial_shared_version,
            _ => unreachable!("Randomness state object must be shared"),
        }))
}
