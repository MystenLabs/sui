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
        constants::VIRTUAL_DISPATCH_TABLE_CACHE_SIZE,
        linkage_context::LinkageHash,
        types::{OriginalId, VersionId},
    },
    validation::verification,
};

use dashmap::DashMap;
use move_vm_config::runtime::VMConfig;
use quick_cache::sync::Cache as QCache;

use std::{collections::BTreeMap, sync::Arc};

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Package {
    pub verified: Arc<verification::ast::Package>,
    pub runtime: Arc<jit::execution::ast::Package>,
}

type PackageCache = DashMap<VersionId, Arc<Package>>;

/// The loader for the VM. This is the data structure is used to resolve packages and cache them
/// and their types. This is then used to create the VTables for the VM.
#[derive(Debug)]
pub struct MoveCache {
    pub(crate) vm_config: Arc<VMConfig>,
    pub(crate) package_cache: Arc<PackageCache>,
    pub(crate) linkage_vtables: Arc<QCache<LinkageHash, VMDispatchTables>>,
    pub(crate) interner: Arc<IdentifierInterner>,
    /// Pinned packages whose `Arc<Package>` is held for the lifetime of this cache, keyed by
    /// `OriginalId`. The JIT translator consults this set to rewrite cross-package calls into
    /// these packages as direct pointers; soundness rests on the fact that these `Arc<Package>`s
    /// outlive every user package compiled against them in this cache.
    pub(crate) system_packages: Arc<BTreeMap<OriginalId, Arc<Package>>>,
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
            package_cache: Arc::new(DashMap::new()),
            interner: Arc::new(IdentifierInterner::new()),
            linkage_vtables: Arc::new(QCache::new(VIRTUAL_DISPATCH_TABLE_CACHE_SIZE)),
            system_packages: Arc::new(BTreeMap::new()),
        }
    }

    pub fn system_packages(&self) -> &BTreeMap<OriginalId, Arc<Package>> {
        &self.system_packages
    }

    /// Register a pinned system package keyed by its `OriginalId`. Returns `true` if newly
    /// inserted, `false` if a package was already registered at that id (which the system-pkg
    /// install loop hits naturally when `resolve_packages` returns previously-installed
    /// siblings as cache hits — caller decides whether to log).
    ///
    /// Only callable during `MoveRuntime` construction, when the caller holds the unique strong
    /// reference to this `MoveCache` (via `Arc::get_mut` on `runtime.cache`). We use
    /// `Arc::get_mut` on the inner map (rather than `Arc::make_mut`) so that CoW cloning can't
    /// silently split the map — if the inner `Arc` were somehow shared, we log and refuse the
    /// insert rather than diverging the JIT translator's read view from the write view.
    pub(crate) fn add_system_package(&mut self, pkg: Arc<Package>) -> bool {
        use std::collections::btree_map::Entry;
        let id = pkg.runtime.original_id;
        let Some(map) = Arc::get_mut(&mut self.system_packages) else {
            tracing::error!(
                %id,
                "add_system_package: inner system_packages Arc is shared; refusing to install"
            );
            debug_assert!(
                false,
                "add_system_package called with shared system_packages Arc"
            );
            return false;
        };
        match map.entry(id) {
            Entry::Occupied(_) => false,
            Entry::Vacant(slot) => {
                slot.insert(pkg);
                true
            }
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
        use dashmap::mapref::entry::Entry;
        // Grab the entry at the top, so we can figure out which flag to return while holding the
        // lock on the shard of dashmap we are modifying (so this does not change out from under us
        // mid-write).
        let entry = self.package_cache.entry(package_key);
        match entry {
            Entry::Occupied(_) => {
                // Package is already present.
                false
            }
            Entry::Vacant(vacant_entry) => {
                let verified = Arc::new(verified);
                let runtime = Arc::new(runtime);
                let package = Package { verified, runtime };
                vacant_entry.insert(Arc::new(package));
                true
            }
        }
    }

    /// Get a package from the cache, if it is present.
    /// If not present, returns `None`.
    pub(crate) fn cached_package_at(&self, package_key: VersionId) -> Option<Arc<Package>> {
        self.package_cache
            .get(&package_key)
            .as_deref()
            .map(Arc::clone)
    }

    // -------------------------------------------
    // Linkage VTable Caching Operations
    // -------------------------------------------

    /// Add linkage tables to the cache for a given linkage context.
    ///
    /// Returns `true` if the tables were newly inserted, `false` if they were already present.
    pub(crate) fn add_linkage_tables_to_cache(
        &self,
        linkage_key: LinkageHash,
        vtables: VMDispatchTables,
    ) -> bool {
        let mut inserted = false;
        let _insert_result: Result<VMDispatchTables, ()> = self
            .linkage_vtables
            .get_or_insert_with::<_, ()>(&linkage_key, || {
                inserted = true;
                Ok(vtables)
            });
        inserted
    }

    /// Clear all cached linkage tables.
    pub(crate) fn drop_all_cached_linkage_tables(&self) {
        self.linkage_vtables.clear();
    }

    /// Get cached linkage tables for a given linkage context, if present, and updates the LRU
    /// stats. If not present, returns `None`.
    pub(crate) fn cached_linkage_tables_at(
        &self,
        linkage_key: &LinkageHash,
    ) -> Option<VMDispatchTables> {
        // NB: QuickCache returns a cloned value. This means we perform a deep clone of the
        // VMDispatchTables, which is mostly Arc pointers, so this is not too expensive.
        self.linkage_vtables.get(linkage_key)
    }

    // -------------------------------------------
    // Getters
    // -------------------------------------------

    pub fn package_cache(&self) -> &PackageCache {
        &self.package_cache
    }

    // -------------------------------------------
    // Cache Eviction For Testing
    // -------------------------------------------

    /// For use with unit testing: remove a package, returning `true` if it was present.
    #[cfg(test)]
    pub(crate) fn remove_package(&self, version_id: &VersionId) -> bool {
        self.package_cache.remove(version_id).is_some()
    }

    // -------------------------------------------
    // Telemetry Reporting
    // -------------------------------------------

    /// Grab telemetry information about the current state of the Move Cache.
    /// Note this may change as it is being computed, due to concurrent access. We do not care, as
    /// telemetry is best-effort.
    pub(crate) fn to_cache_telemetry(&self) -> MoveCacheTelemetry {
        let mut package_cache_count: u64 = 0;
        let mut total_arena_size: u64 = 0;
        let mut module_count: u64 = 0;
        let mut function_count: u64 = 0;
        let mut type_count: u64 = 0;

        // Iterate over each package.
        for entry in self.package_cache.iter() {
            let package_arc = entry.value();
            package_cache_count = package_cache_count.saturating_add(1);

            // Dereference the runtime package.
            let runtime_pkg = &package_arc.runtime;

            // Sum up the number of modules.
            module_count = module_count.saturating_add(runtime_pkg.loaded_modules.len() as u64);

            // Sum up the number of functions and types.
            function_count =
                function_count.saturating_add(runtime_pkg.vtable.functions.len() as u64);
            type_count = type_count.saturating_add(runtime_pkg.vtable.types.len() as u64);

            // Sum the memory usage reported by the arena.
            total_arena_size =
                total_arena_size.saturating_add(runtime_pkg.package_arena.allocated_bytes() as u64);
        }

        let interner_size: u64 = self.interner.size() as u64;

        let vtable_cache_count = self.linkage_vtables.len() as u64;
        let vtable_cache_hits = self.linkage_vtables.hits();
        let vtable_cache_misses = self.linkage_vtables.misses();

        MoveCacheTelemetry {
            package_cache_count,
            total_arena_size,
            module_count,
            function_count,
            type_count,
            interner_size,
            vtable_cache_count,
            vtable_cache_hits,
            vtable_cache_misses,
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
            system_packages,
        } = self;
        Self {
            vm_config: Arc::clone(vm_config),
            package_cache: Arc::clone(package_cache),
            interner: Arc::clone(interner),
            linkage_vtables: Arc::clone(linkage_vtables),
            system_packages: Arc::clone(system_packages),
        }
    }
}
