// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Package Operations
// -------------------------------------------------------------------------------------------------
// These operations sould not be exported beyond the runtime, as they are runtime-internal and
// should not be exposed.

use crate::{
    cache::move_cache::{self, MoveCache, Package},
    dbg_println, jit,
    natives::functions::NativeFunctions,
    record_time,
    runtime::telemetry::TransactionTelemetryContext,
    shared::{
        data_store::DataStore, linkage_context::LinkageContext,
        logging::expect_no_verification_errors, types::VersionId,
    },
    validation::{validate_package, verification},
};
use move_binary_format::errors::{Location, PartialVMError, VMResult};
use move_core_types::vm_status::StatusCode;
use move_vm_config::runtime::VMConfig;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

// Retrieves a set of packages from the cache, attempting to load them from the data store if
// they are not present.
pub(crate) fn resolve_packages(
    telemetry: &mut TransactionTelemetryContext,
    cache: &MoveCache,
    natives: &NativeFunctions,
    data_store: &impl DataStore,
    link_context: &LinkageContext,
    packages_to_read: BTreeSet<VersionId>,
) -> VMResult<BTreeMap<VersionId, Arc<move_cache::Package>>> {
    dbg_println!("loading {packages_to_read:#?} in linkage context {link_context:#?}");
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

    if !pkgs_to_cache.is_empty() {
        // Load and cache anything that wasn't already there.
        // NB: packages can be loaded out of order here (e.g., in parallel) if so desired.
        for pkg in load_and_verify_packages(
            telemetry,
            &cache.vm_config,
            natives,
            data_store,
            allow_loading_failure,
            &pkgs_to_cache,
        )? {
            let pkg = jit_and_cache_package(telemetry, cache, natives, link_context, pkg)?;
            cached_packages.insert(pkg.verified.version_id, pkg);
        }
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
pub(crate) fn load_and_verify_packages(
    telemetry: &mut TransactionTelemetryContext,
    vm_config: &VMConfig,
    natives: &NativeFunctions,
    data_store: &impl DataStore,
    allow_loading_failure: bool,
    packages_to_read: &BTreeSet<VersionId>,
) -> VMResult<Vec<verification::ast::Package>> {
    let packages = packages_to_read.iter().cloned().collect::<Vec<_>>();
    let load_count = packages.len() as u64;
    debug_assert!(load_count > 0);
    let packages = record_time!(LOAD ; load_count ; telemetry => {
        match data_store.load_packages(&packages) {
            Ok(packages) => Ok(packages),
            Err(err) if allow_loading_failure => Err(err),
            Err(err) => {
                tracing::error!("[VM] Error fetching packages {packages_to_read:?}");
                Err(expect_no_verification_errors(err))
            }
        }
    })?;
    // FIXME: should all packages loaded this way be linkage-checked against their defined
    // linkages as well?
    packages
        .into_iter()
        .map(|pkg| {
            record_time!(VALIDATION ; 1; telemetry => {
             validate_package(natives, vm_config, pkg)
            })
        })
        .collect()
}

// Retrieve a JIT-compiled package from the cache, or compile and add it to the cache.
pub(crate) fn jit_package_for_publish(
    telemetry: &mut TransactionTelemetryContext,
    cache: &MoveCache,
    natives: &NativeFunctions,
    link_context: &LinkageContext,
    verified_pkg: verification::ast::Package,
) -> VMResult<Arc<move_cache::Package>> {
    let version_id = verified_pkg.version_id;
    if cache.cached_package_at(version_id).is_some() {
        return Ok(cache.cached_package_at(version_id).unwrap());
    }

    let runtime_pkg = record_time!(JIT ; 1; telemetry => {
         jit::translate_package(
            &cache.vm_config,
            natives,
            link_context,
            verified_pkg.clone(),
        )
        .map_err(|err| err.finish(Location::Undefined))
    })?;
    Ok(Arc::new(Package::new(
        verified_pkg.into(),
        runtime_pkg.into(),
    )))
}

// Retrieve a JIT-compiled package from the cache, or compile and add it to the cache.
pub(crate) fn jit_and_cache_package(
    telemetry: &mut TransactionTelemetryContext,
    cache: &MoveCache,
    natives: &NativeFunctions,
    link_context: &LinkageContext,
    verified_pkg: verification::ast::Package,
) -> VMResult<Arc<move_cache::Package>> {
    let version_id = verified_pkg.version_id;
    // If the package is already in the cache, return it.
    // This is possible since the cache is shared and may be inserted into concurrently by other
    // VMs working over the same cache.
    if cache.cached_package_at(version_id).is_some() {
        return Ok(cache.cached_package_at(version_id).unwrap());
    }

    let runtime_pkg = record_time!(JIT ; 1; telemetry => {
         jit::translate_package(
            &cache.vm_config,
            natives,
            link_context,
            verified_pkg.clone(),
        )
        .map_err(|err| err.finish(Location::Undefined))
    })?;

    cache.add_to_cache(version_id, verified_pkg, runtime_pkg);

    cache.cached_package_at(version_id).ok_or_else(|| {
        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
            .with_message("Package not found in cache after loading".to_string())
            .finish(Location::Undefined)
    })
}
