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
    resolver::{ModuleResolver, SerializedPackage},
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
        let Ok([Some(pkg)]) = self.store.get_packages_static([*package_id]) else {
            return Err(PartialVMError::new(StatusCode::LINKER_ERROR)
                .with_message(format!(
                    "Cannot find {:?} in data cache when building linkage context",
                    package_id
                ))
                .finish(Location::Undefined));
        };
        Ok(pkg
            .modules
            .iter()
            .map(|module| CompiledModule::deserialize_with_defaults(module).unwrap())
            .collect())
    }

    /// Compute all of the transitive dependencies for a `root_package`, including itself.
    pub fn transitive_dependencies(
        &self,
        root_package: &AccountAddress,
    ) -> VMResult<BTreeSet<AccountAddress>> {
        let mut seen: BTreeSet<AccountAddress> = BTreeSet::new();
        let mut to_process: Vec<AccountAddress> = vec![*root_package];

        while let Some(package_id) = to_process.pop() {
            // If we've already processed this package, skip it
            if seen.contains(&package_id) {
                continue;
            }

            // Add the current package to the seen set
            seen.insert(package_id);

            // Attempt to retrieve the package's modules from the store
            let Ok([Some(pkg)]) = self.store.get_packages_static([package_id]) else {
                return Err(PartialVMError::new(StatusCode::LINKER_ERROR)
                    .with_message(format!(
                        "Cannot find {:?} in data cache when building linkage context",
                        package_id
                    ))
                    .finish(Location::Undefined));
            };

            // Process each module and add its dependencies to the to_process list
            for module in &pkg.modules {
                let module = CompiledModule::deserialize_with_defaults(module).unwrap();
                let deps = module
                    .immediate_dependencies()
                    .into_iter()
                    .map(|module| *module.address());

                // Add unprocessed dependencies to the queue
                for dep in deps {
                    if !seen.contains(&dep) {
                        to_process.push(dep);
                    }
                }
            }
        }

        Ok(seen)
    }
}

/// Implement by forwarding to the underlying in memory storage
impl ModuleResolver for TestStore {
    type Error = ();

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.store.get_module(id)
    }

    fn get_packages_static<const N: usize>(
        &self,
        ids: [AccountAddress; N],
    ) -> Result<[Option<SerializedPackage>; N], Self::Error> {
        self.store.get_packages_static(ids)
    }

    fn get_packages(
        &self,
        ids: &[AccountAddress],
    ) -> Result<Vec<Option<SerializedPackage>>, Self::Error> {
        self.store.get_packages(ids)
    }
}
