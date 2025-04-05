// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::move_cache::MoveCache,
    dbg_println,
    execution::{dispatch_tables::VMDispatchTables, interpreter::locals::BaseHeap, vm::MoveVM},
    jit,
    natives::{extensions::NativeContextExtensions, functions::NativeFunctions},
    shared::{gas::GasMeter, linkage_context::LinkageContext, types::OriginalId},
    validation::{validate_for_publish, validate_for_vm_execution, verification::ast as verif_ast},
};

use move_binary_format::errors::VMResult;
use move_core_types::resolver::{ModuleResolver, SerializedPackage};
use move_vm_config::runtime::VMConfig;

use std::{collections::BTreeMap, sync::Arc};

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
    pub fn make_vm<'extensions, DataCache: ModuleResolver>(
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

    pub fn make_vm_with_native_extensions<'extensions, DataCache: ModuleResolver>(
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
            .map(|pkg| (pkg.runtime.original_id, Arc::clone(&pkg.runtime)))
            .collect::<BTreeMap<OriginalId, Arc<jit::execution::ast::Package>>>();

        let virtual_tables = VMDispatchTables::new(self.vm_config.clone(), runtime_packages)?;

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
    pub fn validate_package<'extensions, DataCache: ModuleResolver>(
        &self,
        data_cache: DataCache,
        original_id: OriginalId,
        pkg: SerializedPackage,
        _gas_meter: &mut impl GasMeter,
        native_extensions: NativeContextExtensions<'extensions>,
    ) -> VMResult<(verif_ast::Package, MoveVM<'extensions>)> {
        let version_id = pkg.storage_id;
        dbg_println!("\n\nPublishing module at {version_id} (=> {original_id})\n\n");

        let data_cache = TransactionDataCache::new(data_cache);
        let link_context = LinkageContext::new(pkg.linkage_table.clone());

        // Verify a provided serialized package. This will validate the provided serialized
        // package, including attempting to jit-compile the package and verify linkage with its
        // dependencies in the provided linkage context. This returns the loaded package in the
        // case an `init` function or similar will need to run. This will load the dependencies
        // into the package cache.
        let pkg_dependencies = package_resolution::resolve_packages(
            &self.cache,
            &self.natives,
            &data_cache,
            &link_context,
            link_context.all_package_dependencies_except(pkg.storage_id)?,
        )?;
        let verified_pkg = {
            let deps = pkg_dependencies
                .iter()
                .map(|(id, pkg)| (*id, &*pkg.verified))
                .collect();
            validate_for_publish(&self.natives, &self.vm_config, original_id, pkg, deps)
        }?;
        dbg_println!("\n\nVerified package\n\n");

        let published_package = package_resolution::jit_package_for_publish(
            &self.cache,
            &self.natives,
            &link_context,
            verified_pkg.clone(),
        )?;

        // Generates  a one-off package for executing `init` functions.
        let runtime_packages = pkg_dependencies
            .into_values()
            .chain([published_package])
            .map(|pkg| (pkg.runtime.original_id, Arc::clone(&pkg.runtime)))
            .collect::<BTreeMap<OriginalId, Arc<jit::execution::ast::Package>>>();

        let virtual_tables = VMDispatchTables::new(self.vm_config.clone(), runtime_packages)?;

        let base_heap = BaseHeap::new();

        // Called and checked linkage, etc.
        let instance = MoveVM {
            virtual_tables,
            vm_config: self.vm_config.clone(),
            link_context,
            native_extensions,
            base_heap,
        };
        Ok((verified_pkg, instance))
    }
}
