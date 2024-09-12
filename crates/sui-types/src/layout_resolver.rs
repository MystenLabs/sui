// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::SuiError;
use move_bytecode_utils::{layout::TypeLayoutBuilder, module_cache::GetModule};
use move_core_types::{
    annotated_value as A,
    language_storage::{StructTag, TypeTag},
};

pub trait LayoutResolver {
    fn get_annotated_layout(
        &mut self,
        struct_tag: &StructTag,
    ) -> Result<A::MoveDatatypeLayout, SuiError>;
}

pub fn get_layout_from_struct_tag(
    struct_tag: StructTag,
    resolver: &impl GetModule,
) -> Result<A::MoveDatatypeLayout, SuiError> {
    let type_ = TypeTag::Struct(Box::new(struct_tag));
    let layout = TypeLayoutBuilder::build_with_types(&type_, resolver).map_err(|e| {
        SuiError::ObjectSerializationError {
            error: e.to_string(),
        }
    })?;
    match layout {
        A::MoveTypeLayout::Struct(l) => Ok(A::MoveDatatypeLayout::Struct(l)),
        A::MoveTypeLayout::Enum(e) => Ok(A::MoveDatatypeLayout::Enum(e)),
        _ => {
            unreachable!(
                "We called get_layout_from_struct_tag on a datatype, should get a datatype layout"
            )
        }
    }
}

pub fn into_struct_layout(layout: A::MoveDatatypeLayout) -> Result<A::MoveStructLayout, SuiError> {
    match layout {
        A::MoveDatatypeLayout::Struct(s) => Ok(*s),
        A::MoveDatatypeLayout::Enum(e) => Err(SuiError::ObjectSerializationError {
            error: format!("Expected struct layout but got an enum {e:?}"),
        }),
    }
}
