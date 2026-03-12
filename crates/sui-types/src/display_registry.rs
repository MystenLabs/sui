// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::StructTag;
use move_core_types::language_storage::TypeTag;
use serde::Deserialize;
use serde::Serialize;

use crate::SUI_DISPLAY_REGISTRY_OBJECT_ID;
use crate::SUI_FRAMEWORK_ADDRESS;
use crate::base_types::ObjectID;
use crate::base_types::SequenceNumber;
use crate::collection_types::VecMap;
use crate::derived_object::derive_object_id;
use crate::error::SuiResult;
use crate::object::Owner;
use crate::storage::ObjectStore;

pub const DISPLAY_REGISTRY_MODULE_NAME: &IdentStr = ident_str!("display_registry");
pub const DISPLAY_KEY_STRUCT_NAME: &IdentStr = ident_str!("DisplayKey");

#[derive(Serialize, Deserialize)]
pub struct Display {
    pub id: ObjectID,
    pub fields: VecMap<String, String>,
    pub cap_id: Option<ObjectID>,
}

impl Display {
    pub fn fields(&self) -> impl Iterator<Item = (&str, &str)> {
        self.fields
            .contents
            .iter()
            .map(|e| (e.key.as_str(), e.value.as_str()))
    }
}

/// Derive the ObjectID at which to find the Display object for the given type.
pub fn display_object_id(type_: TypeTag) -> Result<ObjectID, bcs::Error> {
    derive_object_id(
        SUI_DISPLAY_REGISTRY_OBJECT_ID,
        &display_key(type_).into(),
        &[0x00],
    )
}

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

fn display_key(type_: TypeTag) -> StructTag {
    StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: DISPLAY_REGISTRY_MODULE_NAME.to_owned(),
        name: DISPLAY_KEY_STRUCT_NAME.to_owned(),
        type_params: vec![type_],
    }
}
