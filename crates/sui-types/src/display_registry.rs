// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    SUI_DISPLAY_REGISTRY_OBJECT_ID, base_types::SequenceNumber, error::SuiResult, object::Owner,
    storage::ObjectStore,
};

pub fn get_display_registry_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> SuiResult<Option<SequenceNumber>> {
    Ok(object_store
        .get_object(&SUI_DISPLAY_REGISTRY_OBJECT_ID)
        .map(|obj| match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => initial_shared_version,
            _ => unreachable!("DisplayRegistry object must be shared"),
        }))
}
