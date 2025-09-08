// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::SequenceNumber, error::SuiResult, object::Owner, storage::ObjectStore,
    SUI_REGISTRY_ROOT_OBJECT_ID,
};

pub fn get_root_registry_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> SuiResult<Option<SequenceNumber>> {
    Ok(object_store
        .get_object(&SUI_REGISTRY_ROOT_OBJECT_ID)
        .map(|obj| match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => initial_shared_version,
            _ => unreachable!("Registry Root object must be shared"),
        }))
}
