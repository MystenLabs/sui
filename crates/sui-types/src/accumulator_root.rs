// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::SequenceNumber, error::SuiResult, object::Owner, storage::ObjectStore,
    SUI_ACCUMULATOR_ROOT_OBJECT_ID,
};
use move_core_types::{ident_str, identifier::IdentStr};

pub const ACCUMULATOR_ROOT_MODULE: &IdentStr = ident_str!("accumulator");
pub const ACCUMULATOR_ROOT_CREATE_FUNC: &IdentStr = ident_str!("create");

pub fn get_accumulator_root_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> SuiResult<Option<SequenceNumber>> {
    Ok(object_store
        .get_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
        .map(|obj| match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => initial_shared_version,
            _ => unreachable!("Accumulator root object must be shared"),
        }))
}
