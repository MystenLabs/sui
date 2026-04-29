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
    ) -> Result<CA::MoveTypeLayout, SuiError>;
}

pub fn get_layout_from_struct_tag(
    struct_tag: StructTag,
    resolver: &impl GetModule,
) -> Result<CA::MoveTypeLayout, SuiError> {
    let type_ = TypeTag::Struct(Box::new(struct_tag));
    let layout = TypeLayoutBuilder::build_with_types(&type_, resolver).map_err(|e| {
        SuiErrorKind::ObjectSerializationError {
            error: e.to_string(),
        }
    })?;
    if layout.as_datatype().is_none() {
        unreachable!(
            "We called get_layout_from_struct_tag on a datatype, should get a datatype layout"
        );
    }
    Ok(layout)
}

/// Inflate a compressed type layout to its tree struct case, erroring if it is
/// not a struct.
pub fn into_tree_struct_layout(
    layout: CA::MoveTypeLayout,
) -> Result<A::MoveStructLayout, SuiError> {
    let Some(struct_layout) = layout.as_struct() else {
        return Err(SuiErrorKind::ObjectSerializationError {
            error: "Expected struct layout".to_owned(),
        }
        .into());
    };
    let inflated = CA::MoveDatatypeLayout::Struct(struct_layout)
        .inflate()
        .map_err(|e| SuiErrorKind::ObjectSerializationError {
            error: e.to_string(),
        })?;
    match inflated {
        A::MoveDatatypeLayout::Struct(s) => Ok(*s),
        A::MoveDatatypeLayout::Enum(_) => unreachable!("started from struct layout"),
    }
}

pub fn into_struct_layout<'l>(
    layout: CA::MoveDatatypeLayout<'l>,
) -> Result<CA::MoveStructLayout<'l>, SuiError> {
    match layout {
        CA::MoveDatatypeLayout::Struct(s) => Ok(s),
        CA::MoveDatatypeLayout::Enum(e) => Err(SuiErrorKind::ObjectSerializationError {
            error: format!("Expected struct layout but got an enum {e:?}"),
        }
        .into()),
    }
}
