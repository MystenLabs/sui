// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::linkage_context::LinkageContext,
    natives::{extensions::NativeContextExtensions, functions::NativeFunctions},
    on_chain::ast::{PackageStorageId, RuntimePackageId},
    test_utils::{
        gas_schedule::GasStatus, storage::InMemoryStorage, test_store::TestStore,
        vm_test_adapter::VMTestAdapter,
    },
    vm::{vm::VirtualMachine, vm_instance::VirtualMachineExecutionInstance},
};

use move_binary_format::errors::VMResult;
use move_binary_format::file_format::CompiledModule;

use move_core_types::account_address::AccountAddress;
use move_vm_config::runtime::VMConfig;

use std::collections::BTreeSet;

pub struct InMemoryTestAdapter {
    vm: VirtualMachine,
    storage: TestStore,
}

impl InMemoryTestAdapter {
    pub fn new() -> Self {
        let storage = TestStore::new(InMemoryStorage::new());
        let native_functions = NativeFunctions::empty_for_testing().unwrap();
        let vm_config = VMConfig::default();
        let vm = VirtualMachine::new(native_functions, vm_config);
        Self { vm, storage }
    }

    pub fn new_with_vm(vm: VirtualMachine) -> Self {
        let storage = TestStore::new(InMemoryStorage::new());
        Self { vm, storage }
    }

    pub fn new_with_vm_and_storage(vm: VirtualMachine, storage: TestStore) -> Self {
        Self { vm, storage }
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
                .store
                .publish_or_overwrite_module(module_id, module_bytes);
        }
        Ok(())
    }
}

impl VMTestAdapter<TestStore> for InMemoryTestAdapter {
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
        let (changeset, _) = self.vm.publish_package(
            &self.storage,
            &linkage_context,
            runtime_id,
            storage_id,
            modules,
            &mut gas_meter,
        );
        self.storage
            .store
            .apply(changeset?)
            .expect("Failed to apply change set");
        Ok(())
    }

    fn make_vm_instance<'extensions>(
        &self,
        linkage_context: LinkageContext,
    ) -> VMResult<VirtualMachineExecutionInstance<'extensions, &TestStore>> {
        let Self { vm, storage } = self;
        let storage: &TestStore = storage;
        vm.make_instance(storage, linkage_context)
    }

    fn make_vm_instance_with_native_extensions<'extensions>(
        &self,
        linkage_context: LinkageContext,
        native_extensions: NativeContextExtensions<'extensions>,
    ) -> VMResult<VirtualMachineExecutionInstance<'extensions, &TestStore>> {
        let Self { vm, storage } = self;
        vm.make_instance_with_native_extensions(storage, linkage_context, native_extensions)
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
                let new_dependencies = self.storage.transitive_dependencies(dep)?;
                all_dependencies.extend(new_dependencies.into_iter());
            }
        }
        // Consider making tehse into VM errors on failure instead.
        assert!(!all_dependencies.remove(&storage_id),
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
        self.storage.get_compiled_modules(package_id)
    }

    fn vm(&mut self) -> &mut VirtualMachine {
        &mut self.vm
    }

    fn storage(&self) -> &TestStore {
        &self.storage
    }

    fn storage_mut(&mut self) -> &mut TestStore {
        &mut self.storage
    }
}
