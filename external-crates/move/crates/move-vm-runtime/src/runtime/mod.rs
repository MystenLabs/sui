// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::move_cache::MoveCache,
    dbg_println,
    execution::{dispatch_tables::VMDispatchTables, interpreter::locals::BaseHeap, vm::MoveVM},
    jit,
    natives::{extensions::NativeContextExtensions, functions::NativeFunctions},
    shared::{gas::GasMeter, linkage_context::LinkageContext, types::RuntimePackageId},
    try_block,
    validation::{validate_for_publish, validate_for_vm_execution},
};
use move_binary_format::{
    errors::{Location, PartialVMResult, VMResult},
    CompiledModule,
};
use move_core_types::{
    effects::ChangeSet,
    resolver::{MoveResolver, SerializedPackage},
};
use move_vm_config::runtime::VMConfig;
use std::{collections::HashMap, sync::Arc};
use tracing::warn;

// FIXME(cswords): This is only public for testing...
pub mod package_resolution;

pub mod data_cache;
use data_cache::TransactionDataCache;

#[allow(dead_code)]
#[derive(Debug)]
pub struct MoveRuntime {
    /// The VM package cache for the VM, holding currently-loaded packages.
    cache: Arc<MoveCache>,
    /// The native functions the Move VM uses
    natives: Arc<NativeFunctions>,
    /// The Move VM's configuration.
    vm_config: Arc<VMConfig>,
}

impl MoveRuntime {
    pub fn new(natives: NativeFunctions, vm_config: VMConfig) -> Self {
        let natives = Arc::new(natives);
        let vm_config = Arc::new(vm_config);
        let cache = Arc::new(MoveCache::new(natives.clone(), vm_config.clone()));
        Self {
            cache,
            natives,
            vm_config,
        }
    }

    pub fn new_with_default_config(natives: NativeFunctions) -> Self {
        let natives = Arc::new(natives);
        let vm_config = Arc::new(VMConfig::default());
        let cache = Arc::new(MoveCache::new(natives.clone(), vm_config.clone()));
        Self {
            cache,
            natives,
            vm_config,
        }
    }

    /// Retrieive the Move VM Natives associated with the Runtime
    pub fn natives(&self) -> Arc<NativeFunctions> {
        self.natives.clone()
    }

    /// Retrieive the Move VM Config associated with the Runtime
    pub fn vm_config(&self) -> Arc<VMConfig> {
        self.vm_config.clone()
    }

    /// Retrieive the Move Cache associated with the Runtime
    pub fn cache(&self) -> Arc<MoveCache> {
        self.cache.clone()
    }

    /// Makes an Execution Instance for running a Move function invocation.
    /// Note this will hit the VM Cache to construct VTables for that execution, which may block on
    /// cache loading efforts.
    ///
    /// The resuling map of vtables _must_ be closed under the static dependency graph of the root
    /// package w.r.t, to the current linkage context in `data_store`.
    ///
    ///
    /// TODO: Have this hand back a tokio Notify
    pub fn make_vm<'extensions, DataCache: MoveResolver>(
        &self,
        data_cache: DataCache,
        link_context: LinkageContext,
    ) -> VMResult<MoveVM<'extensions>> {
        self.make_vm_with_native_extensions(
            data_cache,
            link_context,
            NativeContextExtensions::default(),
        )
    }

    pub fn make_vm_with_native_extensions<'extensions, DataCache: MoveResolver>(
        &self,
        data_cache: DataCache,
        link_context: LinkageContext,
        native_extensions: NativeContextExtensions<'extensions>,
    ) -> VMResult<MoveVM<'extensions>> {
        let all_packages = link_context.all_packages()?;

        let data_cache = TransactionDataCache::new(data_cache);
        let packages = package_resolution::resolve_packages(
            &self.cache,
            &self.natives,
            &self.vm_config,
            &data_cache,
            &link_context,
            all_packages,
        )?;
        let validation_packages = packages
            .iter()
            .map(|(id, pkg)| (*id, &*pkg.verified))
            .collect();
        validate_for_vm_execution(validation_packages)?;
        let runtime_packages = packages
            .into_values()
            .map(|pkg| (pkg.runtime.runtime_id, Arc::clone(&pkg.runtime)))
            .collect::<HashMap<RuntimePackageId, Arc<jit::execution::ast::Package>>>();

        let virtual_tables = VMDispatchTables::new(runtime_packages)?;

        let base_heap = BaseHeap::new();

        // Called and checked linkage, etc.
        let instance = MoveVM {
            virtual_tables,
            vm_config: self.vm_config.clone(),
            link_context,
            native_extensions,
            base_heap,
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
    /// In case an invariant violation occurs, the provided data cache should be considered
    /// corrupted and discarded; a change set will not be returned.
    pub fn validate_package<DataCache: MoveResolver>(
        &mut self,
        data_cache: DataCache,
        pkg_runtime_id: RuntimePackageId,
        pkg: SerializedPackage,
        _gas_meter: &mut impl GasMeter,
    ) -> (VMResult<ChangeSet>, DataCache) {
        let storage_id = pkg.storage_id;
        dbg_println!("\n\nPublishing module at {storage_id} (=> {pkg_runtime_id})\n\n");
        // TODO: Don't deserialize just for names. Reserialize off the verified ones or something.
        let compiled_modules = match pkg
            .modules
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
        dbg_println!("\n\nGrabbed modules\n\n");

        let mut data_cache = TransactionDataCache::new(data_cache);
        let link_context = LinkageContext::new(
            pkg.storage_id,
            HashMap::from_iter(pkg.linkage_table.clone()),
        );

        // Verify a provided serialized package. This will validate the provided serialized
        // package, including attempting to jit-compile the package and verify linkage with its
        // dependencies in the provided linkage context. This returns the loaded package in the
        // case an `init` function or similar will need to run. This will load the dependencies
        // into the package cache.
        let package = try_block! {
            let dependencies = package_resolution::resolve_packages(
                &self.cache,
                &self.natives,
                &self.vm_config,
                &data_cache,
                &link_context,
                link_context.all_package_dependencies()?,
            )?;
            let dependencies = dependencies.iter().map(|(id, pkg)| (*id, &*pkg.verified)).collect();
            validate_for_publish(
                &self.natives,
                &self.vm_config,
                pkg_runtime_id,
                pkg,
                dependencies
            )
        };
        if let Err(err) = package {
            let data_cache = data_cache.into_remote();
            return (Err(err), data_cache);
        }
        dbg_println!("\n\nVerified package\n\n");

        data_cache.publish_package(storage_id, compiled_modules);
        dbg_println!("\n\nUpdated data cache\n\n");

        let (result, remote) = data_cache.into_effects();
        (result.map_err(|e| e.finish(Location::Undefined)), remote)
    }
}

// TODO: Do this next.
// Let's talk about what this looks like -- and what and when the overlaid cache is used to
// update the main cache.
// struct OverlaidMoveRuntime {}
//
// fn make_overlay_instance(_runtime: MoveRuntime) -> OverlaidMoveRuntime {
//     OverlaidMoveRuntime {}
// }
