// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::{ExecutionError, SuiError},
    object::MoveObject,
};
use move_core_types::{annotated_value as A, language_storage::TypeTag};
use move_vm_types::loaded_data::runtime_types::Type;

pub trait LayoutResolver {
    fn get_annotated_layout(
        &mut self,
        object: &MoveObject,
    ) -> Result<A::MoveStructLayout, SuiError>;
}

pub trait TypeTagResolver {
    fn get_type_tag(&self, type_: &Type) -> Result<TypeTag, ExecutionError>;
}
