// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::SequenceNumber, error::SuiResult, object::Owner, storage::ObjectStore,
    SUI_COIN_REGISTRY_OBJECT_ID,
};
use move_core_types::{ident_str, identifier::IdentStr};

pub const COIN_REGISTRY_MODULE_NAME: &IdentStr = ident_str!("coin_registry");
pub const COIN_REGISTRY_CREATE_FUNCTION_NAME: &IdentStr = ident_str!("create");

pub fn get_coin_registry_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> SuiResult<Option<SequenceNumber>> {
    Ok(object_store
        .get_object(&SUI_COIN_REGISTRY_OBJECT_ID)
        .map(|obj| match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => initial_shared_version,
            _ => unreachable!("Coin Registry object must be shared"),
        }))
}
