// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_vm_runtime::on_chain::ast::PackageStorageId;

use move_binary_format::CompiledModule;
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::ModuleId,
    resolver::{LinkageResolver, ModuleResolver, ResourceResolver},
};
use move_vm_test_utils::InMemoryStorage;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub struct RelinkingStore {
    pub store: InMemoryStorage,
    pub context: AccountAddress,
    pub linkage: BTreeMap<ModuleId, ModuleId>,
    pub dependent_packages: Option<BTreeSet<PackageStorageId>>,
    type_origin: BTreeMap<(ModuleId, Identifier), ModuleId>,
}

impl RelinkingStore {
    pub fn new(store: InMemoryStorage) -> Self {
        Self {
            store,
            context: AccountAddress::ZERO,
            linkage: BTreeMap::new(),
            dependent_packages: None,
            type_origin: BTreeMap::new(),
        }
    }

    pub fn relink(
        &mut self,
        context: AccountAddress,
        linkage: BTreeMap<ModuleId, ModuleId>,
        type_origin: BTreeMap<(ModuleId, Identifier), ModuleId>,
    ) {
        self.context = context;
        self.linkage = linkage;
        self.type_origin = type_origin;
    }

    pub fn set_dependent_packages(&mut self, dependent_packages: BTreeSet<PackageStorageId>) {
        self.dependent_packages = Some(dependent_packages);
    }
}

/// Implemented by referencing the store's built-in data structures
impl LinkageResolver for RelinkingStore {
    type Error = ();

    fn link_context(&self) -> AccountAddress {
        self.context
    }

    /// Remaps `module_id` if it exists in the current linkage table, or returns it unchanged
    /// otherwise.
    fn relocate(&self, module_id: &ModuleId) -> Result<ModuleId, Self::Error> {
        Ok(self.linkage.get(module_id).unwrap_or(module_id).clone())
    }

    fn defining_module(
        &self,
        module_id: &ModuleId,
        struct_: &IdentStr,
    ) -> Result<ModuleId, Self::Error> {
        Ok(self
            .type_origin
            .get(&(module_id.clone(), struct_.to_owned()))
            .unwrap_or(module_id)
            .clone())
    }

    fn all_package_dependencies(&self) -> Result<BTreeSet<AccountAddress>, Self::Error> {
        if let Some(dependent_packages) = &self.dependent_packages {
            return Ok(dependent_packages.clone());
        }
        let modules = self.store.get_package(&self.context)?.unwrap();
        let mut all_deps = BTreeSet::new();
        for module in &modules {
            let module = CompiledModule::deserialize_with_defaults(&module).unwrap();
            let deps = module.immediate_dependencies();
            for dep in deps {
                all_deps.insert(self.relocate(&dep).unwrap().address().clone());
            }
        }

        Ok(all_deps)
    }
}

/// Implement by forwarding to the underlying in memory storage
impl ModuleResolver for RelinkingStore {
    type Error = ();

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.store.get_module(id)
    }

    fn get_package(&self, id: &AccountAddress) -> Result<Option<Vec<Vec<u8>>>, Self::Error> {
        self.store.get_package(id)
    }
}

/// Implement by forwarding to the underlying in memory storage
impl ResourceResolver for RelinkingStore {
    type Error = ();

    fn get_resource(
        &self,
        address: &AccountAddress,
        typ: &move_core_types::language_storage::StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        self.store.get_resource(address, typ)
    }
}
