// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::{ExecutionError, SuiError},
    object::{MoveObject, ObjectFormatOptions},
};
use move_core_types::language_storage::StructTag;
use move_core_types::{language_storage::TypeTag, value::MoveStructLayout};
use move_vm_types::loaded_data::runtime_types::Type;

pub trait LayoutResolver {
    fn get_layout(
        &mut self,
        object: &MoveObject,
        format: ObjectFormatOptions,
    ) -> Result<MoveStructLayout, SuiError>;

    fn get_layout_from_struct_tag(
        &mut self,
        struct_tag: &StructTag,
        format: ObjectFormatOptions,
    ) -> Result<MoveStructLayout, SuiError>;
}

impl LayoutResolver for Box<dyn LayoutResolver> {
    fn get_layout(
        &mut self,
        object: &MoveObject,
        format: ObjectFormatOptions,
    ) -> Result<MoveStructLayout, SuiError> {
        self.as_mut().get_layout(object, format)
    }

    fn get_layout_from_struct_tag(
        &mut self,
        struct_tag: &StructTag,
        format: ObjectFormatOptions,
    ) -> Result<MoveStructLayout, SuiError> {
        self.as_mut().get_layout_from_struct_tag(struct_tag, format)
    }
}

pub trait TypeTagResolver {
    fn get_type_tag(&self, type_: &Type) -> Result<TypeTag, ExecutionError>;
}
