// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This is the "orchestrator" of loading a package.
// The package loader is responsible for the management of packages, package loading and caching,
// and publishing packages to the VM.

use crate::{
    jit, natives::functions::NativeFunctions, runtime::telemetry::MoveCacheTelemetry,
    shared::types::VersionId, validation::verification,
};
use move_vm_config::runtime::VMConfig;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Package {
    pub verified: Arc<verification::ast::Package>,
    pub runtime: Arc<jit::execution::ast::Package>,
}

type PackageCache = HashMap<VersionId, Arc<Package>>;

/// The loader for the VM. This is the data structure is used to resolve packages and cache them
/// and their types. This is then used to create the VTables for the VM.
#[derive(Debug)]
pub struct MoveCache {
    pub(crate) natives: Arc<NativeFunctions>,
    pub(crate) vm_config: Arc<VMConfig>,
    pub(crate) package_cache: Arc<RwLock<PackageCache>>,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl MoveCache {
    pub fn new(natives: Arc<NativeFunctions>, vm_config: Arc<VMConfig>) -> Self {
        Self {
            natives,
            vm_config,
            package_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // -------------------------------------------
    // Caching Operations
    // -------------------------------------------

    // TODO: Make this a VM Result
    pub fn add_to_cache(
        &self,
        package_key: VersionId,
        verified: verification::ast::Package,
        runtime: jit::execution::ast::Package,
    ) {
        // NB: We grab a write lock here to ensure that we don't double-insert a package.
        let mut package_cache = self.package_cache.write();

        if package_cache.contains_key(&package_key) {
            return;
        }
        let verified = Arc::new(verified);
        let runtime = Arc::new(runtime);
        let package = Package { verified, runtime };
        package_cache.insert(package_key, Arc::new(package));
    }

    pub fn cached_package_at(&self, package_key: VersionId) -> Option<Arc<Package>> {
        self.package_cache.read().get(&package_key).map(Arc::clone)
    }

    // -------------------------------------------
    // Getters
    // -------------------------------------------

    pub fn package_cache(&self) -> &RwLock<PackageCache> {
        &self.package_cache
    }

    // -------------------------------------------
    // Cache Eviction For Testing
    // -------------------------------------------

    /// For use with unit testing: remove a package, returning `true` if it was present.
    #[cfg(test)]
    pub(crate) fn remove_package(&self, version_id: &VersionId) -> bool {
        self.package_cache.write().remove(version_id).is_some()
    }

    // -------------------------------------------
    // Telemetry Reporting
    // -------------------------------------------

    pub(crate) fn to_cache_telemetry(&self) -> MoveCacheTelemetry {
        // Lock the package_cache for reading.
        let package_cache = self.package_cache.read();

        let mut package_cache_count: u64 = 0;
        let mut total_arena_size: u64 = 0;
        let mut module_count: u64 = 0;
        let mut function_count: u64 = 0;
        let mut type_count: u64 = 0;

        // Iterate over each package.
        for (_version_id, package_arc) in package_cache.iter() {
            package_cache_count += 1;

            // Dereference the runtime package.
            let runtime_pkg = &package_arc.runtime;

            // Sum up the number of modules.
            module_count += runtime_pkg.loaded_modules.len() as u64;

            // Sum up the number of functions and types.
            function_count += runtime_pkg.vtable.functions.len() as u64;
            type_count += runtime_pkg.vtable.types.len() as u64;

            // Sum the memory usage reported by the arena.
            total_arena_size += runtime_pkg.package_arena.allocated_bytes() as u64;
        }

        MoveCacheTelemetry {
            package_cache_count,
            total_arena_size,
            module_count,
            function_count,
            type_count,
        }
    }
}

impl Package {
    pub(crate) fn new(
        verified: Arc<verification::ast::Package>,
        runtime: Arc<jit::execution::ast::Package>,
    ) -> Self {
        Self { verified, runtime }
    }

    /// Used for testing that the correct number of types are loaded
    #[allow(dead_code)]
    pub(crate) fn loaded_types_len(&self) -> usize {
        self.runtime.vtable.types.len()
    }
}

// -------------------------------------------------------------------------------------------------
// Other Impls
// -------------------------------------------------------------------------------------------------

impl Clone for MoveCache {
    /// Makes a shallow copy of the VM Cache by cloning all the internal `Arc`s.
    fn clone(&self) -> Self {
        let MoveCache {
            natives,
            vm_config,
            package_cache,
        } = self;
        Self {
            natives: natives.clone(),
            vm_config: vm_config.clone(),
            package_cache: package_cache.clone(),
        }
    }
}
