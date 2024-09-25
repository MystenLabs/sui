// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::test_utils::storage::InMemoryStorage;

use move_binary_format::{
    errors::{Location, PartialVMError, VMResult},
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress,
    language_storage::ModuleId,
    resolver::{ModuleResolver, ResourceResolver},
    vm_status::StatusCode,
};
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub struct TestStore {
    pub store: InMemoryStorage,
}

impl TestStore {
    pub fn new(store: InMemoryStorage) -> Self {
        Self { store }
    }

    pub fn get_compiled_modules(
        &self,
        package_id: &AccountAddress,
    ) -> VMResult<Vec<CompiledModule>> {
        let Ok(Some(modules)) = self.store.get_package(package_id) else {
            return Err(PartialVMError::new(StatusCode::LINKER_ERROR)
                .with_message(format!(
                    "Cannot find {:?} in data cache when building linkage context",
                    package_id
                ))
                .finish(Location::Undefined));
        };
        Ok(modules
            .iter()
            .map(|module| CompiledModule::deserialize_with_defaults(module).unwrap())
            .collect())
    }

    pub fn transitive_dependencies(
        &self,
        root_package: &AccountAddress,
    ) -> VMResult<BTreeSet<AccountAddress>> {
        fn generate_dependencies(
            store: &TestStore,
            seen: &mut BTreeSet<AccountAddress>,
            package_id: &AccountAddress,
        ) -> VMResult<()> {
            if seen.contains(package_id) {
                return Ok(());
            }

            seen.insert(*package_id);
            let Ok(Some(modules)) = store.store.get_package(package_id) else {
                return Err(PartialVMError::new(StatusCode::LINKER_ERROR)
                    .with_message(format!(
                        "Cannot find {:?} in data cache when building linkage context",
                        package_id
                    ))
                    .finish(Location::Undefined));
            };
            for module in &modules {
                let module = CompiledModule::deserialize_with_defaults(module).unwrap();
                let deps = module
                    .immediate_dependencies()
                    .into_iter()
                    .map(|module| *module.address())
                    .collect::<Vec<_>>();
                for dep in &deps {
                    generate_dependencies(store, seen, dep)?;
                }
            }
            Ok(())
        }

        let mut deps = BTreeSet::new();
        generate_dependencies(self, &mut deps, root_package)?;
        Ok(deps)
    }
}

// impl LinkageResolver for RelinkingStore {
//     type Error = ();
//
//     fn link_context(&self) -> AccountAddress {
//         self.context
//     }
//
//     /// Remaps `module_id` if it exists in the current linkage table, or returns it unchanged
//     /// otherwise.
//     fn relocate(&self, module_id: &ModuleId) -> Result<ModuleId, Self::Error> {
//         Ok(self.linkage.get(module_id).unwrap_or(module_id).clone())
//     }
//
//     fn defining_module(
//         &self,
//         module_id: &ModuleId,
//         struct_: &IdentStr,
//     ) -> Result<ModuleId, Self::Error> {
//         Ok(self
//             .type_origin
//             .get(&(module_id.clone(), struct_.to_owned()))
//             .unwrap_or(module_id)
//             .clone())
//     }
//
//     fn all_package_dependencies(&self) -> Result<BTreeSet<AccountAddress>, Self::Error> {
//         if let Some(dependent_packages) = &self.dependent_packages {
//             return Ok(dependent_packages.clone());
//         }
//         let modules = self.store.get_package(&self.context)?.unwrap();
//
//         Ok(all_deps)
//     }
// }

/// Implement by forwarding to the underlying in memory storage
impl ModuleResolver for TestStore {
    type Error = ();

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.store.get_module(id)
    }

    fn get_package(&self, id: &AccountAddress) -> Result<Option<Vec<Vec<u8>>>, Self::Error> {
        self.store.get_package(id)
    }
}

/// Implement by forwarding to the underlying in memory storage
impl ResourceResolver for TestStore {
    type Error = ();

    fn get_resource(
        &self,
        address: &AccountAddress,
        typ: &move_core_types::language_storage::StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        self.store.get_resource(address, typ)
    }
}
