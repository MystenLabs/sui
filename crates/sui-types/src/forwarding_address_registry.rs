// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID;
use crate::base_types::SequenceNumber;
use crate::error::SuiResult;
use crate::object::Owner;
use crate::storage::ObjectStore;

pub fn get_forwarding_address_registry_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> SuiResult<Option<SequenceNumber>> {
    Ok(object_store
        .get_object(&SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID)
        .map(|obj| match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => initial_shared_version,
            _ => unreachable!("ForwardingAddressRegistry object must be shared"),
        }))
}
