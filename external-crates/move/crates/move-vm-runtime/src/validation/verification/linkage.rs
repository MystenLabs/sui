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
    CompiledModule,
    errors::{Location, VMResult},
    partial_vm_error,
};
use move_bytecode_verifier::{
    cyclic_dependencies,
    dependencies::{self, DependencyIndex, IndexedModule},
};
use move_core_types::language_storage::ModuleId;
use quick_cache::unsync::Cache as QCache;
use std::{
    collections::{BTreeMap, HashMap},
    rc::Rc,
};
use tracing::instrument;

type ResolvedModuleKey = (VersionId, ModuleId);

// Bound retained per-module declaration indexes during one linkage validation. Cache misses
// repeat index construction and don't affect linkage-verification results.
const MODULE_INDEX_CACHE_CAPACITY: usize = 1024;

/// Indexed state created for one linkage-validation invocation and discarded before it returns.
///
/// A module ID is resolved through this invocation's original-package-to-version mapping before
/// it is used to retrieve a module or a dependency declaration index.
struct LinkageValidationEnvironment<'a> {
    relocation_map: &'a HashMap<OriginalId, VersionId>,
    resolved_modules: BTreeMap<ResolvedModuleKey, &'a CompiledModule>,
    // Shared dependencies reuse their declaration indexes across callers.
    //
    // `Rc`'s are not _strictly_ necessary, however they are used to ensure that if a single
    // DependencyIndex size exceeds the cache capacity, it is still retained for the duration of
    // the linkage validation. It also makes the code a bit cleaner/simpler.
    module_index_cache: QCache<ResolvedModuleKey, Rc<IndexedModule<'a>>>,
}

impl<'a> LinkageValidationEnvironment<'a> {
    fn new(
        cached_packages: &'a BTreeMap<VersionId, &'a Package>,
        relocation_map: &'a HashMap<OriginalId, VersionId>,
    ) -> Self {
        let mut environment = Self {
            relocation_map,
            resolved_modules: BTreeMap::new(),
            module_index_cache: QCache::new(MODULE_INDEX_CACHE_CAPACITY),
        };
        for (version_id, package) in cached_packages {
            debug_assert!(version_id == &package.version_id);
            environment.extend_with_package(package);
        }
        environment
    }

    /// Adds a package to the environment for cached or publish-inclusive validation.
    fn extend_with_package(&mut self, package: &'a Package) {
        let version_id = self
            .relocation_map
            .get(&package.original_id)
            .copied()
            .unwrap_or(package.version_id);
        for module in package.as_modules() {
            let key = (version_id, module.value.self_id());
            let previous = self.resolved_modules.insert(key.clone(), &module.value);
            if previous.is_some() {
                // TODO(execution-version-cut): Change this to an invariant violation. For now we
                // remove the previous entry to ensure the cache remains consistent.
                debug_assert!(
                    previous.is_none(),
                    "resolved module key should not be overwritten during linkage validation"
                );
                self.module_index_cache.remove(&key);
            }
        }
    }

    fn resolve_module(
        &self,
        module_id: &ModuleId,
    ) -> VMResult<(ResolvedModuleKey, &'a CompiledModule)> {
        let version_id = *self
            .relocation_map
            .get(module_id.address())
            .ok_or_else(|| partial_vm_error!(MISSING_DEPENDENCY).finish(Location::Undefined))?;
        let key = (version_id, module_id.clone());
        let module = self.resolved_modules.get(&key).copied().ok_or_else(|| {
            partial_vm_error!(MISSING_DEPENDENCY).finish(Location::Package(version_id))
        })?;
        Ok((key, module))
    }

    /// Verifies immediate dependency declarations using cached per-module indexes.
    fn verify_module_dependencies(&mut self, module: &CompiledModule) -> VMResult<()> {
        // Resolve every immediate dependency before verification to preserve missing-dependency
        // errors from the linkage environment.
        let resolved_dependencies = module
            .immediate_dependencies()
            .into_iter()
            .map(|module_id| self.resolve_module(&module_id))
            .collect::<VMResult<BTreeMap<_, _>>>()?;
        let dependency_index = DependencyIndex::from_indexed_modules(
            resolved_dependencies.into_iter().map(|(key, dependency)| {
                if let Some(indexed) = self.module_index_cache.get(&key) {
                    Rc::clone(indexed)
                } else {
                    let index = Rc::new(IndexedModule::new(dependency));
                    self.module_index_cache.insert(key, Rc::clone(&index));
                    index
                }
            }),
        );
        dependencies::verify_module_with_dependency_index(module, &dependency_index)?;
        Ok(())
    }
}

/// Verifies that all packages in the provided map have valid linkage and no cyclic dependencies
/// between them.
#[instrument(level = "trace", skip_all)]
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
    tracing::trace!(
        linkage_table = ?relocation_map,
        "verifying linkage and cyclic checks for packages",
    );
    let mut validation_environment =
        LinkageValidationEnvironment::new(cached_packages, &relocation_map);

    for package in cached_packages.values() {
        let package_modules = package.as_modules().into_iter().collect::<Vec<_>>();
        verify_package_valid_linkage(&package_modules, &mut validation_environment)?;
        verify_package_no_cyclic_relationships(&package_modules, cached_packages, &relocation_map)?;
    }

    Ok(())
}

/// Does the same as `verify_linkage_and_cyclic_checks` however it special-cases the package that
/// is being published so that we can verify that the package can be published before adding it to
/// the cache (i.e., that at least in the current linking context the package is valid w.r.t. its
/// dependencies).
#[instrument(level = "trace", skip_all)]
pub(crate) fn verify_linkage_and_cyclic_checks_for_publication(
    package_to_publish: &Package,
    cached_packages: &BTreeMap<VersionId, &Package>,
) -> VMResult<()> {
    tracing::trace!(
        version_id = %package_to_publish.version_id,
        original_id = %package_to_publish.original_id,
        version = %package_to_publish.version,
        "verifying linkage and cyclic checks for package publication",
    );
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

    // Verify the dependencies of the package to publish against the cached-only set without the
    // to-be-published package first.
    let mut validation_environment =
        LinkageValidationEnvironment::new(cached_packages, &relocation_map);
    for package in cached_packages.values() {
        let package_modules = package.as_modules().into_iter().collect::<Vec<_>>();
        verify_package_valid_linkage(&package_modules, &mut validation_environment)?;
        verify_package_no_cyclic_relationships(&package_modules, cached_packages, &relocation_map)?;
    }

    // Extend the validation environment with the package to publish before validating it.
    validation_environment.extend_with_package(package_to_publish);
    let package_modules = package_to_publish
        .as_modules()
        .into_iter()
        .collect::<Vec<_>>();
    verify_package_valid_linkage(&package_modules, &mut validation_environment)?;
    verify_package_no_cyclic_relationships(&package_modules, cached_packages, &relocation_map)?;

    Ok(())
}

/// NB: In all cases it is assume the `package` is in the `relocation_map`. In the case of
/// publication it will simply be a mapping of the package's original package ID to itself (since
/// they are the same for publication).
#[instrument(level = "trace", skip_all, ret)]
fn verify_package_no_cyclic_relationships(
    package: &[&Module],
    cached_packages: &BTreeMap<VersionId, &Package>,
    relocation_map: &HashMap<OriginalId, VersionId>,
) -> VMResult<()> {
    let mut to_visit_modules: BTreeMap<_, _> =
        package.iter().map(|m| (m.value.self_id(), m)).collect();
    let module_map = to_visit_modules.clone();

    // Iteratively visit modules, removing them from the to-visit set as we go. If we encounter a
    // cycle an error is returned.
    while let Some((_, module)) = to_visit_modules.pop_last() {
        let visited = cyclic_dependencies::verify_module(&module.value, |original_module_id| {
            let module = if let Some(bundled) = module_map.get(original_module_id) {
                Some(**bundled)
            } else {
                let version_id = relocation_map
                    .get(original_module_id.address())
                    .ok_or_else(|| partial_vm_error!(MISSING_DEPENDENCY))?;
                cached_packages
                    .get(version_id)
                    .and_then(|p| p.modules.get(&original_module_id.to_owned()))
            };

            module
                .map(|m| m.value.immediate_dependencies())
                .ok_or_else(|| partial_vm_error!(MISSING_DEPENDENCY))
        })?;

        // Remove all visited modules from the to-visit set.
        for k in visited.iter() {
            to_visit_modules.remove(k);
        }
    }

    Ok(())
}

// Given the package and the validation environment this function verifies that
// all modules in the provided package have valid linkage to their dependencies.
#[instrument(level = "trace", skip_all, ret)]
fn verify_package_valid_linkage(
    package: &[&Module],
    validation_environment: &mut LinkageValidationEnvironment<'_>,
) -> VMResult<()> {
    for m in package {
        validation_environment.verify_module_dependencies(&m.value)?;
    }
    Ok(())
}
