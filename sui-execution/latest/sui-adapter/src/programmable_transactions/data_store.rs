// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::programmable_transactions::linkage_view::LinkageView;
use move_binary_format::errors::{Location, PartialVMError, PartialVMResult, VMResult};
use move_core_types::{
    account_address::AccountAddress, identifier::IdentStr, language_storage::ModuleId,
    resolver::ModuleResolver, vm_status::StatusCode,
};
use move_vm_types::data_store::DataStore;
use sui_types::move_package::MovePackage;

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

// TODO: `DataStore` will be reworked and this is likely to disappear.
//       Leaving this comment around until then as testament to better days to come...
impl DataStore for SuiDataStore<'_, '_> {
    fn link_context(&self) -> PartialVMResult<AccountAddress> {
        self.linkage_view.link_context().map_err(|e| {
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message(e.to_string())
        })
    }

    fn relocate(&self, module_id: &ModuleId) -> PartialVMResult<ModuleId> {
        self.linkage_view.relocate(module_id).map_err(|err| {
            PartialVMError::new(StatusCode::LINKER_ERROR)
                .with_message(format!("Error relocating {module_id}: {err:?}"))
        })
    }

    fn defining_module(
        &self,
        runtime_id: &ModuleId,
        struct_: &IdentStr,
    ) -> PartialVMResult<ModuleId> {
        self.linkage_view
            .defining_module(runtime_id, struct_)
            .map_err(|err| {
                PartialVMError::new(StatusCode::LINKER_ERROR).with_message(format!(
                    "Error finding defining module for {runtime_id}::{struct_}: {err:?}"
                ))
            })
    }

    fn load_module(&self, module_id: &ModuleId) -> VMResult<Vec<u8>> {
        if let Some(bytes) = self.get_module(module_id) {
            return Ok(bytes.clone());
        }
        match self.linkage_view.get_module(module_id) {
            Ok(Some(bytes)) => Ok(bytes),
            Ok(None) => Err(PartialVMError::new(StatusCode::LINKER_ERROR)
                .with_message(format!("Cannot find {:?} in data cache", module_id))
                .finish(Location::Undefined)),
            Err(err) => {
                let msg = format!("Unexpected storage error: {:?}", err);
                Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(msg)
                        .finish(Location::Undefined),
                )
            }
        }
    }

    fn publish_module(&mut self, _module_id: &ModuleId, _blob: Vec<u8>) -> VMResult<()> {
        // we cannot panic here because during execution and publishing this is
        // currently called from the publish flow in the Move runtime
        Ok(())
    }
}
