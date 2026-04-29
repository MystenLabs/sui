// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::{SuiError, SuiErrorKind};
use move_bytecode_utils::{layout::TypeLayoutBuilder, module_cache::GetModule};
use move_core_types::{
    compressed::annotated as CA,
    language_storage::{StructTag, TypeTag},
};

pub trait LayoutResolver {
    fn get_annotated_layout(
        &mut self,
        struct_tag: &StructTag,
    ) -> Result<CA::MoveDatatypeLayout, SuiError>;
}

pub fn get_layout_from_struct_tag(
    struct_tag: StructTag,
    resolver: &impl GetModule,
) -> Result<CA::MoveDatatypeLayout, SuiError> {
    let type_ = TypeTag::Struct(Box::new(struct_tag));
    let layout = TypeLayoutBuilder::build_with_types(&type_, resolver).map_err(|e| {
        SuiErrorKind::ObjectSerializationError {
            error: e.to_string(),
        }
    })?;
    match CA::MoveDatatypeLayout::new(layout) {
        Some(dt) => Ok(dt),
        None => {
            unreachable!(
                "We called get_layout_from_struct_tag on a datatype, should get a datatype layout"
            )
        }
    }
}

pub fn into_struct_layout(
    layout: CA::MoveDatatypeLayout,
) -> Result<CA::MoveStructLayout, SuiError> {
    match layout.into_inner() {
        CA::MoveDatatypeLayout_::Struct(s) => Ok(*s),
        CA::MoveDatatypeLayout_::Enum(e) => Err(SuiErrorKind::ObjectSerializationError {
            error: format!("Expected struct layout but got an enum {e:?}"),
        }
        .into()),
    }
}
