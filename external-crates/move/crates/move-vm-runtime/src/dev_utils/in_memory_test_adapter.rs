// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dev_utils::{
        gas_schedule::GasStatus,
        storage::{InMemoryStorage, StoredPackage},
        vm_test_adapter::VMTestAdapter,
    },
    execution::vm::MoveVM,
    natives::{extensions::NativeContextExtensions, functions::NativeFunctions},
    runtime::{telemetry::MoveRuntimeTelemetry, MoveRuntime},
    shared::{
        linkage_context::LinkageContext,
        types::{DefiningTypeId, OriginalId},
    },
    validation::verification::ast as verif_ast,
};
use move_binary_format::errors::{Location, PartialVMError, VMResult};
use move_binary_format::file_format::CompiledModule;
use move_core_types::{
    account_address::AccountAddress,
    resolver::{ModuleResolver, SerializedPackage},
    vm_status::StatusCode,
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

    pub fn insert_package_into_storage(&mut self, pkg: StoredPackage) {
        self.storage.publish_package(pkg);
    }

    pub fn get_package(&self, original_id: &OriginalId) -> VMResult<SerializedPackage> {
        let Ok([Some(pkg)]) = self.storage.get_packages_static([*original_id]) else {
            return Err(PartialVMError::new(StatusCode::LINKER_ERROR)
                .with_message(format!(
                    "Cannot find package {:?} in data cache",
                    original_id
                ))
                .finish(Location::Undefined));
        };
        Ok(pkg)
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
            for module in pkg.modules.values() {
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
    fn verify_package<'extensions>(
        &mut self,
        original_id: OriginalId,
        package: SerializedPackage,
    ) -> VMResult<(verif_ast::Package, MoveVM<'extensions>)> {
        let Some(version_id) = package.linkage_table.get(&original_id).cloned() else {
            // TODO: VM error instead?
            panic!("Did not find runtime ID {original_id} in linkage context.");
        };
        assert_eq!(version_id, package.storage_id);
        let mut gas_meter = GasStatus::new_unmetered();
        self.runtime.validate_package(
            &self.storage,
            original_id,
            package.clone(),
            &mut gas_meter,
            NativeContextExtensions::default(),
        )
    }

    fn publish_verified_package(
        &mut self,
        original_id: OriginalId,
        package: verif_ast::Package,
    ) -> VMResult<()> {
        let Some(version_id) = package.linkage_table.get(&original_id).cloned() else {
            // TODO: VM error instead?
            panic!("Did not find runtime ID {original_id} in linkage context.");
        };
        assert!(version_id == package.version_id);
        self.storage
            .publish_package(StoredPackage::from_verified_package(package));
        Ok(())
    }

    fn make_vm<'extensions>(
        &self,
        linkage_context: LinkageContext,
    ) -> VMResult<MoveVM<'extensions>> {
        let Self { runtime, storage } = self;
        let storage: &InMemoryStorage = storage;
        runtime.make_vm(storage, linkage_context)
    }

    fn make_vm_with_native_extensions<'extensions>(
        &self,
        linkage_context: LinkageContext,
        native_extensions: NativeContextExtensions<'extensions>,
    ) -> VMResult<MoveVM<'extensions>> {
        let Self { runtime, storage } = self;
        runtime.make_vm_with_native_extensions(storage, linkage_context, native_extensions)
    }

    fn get_telemetry_report(&self) -> MoveRuntimeTelemetry {
        self.runtime.get_telemetry_report()
    }

    // Generate a linkage context for a given runtime ID, storage ID, and list of compiled modules.
    // This will generate the linkage context based on the transitive dependencies of the
    // provided package modules if the package's dependencies are in the data cache, or error
    // otherwise.
    // TODO: It would be great, longer term, to move this to the trait and reuse it.
    fn generate_linkage_context(
        &self,
        original_id: OriginalId,
        version_id: DefiningTypeId,
        modules: &[CompiledModule],
    ) -> VMResult<LinkageContext> {
        let mut all_dependencies: BTreeSet<AccountAddress> = BTreeSet::new();
        for module in modules {
            for dep in module
                .immediate_dependencies()
                .iter()
                .map(|dep| dep.address())
                .filter(|dep| *dep != &original_id)
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
            !all_dependencies.remove(&version_id),
            "Found circular dependencies during linkage generation."
        );
        assert!(
            !all_dependencies.contains(&original_id),
            "Found circular dependencies during linkage generation."
        );
        let linkage_context = LinkageContext::new(
            all_dependencies
                .into_iter()
                .map(|id| (id, id))
                .chain(vec![(original_id, version_id)])
                .collect(),
        );
        Ok(linkage_context)
    }

    fn get_package_from_store(&self, version_id: &DefiningTypeId) -> VMResult<SerializedPackage> {
        self.get_package(version_id)
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
