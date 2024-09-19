// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::vm_cache::VMCache,
    natives::extensions::NativeContextExtensions,
    natives::functions::NativeFunctions,
    on_chain::{ast::PackageStorageId, data_cache::TransactionDataCache},
    vm::vm_instance::VirtualMachineExecutionInstance,
};

use move_binary_format::{
    errors::{Location, PartialVMResult, VMResult},
    CompiledModule,
};
use move_core_types::{effects::ChangeSet, resolver::MoveResolver};
use move_vm_config::runtime::VMConfig;
use move_vm_types::gas::GasMeter;
use tracing::warn;

use std::sync::Arc;

#[derive(Debug)]
pub struct VirtualMachine {
    /// The VM package cache for the VM, holding currently-loaded packages.
    cache: Arc<VMCache>,
    /// The native functions the Move VM uses
    natives: Arc<NativeFunctions>,
    /// The Move VM's configuration.
    vm_config: Arc<VMConfig>,
}

impl VirtualMachine {
    pub fn new(natives: NativeFunctions, vm_config: VMConfig) -> Self {
        let natives = Arc::new(natives);
        let vm_config = Arc::new(vm_config);
        let cache = Arc::new(VMCache::new(natives.clone(), vm_config.clone()));
        Self {
            cache,
            natives,
            vm_config,
        }
    }

    pub fn new_with_default_config(natives: NativeFunctions) -> Self {
        let natives = Arc::new(natives);
        let vm_config = Arc::new(VMConfig::default());
        let cache = Arc::new(VMCache::new(natives.clone(), vm_config.clone()));
        Self {
            cache,
            natives,
            vm_config,
        }
    }

    /// Makes an Execution Instance for running a Move function invocation.
    /// Note this will hit the VM Cache to construct VTables for that execution, which may block on
    /// cache loading efforts.
    /// TODO: Have this hand back a tokio Notify
    pub fn make_instance<'extensions, DataCache: MoveResolver>(
        &mut self,
        remote: DataCache,
    ) -> VMResult<VirtualMachineExecutionInstance<'extensions, DataCache>> {
        let data_cache = TransactionDataCache::new(remote);
        let virtual_tables = self.cache.generate_runtime_vtables(&data_cache)?;
        // Called and checked linkage, etc.
        let instance = VirtualMachineExecutionInstance {
            virtual_tables,
            vm_cache: self.cache.clone(),
            vm_config: self.vm_config.clone(),
            data_cache,
            native_extensions: NativeContextExtensions::default(),
        };
        Ok(instance)
    }

    /// Publish a package.
    ///
    /// This loads and validates the package against the VM cache and writes out publication
    /// effects to the provided data cache. The VM cache is not updated with the package, however.
    ///
    /// The Move VM MUST return a user error, i.e., an error that's not an invariant violation, if
    /// any module fails to deserialize or verify (see the full list of  failing conditions in the
    /// `publish_module` API). The publishing of the package is an all-or-nothing action: either
    /// all modules are published to the data store or none is.
    ///
    /// Similar to the `publish_module` API, the Move VM should not be able to produce other user
    /// errors. Besides, no user input should cause the Move VM to return an invariant violation.
    ///
    /// In case an invariant violation occurs, the provided data cache VM instance should be
    /// considered corrupted and discarded; a change set will not be returned.
    pub fn publish_package<DataCache: MoveResolver>(
        &mut self,
        data_cache: DataCache,
        package_id: PackageStorageId,
        package: Vec<Vec<u8>>,
        _gas_meter: &mut impl GasMeter,
    ) -> (VMResult<ChangeSet>, DataCache) {
        println!("\n\nPublishing module at {package_id}\n\n");
        let compiled_modules = match package
            .iter()
            .map(|blob| {
                CompiledModule::deserialize_with_config(blob, &self.vm_config.binary_config)
                    .map(|m| (m.self_id().name().to_owned(), blob.clone()))
            })
            .collect::<PartialVMResult<Vec<_>>>()
        {
            Ok(modules) => modules,
            Err(err) => {
                warn!("[VM] module deserialization failed {:?}", err);
                return (Err(err.finish(Location::Undefined)), data_cache);
            }
        };
        println!("\n\nGrabbed modules\n\n");

        let mut data_cache = TransactionDataCache::new(data_cache);
        let _package =
            match self
                .cache
                .verify_package_for_publication(package, &data_cache, package_id)
            {
                Ok(package) => package,
                Err(err) => {
                    let data_cache = data_cache.into_remote();
                    return (Err(err), data_cache);
                }
            };
        println!("\n\nVerified package\n\n");

        data_cache.publish_package(package_id, compiled_modules);
        println!("\n\nUpdated data cache\n\n");

        let (result, remote) = data_cache.into_effects();
        (result.map_err(|e| e.finish(Location::Undefined)), remote)
    }
}
