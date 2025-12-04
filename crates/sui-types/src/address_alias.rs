// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{StructTag, TypeTag};
use mysten_common::debug_fatal;
use serde::{Deserialize, Serialize};

use crate::base_types::{SequenceNumber, SuiAddress};
use crate::collection_types::VecSet;
use crate::error::{SuiErrorKind, SuiResult};
use crate::object::Owner;
use crate::storage::ObjectStore;
use crate::{SUI_ADDRESS_ALIAS_STATE_OBJECT_ID, derived_object};
use crate::{SUI_FRAMEWORK_ADDRESS, id::UID};

// Rust version of the Move sui::authenticator_state::AddressAliases type
#[derive(Debug, Serialize, Deserialize)]
pub struct AddressAliases {
    pub id: UID,
    pub aliases: VecSet<SuiAddress>,
}

pub fn get_address_alias_state_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> SuiResult<Option<SequenceNumber>> {
    Ok(object_store
        .get_object(&SUI_ADDRESS_ALIAS_STATE_OBJECT_ID)
        .map(|obj| match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => initial_shared_version,
            _ => unreachable!("Address alias state object must be shared"),
        }))
}

pub fn get_address_aliases_from_store(
    object_store: &dyn ObjectStore,
    address: SuiAddress,
) -> SuiResult<Option<(AddressAliases, SequenceNumber)>> {
    let alias_key_type = TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: Identifier::new("address_alias").unwrap(),
        name: Identifier::new("AliasKey").unwrap(),
        type_params: vec![],
    }));

    let key_bytes = bcs::to_bytes(&address).unwrap();
    let Ok(address_aliases_id) = derived_object::derive_object_id(
        SuiAddress::from(SUI_ADDRESS_ALIAS_STATE_OBJECT_ID),
        &alias_key_type,
        &key_bytes,
    ) else {
        debug_fatal!("failed to compute derived object id for alias state");
        return Err(SuiErrorKind::Unknown(
            "failed to compute derived object id for alias state".to_string(),
        )
        .into());
    };
    let address_aliases = object_store.get_object(&address_aliases_id);

    Ok(address_aliases.map(|obj| {
        let move_obj = obj
            .data
            .try_as_move()
            .expect("AddressAliases object must be a MoveObject");
        let address_aliases: AddressAliases =
            bcs::from_bytes(move_obj.contents()).expect("failed to parse AddressAliases object");
        (address_aliases, obj.version())
    }))
}
