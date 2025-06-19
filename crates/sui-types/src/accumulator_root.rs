// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    dynamic_field::{derive_dynamic_field_id, DOFWrapper},
    error::SuiResult,
    object::Owner,
    storage::ObjectStore,
    MoveTypeTagTrait, SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID,
};
use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AccumulatorKey {
    owner: SuiAddress,
    type_tag: TypeTag,
}

impl AccumulatorKey {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_PACKAGE_ID.into(),
            module: ACCUMULATOR_ROOT_MODULE.to_owned(),
            name: ident_str!("AccumulatorKey").to_owned(),
            type_params: vec![],
        }
    }
}

impl MoveTypeTagTrait for AccumulatorKey {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(Self::type_()))
    }
}

// TODO(address-balances): This may not be the actual way of organizing balance accounts.
// Fix it when we have the Move code.
pub fn derive_balance_account_object_id(
    owner: SuiAddress,
    type_tag: TypeTag,
) -> anyhow::Result<ObjectID> {
    let key = DOFWrapper {
        name: AccumulatorKey { owner, type_tag },
    };
    derive_dynamic_field_id(
        SUI_ACCUMULATOR_ROOT_OBJECT_ID,
        &AccumulatorKey::get_type_tag(),
        &bcs::to_bytes(&key)?,
    )
    .map_err(|e| e.into())
}
