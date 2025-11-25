// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Package Operations
// -------------------------------------------------------------------------------------------------
// These operations sould not be exported beyond the runtime, as they are runtime-internal and
// should not be exposed.

use crate::{
    cache::move_cache::{self, MoveCache, Package, ResolvedPackageResult},
    dbg_println, jit,
    natives::functions::NativeFunctions,
    shared::{logging::expect_no_verification_errors, types::VersionId},
    validation::{validate_package, verification},
};
use move_binary_format::errors::{Location, PartialVMError, VMResult};
use move_core_types::{
    resolver::{ModuleResolver, SerializedPackage},
    vm_status::StatusCode,
};
use move_vm_config::runtime::VMConfig;
use tracing::error;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

// Retrieves a package from the cache, attempting to load it from the data store if
// it is not present.
pub fn resolve_package(
    store: impl ModuleResolver,
    cache: &MoveCache,
    natives: &NativeFunctions,
    package_to_read: VersionId,
) -> VMResult<ResolvedPackageResult> {
    let mut packages = resolve_packages(store, cache, natives, BTreeSet::from([package_to_read]))?;

    if packages.is_empty() {
        return Ok(ResolvedPackageResult::NotFound);
    }

    let Some(pkg) = packages.remove(&package_to_read) else {
        debug_assert!(false, "A different package was loaded than was requested");
        return Err(
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message(
                    "Package not found in loaded cache despite just loading it".to_string(),
                )
                .finish(Location::Package(package_to_read)),
        );
    };

    debug_assert!(
        packages.is_empty(),
        "More than one package was loaded when only one was requested"
    );
    if !packages.is_empty() {
        error!("[VM] More than one package was loaded when only one was requested: {packages:#?}");
        return Err(
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message(
                    "More than one package was loaded when only one was requested".to_string(),
                )
                .finish(Location::Package(package_to_read)),
        );
    }

    Ok(ResolvedPackageResult::Found(pkg))
}

// Retrieves a set of packages from the cache, attempting to load them from the data store if
// they are not present.
pub fn resolve_packages(
    store: impl ModuleResolver,
    cache: &MoveCache,
    natives: &NativeFunctions,
    packages_to_read: BTreeSet<VersionId>,
) -> VMResult<BTreeMap<VersionId, Arc<move_cache::Package>>> {
    dbg_println!("loading {packages_to_read:#?}");
    let allow_loading_failure = true;

    let initial_size = packages_to_read.len();

    let mut cached_packages = BTreeMap::new();
    let mut pkgs_to_cache = BTreeSet::new();

    // Determine what is already in the cache.
    for pkg_id in packages_to_read {
        if let Some(pkg) = cache.cached_package_at(pkg_id) {
            cached_packages.insert(pkg_id, pkg);
        } else {
            pkgs_to_cache.insert(pkg_id);
        }
    }

    // Load and cache anything that wasn't already there.
    // NB: packages can be loaded out of order here (e.g., in parallel) if so desired.
    for pkg in load_and_verify_packages(
        store,
        &cache.vm_config,
        natives,
        allow_loading_failure,
        &pkgs_to_cache,
    )? {
        let pkg = jit_and_cache_package(cache, natives, pkg)?;
        cached_packages.insert(pkg.verified.version_id, pkg);
    }

    // The number of cached packages should be the same as the number of packages provided to
    // us by the linkage context.
    debug_assert!(
        cached_packages.len() == initial_size,
        "Mismatch in number of packages in linkage table and cached packages"
    );
    Ok(cached_packages)
}

// Read the package from the data store, deserialize it, and verify it (internally).
// NB: Does not perform cyclic dependency verification or linkage checking.
pub fn load_and_verify_packages(
    store: impl ModuleResolver,
    vm_config: &VMConfig,
    natives: &NativeFunctions,
    allow_loading_failure: bool,
    packages_to_read: &BTreeSet<VersionId>,
) -> VMResult<Vec<verification::ast::Package>> {
    let packages = packages_to_read.iter().cloned().collect::<Vec<_>>();
    let packages = match load_packages(store, &packages) {
        Ok(packages) => packages,
        Err(err) if allow_loading_failure => return Err(err),
        Err(err) => {
            error!("[VM] Error fetching packages {packages_to_read:?}");
            return Err(expect_no_verification_errors(err));
        }
    };
    // FIXME: should all packages loaded this way be linkage-checked against their defined
    // linkages as well?
    packages
        .into_iter()
        .map(|pkg| validate_package(natives, vm_config, pkg))
        .collect()
}

// Loads a set of packages from the data store, converting any underlying storage errors into VM errors.
// If any package is not found, an error is returned.
// If there is an error loading any package, an error is returned.
// The order of the returned packages matches the order of the provided version ids.
fn load_packages(
    store: impl ModuleResolver,
    ids: &[VersionId],
) -> VMResult<Vec<SerializedPackage>> {
    let pkgs = match store.get_packages(ids) {
        Ok(pkgs) => pkgs
            .into_iter()
            .enumerate()
            .map(|(idx, pkg)| {
                pkg.ok_or_else(|| {
                    PartialVMError::new(StatusCode::LINKER_ERROR)
                        .with_message(format!("Cannot find package {:?} in data cache", ids[idx],))
                        .finish(Location::Package(ids[idx]))
                })
            })
            .collect::<VMResult<Vec<_>>>()?,
        Err(err) => {
            let msg = format!("Unexpected storage error: {:?}", err);
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(msg)
                    .finish(Location::Undefined),
            );
        }
    };

    // Should all be the same length, the the ordering should be preserved.
    debug_assert_eq!(pkgs.len(), ids.len());
    for (pkg, id) in pkgs.iter().zip(ids.iter()) {
        debug_assert_eq!(pkg.version_id, *id);
    }

    Ok(pkgs)
}

// Retrieve a JIT-compiled package from the cache, or compile and add it to the cache.
pub fn jit_package_for_publish(
    cache: &MoveCache,
    natives: &NativeFunctions,
    verified_pkg: verification::ast::Package,
) -> VMResult<Arc<move_cache::Package>> {
    let version_id = verified_pkg.version_id;
    if let Some(pkg) = cache.cached_package_at(version_id) {
        return Ok(pkg);
    }

    let runtime_pkg = jit::translate_package(
        &cache.vm_config,
        &cache.interner,
        natives,
        verified_pkg.clone(),
    )
    .map_err(|err| err.finish(Location::Package(version_id)))?;

    Ok(Arc::new(Package::new(
        verified_pkg.into(),
        runtime_pkg.into(),
    )))
}

// Retrieve a JIT-compiled package from the cache, or compile and add it to the cache.
pub fn jit_and_cache_package(
    cache: &MoveCache,
    natives: &NativeFunctions,
    verified_pkg: verification::ast::Package,
) -> VMResult<Arc<move_cache::Package>> {
    let version_id = verified_pkg.version_id;
    // If the package is already in the cache, return it.
    // This is possible since the cache is shared and may be inserted into concurrently by other
    // VMs working over the same cache.
    if let Some(pkg) = cache.cached_package_at(version_id) {
        return Ok(pkg);
    }

    let runtime_pkg = jit::translate_package(
        &cache.vm_config,
        &cache.interner,
        natives,
        verified_pkg.clone(),
    )
    .map_err(|err| err.finish(Location::Package(version_id)))?;

    cache.add_to_cache(version_id, verified_pkg, runtime_pkg);

    cache.cached_package_at(version_id).ok_or_else(|| {
        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
            .with_message("Package not found in cache after loading".to_string())
            .finish(Location::Package(version_id))
    })
}
