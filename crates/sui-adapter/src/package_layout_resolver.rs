// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::programmable_transactions::linkage_view::{LinkageInfo, LinkageView};
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::resolver::LinkageResolver;
use move_core_types::{language_storage::ModuleId, value::MoveStructLayout};
use sui_types::base_types::ObjectID;
use sui_types::storage::BackingPackageStore;
use sui_types::{
    error::SuiError,
    layout_resolver::LayoutResolver,
    object::{MoveObject, ObjectFormatOptions},
};

pub struct PackageLayoutResolver<'state, S: GetModule + BackingPackageStore> {
    linkage_view: LinkageView<'state, S>,
}

impl<'state, S: GetModule + BackingPackageStore> PackageLayoutResolver<'state, S> {
    pub fn new(temporary_store: &'state S) -> Self {
        let linkage_view = LinkageView::new(temporary_store, LinkageInfo::Unset);
        Self { linkage_view }
    }
}

// Return a `MoveStructLayout` given an `Object`
impl<'state, S: GetModule<Error = SuiError, Item = CompiledModule> + BackingPackageStore>
    LayoutResolver for PackageLayoutResolver<'state, S>
{
    fn get_layout(
        &mut self,
        format: ObjectFormatOptions,
        object: &MoveObject,
    ) -> Result<MoveStructLayout, SuiError> {
        // println!("START: get_layout {:?}", object);
        let package_id = &ObjectID::from(object.type_().address());
        match self.linkage_view.storage().get_package(package_id)? {
            None => {
                // println!("No package found");
                panic!()
            }
            Some(package) => {
                // println!("Set package {:?}", package);
                self.linkage_view.set_linkage(&package)?
            }
        };
        let res = object.get_layout(format, self);
        self.linkage_view.reset_linkage();
        // println!("END: get_layout {:?}", object);
        res
    }
}

impl<'state, S: GetModule<Error = SuiError, Item = CompiledModule> + BackingPackageStore> GetModule
    for PackageLayoutResolver<'state, S>
{
    type Error = SuiError;
    type Item = CompiledModule;

    fn get_module_by_id(&self, runtime_id: &ModuleId) -> Result<Option<CompiledModule>, SuiError> {
        // println!("START get_module_by_id {:?}", runtime_id);
        let storage_id = match self.linkage_view.relocate(runtime_id) {
            Ok(id) => id,
            Err(_) => runtime_id.clone(),
        };
        let res = self.linkage_view.storage().get_module_by_id(&storage_id);
        // println!("END get_module_by_id {:?}", runtime_id);
        res
    }
}
