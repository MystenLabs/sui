// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::SuiError,
    object::{MoveObject, ObjectFormatOptions},
    storage::BackingPackageStore,
};
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::value::MoveStructLayout;

// Hide details of package relinking
pub trait LayoutResolver {
    // Return a `MoveStructLayout` given an `Object`
    fn get_layout(
        &mut self,
        format: ObjectFormatOptions,
        object: &MoveObject,
    ) -> Result<MoveStructLayout, SuiError>;
}

pub struct ForwardLayoutResolver<S: GetModule + BackingPackageStore> {
    store: S,
}

impl<S: GetModule + BackingPackageStore> ForwardLayoutResolver<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }
}

impl<S: GetModule + BackingPackageStore> LayoutResolver for ForwardLayoutResolver<S> {
    fn get_layout(
        &mut self,
        format: ObjectFormatOptions,
        object: &MoveObject,
    ) -> Result<MoveStructLayout, SuiError> {
        object.get_layout(format, &self.store)
    }
}
