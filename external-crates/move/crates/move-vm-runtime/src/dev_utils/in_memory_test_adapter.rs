// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dev_utils::{
        gas_schedule::GasStatus, storage::InMemoryStorage, vm_test_adapter::VMTestAdapter,
    },
    execution::vm::MoveVM,
    natives::{extensions::NativeContextExtensions, functions::NativeFunctions},
    runtime::MoveRuntime,
    shared::{
        linkage_context::LinkageContext,
        types::{PackageStorageId, RuntimePackageId},
    },
};

use move_binary_format::errors::{Location, PartialVMError, VMResult};
use move_binary_format::file_format::CompiledModule;

use move_core_types::{
    account_address::AccountAddress, resolver::ModuleResolver, vm_status::StatusCode,
};
use move_vm_config::runtime::VMConfig;

use std::collections::BTreeSet;

pub struct InMemoryTestAdapter {
    runtime: MoveRuntime,
    pub storage: InMemoryStorage,
}

impl InMemoryTestAdapter {
    pub fn new() -> Self {
        let storage = InMemoryStorage::new();
        let native_functions = NativeFunctions::empty_for_testing().unwrap();
        let vm_config = VMConfig::default();
        let runtime = MoveRuntime::new(native_functions, vm_config);
        Self { runtime, storage }
    }

    pub fn new_with_runtime(runtime: MoveRuntime) -> Self {
        let storage = InMemoryStorage::new();
        Self { runtime, storage }
    }

    pub fn new_with_runtime_and_storage(runtime: MoveRuntime, storage: InMemoryStorage) -> Self {
        Self { runtime, storage }
    }

    /// Insert package into storage without any checking or validation. This is useful for
    /// testing invalid packages and other failures.
    pub fn insert_modules_into_storage(
        &mut self,
        modules: Vec<CompiledModule>,
    ) -> anyhow::Result<()> {
        assert!(
            !modules.is_empty(),
            "Tried to add empty module(s) to storage"
        );
        // TODO: Should we enforce this is a set?
        for module in modules {
            let module_id = module.self_id();
            let mut module_bytes = vec![];
            module.serialize_with_version(module.version, &mut module_bytes)?;
            self.storage
                .publish_or_overwrite_module(module_id, module_bytes);
        }
        Ok(())
    }

    pub fn get_compiled_modules(
        &self,
        package_id: &RuntimePackageId,
    ) -> VMResult<Vec<CompiledModule>> {
        let Ok([Some(pkg)]) = self.storage.get_packages_static([*package_id]) else {
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
            let Ok([Some(pkg)]) = self.storage.get_packages_static([package_id]) else {
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

impl VMTestAdapter<InMemoryStorage> for InMemoryTestAdapter {
    fn publish_package(
        &mut self,
        linkage_context: LinkageContext,
        runtime_id: RuntimePackageId,
        modules: Vec<CompiledModule>,
    ) -> VMResult<()> {
        let Some(storage_id) = linkage_context.linkage_table.get(&runtime_id).cloned() else {
            // TODO: VM error instead?
            panic!("Did not find runtime ID in linkage context.");
        };
        let modules = modules
            .into_iter()
            .map(|module| {
                let mut module_bytes = vec![];
                module
                    .serialize_with_version(module.version, &mut module_bytes)
                    .unwrap();
                module_bytes
            })
            .collect::<Vec<_>>();

        let mut gas_meter = GasStatus::new_unmetered();
        let (changeset, _) = self.runtime.validate_package(
            &self.storage,
            &linkage_context,
            runtime_id,
            storage_id,
            modules,
            &mut gas_meter,
        );
        self.storage
            .apply(changeset?)
            .expect("Failed to apply change set");
        Ok(())
    }

    fn make_vm<'extensions>(
        &self,
        linkage_context: LinkageContext,
    ) -> VMResult<MoveVM<'extensions, &InMemoryStorage>> {
        let Self { runtime, storage } = self;
        let storage: &InMemoryStorage = storage;
        runtime.make_vm(storage, linkage_context)
    }

    fn mave_vm_with_native_extensions<'extensions>(
        &self,
        linkage_context: LinkageContext,
        native_extensions: NativeContextExtensions<'extensions>,
    ) -> VMResult<MoveVM<'extensions, &InMemoryStorage>> {
        let Self { runtime, storage } = self;
        runtime.make_vm_with_native_extensions(storage, linkage_context, native_extensions)
    }

    // Generate a linkage context for a given runtime ID, storage ID, and list of compiled modules.
    // This will generate the linkage context based on the transitive dependencies of the
    // provided package modules if the package's dependencies are in the data cache, or error
    // otherwise.
    // TODO: It would be great, longer term, to move this to the trait and reuse it.
    fn generate_linkage_context(
        &self,
        runtime_package_id: RuntimePackageId,
        storage_id: PackageStorageId,
        modules: &[CompiledModule],
    ) -> VMResult<LinkageContext> {
        let mut all_dependencies: BTreeSet<AccountAddress> = BTreeSet::new();
        for module in modules {
            for dep in module
                .immediate_dependencies()
                .iter()
                .map(|dep| dep.address())
                .filter(|dep| *dep != &runtime_package_id)
            {
                // If this dependency is in here, its transitive dependencies are, too.
                if all_dependencies.contains(dep) {
                    continue;
                }
                let new_dependencies = self.transitive_dependencies(dep)?;
                all_dependencies.extend(new_dependencies.into_iter());
            }
        }
        // Consider making tehse into VM errors on failure instead.
        assert!(
            !all_dependencies.remove(&storage_id),
            "Found circular dependencies during linkage generation."
        );
        assert!(
            !all_dependencies.contains(&runtime_package_id),
            "Found circular dependencies during linkage generation."
        );
        let linkage_context = LinkageContext::new(
            storage_id,
            all_dependencies
                .into_iter()
                .map(|id| (id, id))
                .chain(vec![(runtime_package_id, storage_id)])
                .collect(),
        );
        Ok(linkage_context)
    }

    fn get_compiled_modules_from_storage(
        &self,
        package_id: &PackageStorageId,
    ) -> VMResult<Vec<CompiledModule>> {
        self.get_compiled_modules(package_id)
    }

    fn runtime(&mut self) -> &mut MoveRuntime {
        &mut self.runtime
    }

    fn storage(&self) -> &InMemoryStorage {
        &self.storage
    }

    fn storage_mut(&mut self) -> &mut InMemoryStorage {
        &mut self.storage
    }
}
