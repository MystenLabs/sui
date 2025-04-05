// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Contains the logic for linkag and cyclic checks for packages. These checks are always done with
// respect to a specific linkage context (i.e., a fixed mapping of package -> specific version to
// be used for that package).
//
// The checks are done in the context of a set of packages that are already loaded in the cache,
// with the exception of possibly the root package in the case of package publication.
//
// NB: this process is fallible due to relinking! If a package is loaded with a different set of
// dependencies fail the linkage checks in this module.

use crate::{
    shared::types::{OriginalId, VersionId},
    validation::verification::ast::{Module, Package},
};
use move_binary_format::{
    errors::{Location, PartialVMError, VMResult},
    CompiledModule,
};
use move_bytecode_verifier::{cyclic_dependencies, dependencies};
use move_core_types::vm_status::StatusCode;
use std::collections::{BTreeMap, HashMap};

/// Verifies that all packages in the provided map have valid linkage and no cyclic dependencies
/// between them.
pub fn verify_linkage_and_cyclic_checks(
    cached_packages: &BTreeMap<VersionId, &Package>,
) -> VMResult<()> {
    let relocation_map: HashMap<OriginalId, VersionId> = cached_packages
        .iter()
        .map(|(k, v)| {
            debug_assert!(k == &v.version_id);
            (v.original_id, v.version_id)
        })
        .collect();
    for package in cached_packages.values() {
        let package_modules = package.as_modules().into_iter().collect::<Vec<_>>();
        verify_package_valid_linkage(&package_modules, cached_packages, &relocation_map)?;
        verify_package_no_cyclic_relationships(&package_modules, cached_packages, &relocation_map)?;
    }

    Ok(())
}

/// Does the same as `verify_linkage_and_cyclic_checks` however it special-cases the package that
/// is being published so that we can verify that the package can be published before adding it to
/// the cache (i.e., that at least in the current linking context the package is valid w.r.t. its
/// dependencies).
pub(crate) fn verify_linkage_and_cyclic_checks_for_publication(
    package_to_publish: &Package,
    cached_packages: &BTreeMap<VersionId, &Package>,
) -> VMResult<()> {
    let relocation_map: HashMap<OriginalId, VersionId> = cached_packages
        .iter()
        .map(|(k, v)| {
            debug_assert!(k == &v.version_id);
            (v.original_id, v.version_id)
        })
        .chain(std::iter::once((
            package_to_publish.original_id,
            package_to_publish.original_id,
        )))
        .collect();

    // Verify the dependencies of the package to publish.
    for package in cached_packages.values() {
        let package_modules = package.as_modules().into_iter().collect::<Vec<_>>();
        verify_package_valid_linkage(&package_modules, cached_packages, &relocation_map)?;
        verify_package_no_cyclic_relationships(&package_modules, cached_packages, &relocation_map)?;
    }

    // Now verify the package to publish
    let package_modules = package_to_publish
        .as_modules()
        .into_iter()
        .collect::<Vec<_>>();
    verify_package_valid_linkage(&package_modules, cached_packages, &relocation_map)?;
    verify_package_no_cyclic_relationships(&package_modules, cached_packages, &relocation_map)?;

    Ok(())
}

/// NB: In all cases it is assume the `package` is in the `relocation_map`. In the case of
/// publication it will simply be a mapping of the package's original package ID to itself (since
/// they are the same for publication).
fn verify_package_no_cyclic_relationships(
    package: &[&Module],
    cached_packages: &BTreeMap<VersionId, &Package>,
    relocation_map: &HashMap<VersionId, OriginalId>,
) -> VMResult<()> {
    let (module, bundle_verified) = if package.len() == 1 {
        (&package[0], BTreeMap::new())
    } else {
        let module = &package[0];
        let module_map = package
            .iter()
            .skip(1)
            .map(|m| (m.value.self_id(), m))
            .collect();
        (module, module_map)
    };

    cyclic_dependencies::verify_module(&module.value, |runtime_module_id| {
        let module = if let Some(bundled) = bundle_verified.get(runtime_module_id) {
            Some(**bundled)
        } else {
            let storage_id = relocation_map
                .get(runtime_module_id.address())
                .ok_or_else(|| PartialVMError::new(StatusCode::MISSING_DEPENDENCY))?;
            cached_packages
                .get(storage_id)
                .and_then(|p| p.modules.get(&runtime_module_id.to_owned()))
        };

        module
            .map(|m| m.value.immediate_dependencies())
            .ok_or_else(|| PartialVMError::new(StatusCode::MISSING_DEPENDENCY))
    })?;

    Ok(())
}

// Given the package, the cached packages, and the relocation map, this function verifies that
// all modules in the provided package have valid linkage to their dependencies.
fn verify_package_valid_linkage(
    package: &[&Module],
    cached_packages: &BTreeMap<VersionId, &Package>,
    relocation_map: &HashMap<VersionId, OriginalId>,
) -> VMResult<()> {
    let package_module_map = package
        .iter()
        .map(|m| (m.value.self_id(), m))
        .collect::<BTreeMap<_, _>>();
    for m in package {
        let imm_deps = m.value.immediate_dependencies();
        let module_deps = imm_deps
            .iter()
            .map(|module_id| {
                if let Some(m) = package_module_map.get(module_id) {
                    Ok(&m.value)
                } else {
                    let package = relocation_map
                        .get(module_id.address())
                        .and_then(|package_id| cached_packages.get(package_id))
                        .ok_or_else(|| {
                            PartialVMError::new(StatusCode::MISSING_DEPENDENCY)
                                .finish(Location::Undefined)
                        })?;
                    let module = package.modules.get(&module_id.to_owned()).ok_or_else(|| {
                        PartialVMError::new(StatusCode::MISSING_DEPENDENCY)
                            .finish(Location::Undefined)
                    })?;
                    Ok(&module.value)
                }
            })
            .collect::<VMResult<Vec<&CompiledModule>>>()?;
        dependencies::verify_module(&m.value, module_deps)?;
    }
    Ok(())
}
