// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    MoveTypeTagTrait, SUI_ACCUMULATOR_ROOT_OBJECT_ID,
    SUI_FRAMEWORK_ADDRESS, 
    dynamic_field::{DynamicFieldKey},
    error::SuiResult,
    storage::{ObjectStore},
};
use move_core_types::{
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};

pub const ACCUMULATOR_METADATA_MODULE: &IdentStr = ident_str!("accumulator_metadata");
pub const ACCUMULATOR_OBJECT_COUNT_KEY_STRUCT_NAME: &IdentStr =
    ident_str!("AccumulatorObjectCountKey");

/// Rust version of the Move sui::accumulator_metadata::AccumulatorObjectCountKey type.
/// This is used as a dynamic field key to store the net count of accumulator objects
/// as a dynamic field on the accumulator root object.
///
/// There is no u8 in the Move definition, however empty structs in Move
/// are represented as a single byte 0 in the serialized data.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct AccumulatorObjectCountKey(u8);

impl MoveTypeTagTrait for AccumulatorObjectCountKey {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: ACCUMULATOR_METADATA_MODULE.to_owned(),
            name: ACCUMULATOR_OBJECT_COUNT_KEY_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }))
    }
}

/// Reads the accumulator object count from the accumulator root's dynamic fields.
pub fn get_accumulator_object_count(object_store: &dyn ObjectStore) -> SuiResult<Option<u64>> {
    DynamicFieldKey(
        SUI_ACCUMULATOR_ROOT_OBJECT_ID,
        AccumulatorObjectCountKey(0),
        AccumulatorObjectCountKey::get_type_tag(),
    )
    .into_unbounded_id()?
    .load_object(object_store)
    .map(|o| o.load_value::<u64>())
    .transpose()
}
