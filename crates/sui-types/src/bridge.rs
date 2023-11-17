// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::SequenceNumber;
use crate::error::SuiResult;
use crate::object::Owner;
use crate::storage::ObjectStore;
use crate::SUI_BRIDGE_OBJECT_ID;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;

pub const BRIDGE_MODULE_NAME: &IdentStr = ident_str!("bridge");
pub const BRIDGE_CREATE_FUNCTION_NAME: &IdentStr = ident_str!("create");

pub fn get_bridge_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> SuiResult<Option<SequenceNumber>> {
    Ok(object_store
        .get_object(&SUI_BRIDGE_OBJECT_ID)?
        .map(|obj| match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => initial_shared_version,
            _ => unreachable!("Bridge object must be shared"),
        }))
}
