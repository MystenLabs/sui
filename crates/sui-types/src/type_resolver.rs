// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::annotated_value;
use move_core_types::language_storage::{StructTag, TypeTag};
use move_vm_types::loaded_data::runtime_types::Type;

use crate::error::{ExecutionError, SuiError};

pub trait LayoutResolver {
    fn get_annotated_layout(
        &mut self,
        struct_tag: &StructTag,
    ) -> Result<annotated_value::MoveStructLayout, SuiError>;
}

pub trait TypeTagResolver {
    fn get_type_tag(&self, type_: &Type) -> Result<TypeTag, ExecutionError>;
}
