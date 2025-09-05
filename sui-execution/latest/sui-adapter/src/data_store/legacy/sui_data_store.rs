// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::data_store::{PackageStore, legacy::linkage_view::LinkageView};
use move_binary_format::errors::{Location, PartialVMError, PartialVMResult, VMResult};
use move_core_types::{
    account_address::AccountAddress, identifier::IdentStr, language_storage::ModuleId,
    resolver::ModuleResolver, vm_status::StatusCode,
};
use move_vm_runtime::shared::data_store::DataStore;
use move_vm_types::data_store::DataStore;
use std::rc::Rc;
use sui_types::{base_types::ObjectID, error::SuiResult, move_package::MovePackage};

// Implementation of the `DataStore` trait for the Move VM.
// When used during execution it may have a list of new packages that have
// just been published in the current context. Those are used for module/type
// resolution when executing module init.
// It may be created with an empty slice of packages either when no publish/upgrade
// are performed or when a type is requested not during execution.
pub(crate) struct SuiDataStore<'state, 'a> {
    linkage_view: &'a LinkageView<'state>,
    new_packages: &'a [MovePackage],
}

impl<'state, 'a> SuiDataStore<'state, 'a> {
    pub(crate) fn new(
        linkage_view: &'a LinkageView<'state>,
        new_packages: &'a [MovePackage],
    ) -> Self {
        Self {
            linkage_view,
            new_packages,
        }
    }

    fn get_module(&self, module_id: &ModuleId) -> Option<&Vec<u8>> {
        for package in self.new_packages {
            let module = package.get_module(module_id);
            if module.is_some() {
                return module;
            }
        }
        None
    }
}

impl PackageStore for SuiDataStore<'_, '_> {
    fn get_package(&self, id: &ObjectID) -> SuiResult<Option<Rc<MovePackage>>> {
        for package in self.new_packages {
            if package.id() == *id {
                return Ok(Some(Rc::new(package.clone())));
            }
        }
        self.linkage_view.get_package(id)
    }

    fn resolve_type_to_defining_id(
        &self,
        _module_address: ObjectID,
        _module_name: &IdentStr,
        _type_name: &IdentStr,
    ) -> SuiResult<Option<ObjectID>> {
        unimplemented!(
            "resolve_type_to_defining_id is not implemented for legacy::SuiDataStore and should never be called"
        )
    }
}
