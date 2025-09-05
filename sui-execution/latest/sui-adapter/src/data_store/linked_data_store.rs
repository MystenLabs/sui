// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::PackageStore,
    static_programmable_transactions::linkage::resolved_linkage::RootedLinkage,
};
use move_binary_format::errors::{Location, PartialVMError, PartialVMResult, VMResult};
use move_core_types::{
    account_address::AccountAddress,
    identifier::IdentStr,
    language_storage::ModuleId,
    resolver::{LinkageResolver, ModuleResolver},
    vm_status::StatusCode,
};
use move_vm_types::data_store::DataStore;
use sui_types::{
    base_types::ObjectID,
    error::{ExecutionErrorKind, SuiError},
};

/// A `LinkedDataStore` is a wrapper around a `PackageStore` (i.e., a package store where
/// we can also resolve types to defining IDs) along with a specific `linkage`. These two together
/// allow us to resolve modules and types in a way that is consistent with the `linkage` provided
/// and allow us to then pass this into the VM. Until we have a linkage set it is not possible to
/// construct a valid `DataStore` for execution in the VM as it needs to be able to resolve modules
/// under a specific linkage.
pub struct LinkedDataStore<'a> {
    pub linkage: &'a RootedLinkage,
    pub store: &'a dyn PackageStore,
}

impl<'a> LinkedDataStore<'a> {
    pub fn new(linkage: &'a RootedLinkage, store: &'a dyn PackageStore) -> Self {
        Self { linkage, store }
    }
}

impl DataStore for LinkedDataStore<'_> {
    fn link_context(&self) -> PartialVMResult<AccountAddress> {
        Ok(self.linkage.link_context)
    }

    fn relocate(&self, module_id: &ModuleId) -> PartialVMResult<ModuleId> {
        self.linkage
            .resolved_linkage
            .linkage
            .get(&ObjectID::from(*module_id.address()))
            .map(|obj_id| ModuleId::new(**obj_id, module_id.name().to_owned()))
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::LINKER_ERROR).with_message(format!(
                    "Error relocating {module_id} -- could not find linkage"
                ))
            })
    }

    fn defining_module(
        &self,
        module_id: &ModuleId,
        struct_: &IdentStr,
    ) -> PartialVMResult<ModuleId> {
        self.store
            .resolve_type_to_defining_id(
                    ObjectID::from(*module_id.address()),
                    module_id.name(),
                    struct_,
                )
                .ok()
                .flatten()
                .map(|obj_id| ModuleId::new(*obj_id, module_id.name().to_owned()))
                .ok_or_else(|| {
                    PartialVMError::new(StatusCode::LINKER_ERROR).with_message(format!(
                        "Error finding defining module for {module_id}::{struct_} -- could nod find linkage"
                    ))
                })
    }

    // NB: module_id is original ID based
    fn load_module(&self, module_id: &ModuleId) -> VMResult<Vec<u8>> {
        let package_storage_id = ObjectID::from(*module_id.address());
        match self
            .store
            .get_package(&package_storage_id)
            .map(|pkg| pkg.and_then(|pkg| pkg.get_module(module_id).cloned()))
        {
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
        Ok(())
    }
}

impl DataStore for &LinkedDataStore<'_> {
    fn link_context(&self) -> PartialVMResult<AccountAddress> {
        DataStore::link_context(*self)
    }

    fn relocate(&self, module_id: &ModuleId) -> PartialVMResult<ModuleId> {
        DataStore::relocate(*self, module_id)
    }

    fn defining_module(
        &self,
        module_id: &ModuleId,
        struct_: &IdentStr,
    ) -> PartialVMResult<ModuleId> {
        DataStore::defining_module(*self, module_id, struct_)
    }

    fn load_module(&self, module_id: &ModuleId) -> VMResult<Vec<u8>> {
        DataStore::load_module(*self, module_id)
    }

    fn publish_module(&mut self, _module_id: &ModuleId, _blob: Vec<u8>) -> VMResult<()> {
        Ok(())
    }
}

impl ModuleResolver for LinkedDataStore<'_> {
    type Error = SuiError;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.load_module(id)
            .map(Some)
            .map_err(|_| SuiError::from(ExecutionErrorKind::VMVerificationOrDeserializationError))
    }
}

impl LinkageResolver for LinkedDataStore<'_> {
    type Error = SuiError;

    fn link_context(&self) -> AccountAddress {
        // TODO should we propagate the error
        DataStore::link_context(self).unwrap()
    }

    fn relocate(&self, module_id: &ModuleId) -> Result<ModuleId, Self::Error> {
        DataStore::relocate(self, module_id).map_err(|err| {
            make_invariant_violation!("Error relocating {}: {:?}", module_id, err).into()
        })
    }

    fn defining_module(
        &self,
        runtime_id: &ModuleId,
        struct_: &IdentStr,
    ) -> Result<ModuleId, Self::Error> {
        DataStore::defining_module(self, runtime_id, struct_).map_err(|err| {
            make_invariant_violation!(
                "Error finding defining module for {}::{}: {:?}",
                runtime_id,
                struct_,
                err
            )
            .into()
        })
    }
}
