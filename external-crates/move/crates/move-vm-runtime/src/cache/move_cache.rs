// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This is the "orchestrator" of loading a package.
// The package loader is responsible for the management of packages, package loading and caching,
// and publishing packages to the VM.

use crate::{
    cache::identifier_interner::IdentifierInterner,
    execution::dispatch_tables::VMDispatchTables,
    jit,
    runtime::telemetry::MoveCacheTelemetry,
    shared::{
        constants::VIRTUAL_DISPATCH_TABLE_LRU_SIZE, linkage_context::LinkageHash, types::VersionId,
    },
    validation::verification,
};

use lru::LruCache;
use move_vm_config::runtime::VMConfig;
use parking_lot::RwLock;

use std::{collections::HashMap, num::NonZero, sync::Arc};

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
    pub(crate) vm_config: Arc<VMConfig>,
    pub(crate) package_cache: Arc<RwLock<PackageCache>>,
    pub(crate) linkage_vtables: Arc<RwLock<LruCache<LinkageHash, VMDispatchTables>>>,
    pub(crate) interner: Arc<IdentifierInterner>,
}

#[derive(Debug)]
pub enum ResolvedPackageResult {
    /// The package was found, loaded, and cached.
    Found(Arc<Package>),
    /// The package was not found.
    NotFound,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl MoveCache {
    pub fn new(vm_config: Arc<VMConfig>) -> Self {
        Self {
            vm_config,
            package_cache: Arc::new(RwLock::new(HashMap::new())),
            interner: Arc::new(IdentifierInterner::new()),
            linkage_vtables: Arc::new(RwLock::new(LruCache::new(
                NonZero::new(VIRTUAL_DISPATCH_TABLE_LRU_SIZE).unwrap(),
            ))),
        }
    }

    // -------------------------------------------
    // Package Caching Operations
    // -------------------------------------------

    /// Add a package to the cache. If the package is already present, this is a no-op.
    ///
    /// Important: it is not an error if a package is already present when we go to insert a
    /// package. This can happen in concurrent scenarios where multiple threads attempt to load the
    /// same package at the same time -- they could both check that the package is not yet cached
    /// with `cached_package_at`, and then both proceed to independenctly load and verify the
    /// package. As the write lock is not held between the `cached_package_at` call and the call to
    /// `add_to_cache` the package could be inserted by another thread in the meantime.
    ///
    /// Returns `true` if the package was newly inserted, `false` if it was already present.
    /// NB: in a parallel scenario the result of this function is not guaranteed to be
    /// deterministic.
    pub(crate) fn add_package_to_cache(
        &self,
        package_key: VersionId,
        verified: verification::ast::Package,
        runtime: jit::execution::ast::Package,
    ) -> bool {
        // NB: We grab a write lock here to ensure that we don't double-insert a package.
        let mut package_cache = self.package_cache.write();

        // Check if the package is already present, and if so, return false early to avoid
        // re-inserting the already-cached package.
        if package_cache.contains_key(&package_key) {
            return false;
        }
        let verified = Arc::new(verified);
        let runtime = Arc::new(runtime);
        let package = Package { verified, runtime };
        package_cache.insert(package_key, Arc::new(package));
        true
    }

    /// Get a package from the cache, if it is present.
    /// If not present, returns `None`.
    pub(crate) fn cached_package_at(&self, package_key: VersionId) -> Option<Arc<Package>> {
        self.package_cache.read().get(&package_key).map(Arc::clone)
    }

    // -------------------------------------------
    // Linkage Caching Operations
    // -------------------------------------------

    /// Add linkage tables to the cache for a given linkage context.
    ///
    /// Returns `true` if the package was newly inserted, `false` if it was already present.
    /// NB: in a parallel scenario the result of this function is not guaranteed to be
    /// deterministic.
    pub(crate) fn add_linkage_tables_to_cache(
        &self,
        linkage_key: LinkageHash,
        vtables: VMDispatchTables,
    ) -> bool {
        let mut linkage_vtables = self.linkage_vtables.write();
        let prev = linkage_vtables.put(linkage_key, vtables);
        prev.is_none()
    }

    /// Get cached linkage tables for a given linkage context, if present, and updates the LRU
    /// stats. If not present, returns `None`.
    pub(crate) fn cached_linkage_tables_at(
        &self,
        linkage_key: &LinkageHash,
    ) -> Option<VMDispatchTables> {
        // We have to grab this as mutable because LRU cache updates the internal state on get.
        let mut linkage_vtables = self.linkage_vtables.write();
        linkage_vtables.get(linkage_key).cloned()
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

        let interner_size: u64 = self.interner.size() as u64;

        let vtable_lru_count = self.linkage_vtables.read().len() as u64;

        MoveCacheTelemetry {
            package_cache_count,
            total_arena_size,
            module_count,
            function_count,
            type_count,
            interner_size,
            vtable_lru_count,
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
            vm_config,
            package_cache,
            interner,
            linkage_vtables,
        } = self;
        Self {
            vm_config: Arc::clone(vm_config),
            package_cache: Arc::clone(package_cache),
            interner: Arc::clone(interner),
            linkage_vtables: Arc::clone(linkage_vtables),
        }
    }
}
