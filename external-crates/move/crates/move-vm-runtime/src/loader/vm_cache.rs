// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This is the "orchestrator" of loading a package.
// The package loader is responsible for the management of packages, package loading and caching,
// and publishing packages to the VM.

use super::{
    ast::{LoadedPackage, PackageStorageId, RuntimePackageId},
    chain_ast::BinaryFormatPackage,
    linkage_checker,
    runtime_vtable::RuntimeVTables,
    translate2,
    type_cache::TypeCache,
};
use crate::{logging::expect_no_verification_errors, native_functions::NativeFunctions};
use move_binary_format::{
    errors::{verification_error, Location, PartialVMError, PartialVMResult, VMResult},
    file_format::{StructFieldInformation, TableIndex},
    CompiledModule, IndexKind,
};
use move_core_types::vm_status::StatusCode;
use move_vm_config::runtime::VMConfig;
use move_vm_types::data_store::DataStore;
use parking_lot::RwLock;
use petgraph::{algo::toposort, graphmap::DiGraphMap};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};
use tracing::error;

/// The loader for the VM. This is the data structure is used to resolve packages and cache them
/// and their types. This is then used to create the VTables for the VM.
pub(crate) struct VMCache {
    pub(crate) natives: Arc<NativeFunctions>,
    pub(crate) vm_config: VMConfig,
    pub(crate) type_cache: RwLock<TypeCache>,
    pub(crate) package_cache: RwLock<HashMap<PackageStorageId, Arc<LoadedPackage>>>,
}

impl VMCache {
    pub fn new(natives: NativeFunctions, vm_config: VMConfig) -> Self {
        Self {
            natives: Arc::new(natives),
            vm_config,
            package_cache: RwLock::new(HashMap::new()),
            type_cache: RwLock::new(TypeCache::new()),
        }
    }

    /// Load the transitive closure of packages for the current linkage context. NOTE: this does
    /// _not_ perform cyclic dependency verification or linkage checking.
    pub fn load_and_cache_link_context(
        &self,
        data_store: &impl DataStore,
    ) -> VMResult<BTreeMap<PackageStorageId, Arc<LoadedPackage>>> {
        let root_package = data_store.link_context();
        let mut all_packages = data_store.all_package_dependencies()?;
        all_packages.insert(root_package);
        self.load_and_cache_packages(data_store, all_packages)
    }

    /// Publish a package to the package loader. This will cache the package and verify the package
    /// under the current linkage context.
    pub fn publish_package(
        &self,
        modules: Vec<Vec<u8>>,
        data_store: &impl DataStore,
        runtime_package_id: RuntimePackageId,
    ) -> VMResult<()> {
        let loading_package = self.deserialize_and_verify_package(modules)?;

        // Make sure all modules' self addresses match the `runtime_package_id`.
        for module in loading_package.as_modules().into_iter() {
            if module.address() != &runtime_package_id {
                return Err(verification_error(
                    StatusCode::MISMATCHED_MODULE_IDS_IN_PACKAGE,
                    IndexKind::AddressIdentifier,
                    module.self_handle_idx().0,
                )
                .finish(Location::Undefined));
            }
        }

        let storage_id = {
            let module_id = loading_package
                .as_modules()
                .into_iter()
                .next()
                .expect("non-empty package")
                .self_id();
            *data_store
                .relocate(&module_id)
                .map_err(|e| e.finish(Location::Undefined))?
                .address()
        };

        // Cache the package's dependencies without the package.
        let cached_packages =
            self.load_and_cache_packages(data_store, data_store.all_package_dependencies()?)?;

        // Now verify linking on-the-spot to make sure that the current package links correctly in
        // the supplied linking context.
        linkage_checker::verify_linkage_and_cyclic_checks_for_publication(
            &loading_package,
            &cached_packages,
        )?;

        // Cache the package and its types.
        self.cache_package(
            storage_id,
            loading_package,
            &self.natives,
            data_store,
            &self.type_cache,
        )?;

        Ok(())
    }

    /// Given a data store this function creates a new map of loaded packages that consist of the
    /// root package and all of its dependencies as specified by the root package based on the link
    /// context within the data store.
    ///
    /// The resuling map of vtables _must_ be closed under the static dependency graph of the root
    /// package w.r.t, to the current linkage context in `data_store`.
    pub fn generate_vtables<'a>(
        &'a self,
        data_store: &impl DataStore,
    ) -> VMResult<RuntimeVTables<'a>> {
        let mut loaded_packages = HashMap::new();

        // Make sure the root package and all of its dependencies (under the current linkage
        // context) are loaded.
        let cached_packages = self.load_and_cache_link_context(data_store)?;

        // Verify that the linkage and cyclic checks pass for all packages under the current
        // linkage context.
        linkage_checker::verify_linkage_and_cyclic_checks(&cached_packages)?;
        cached_packages.into_iter().for_each(|(_, p)| {
            loaded_packages.insert(p.runtime_id, p);
        });

        Ok(RuntimeVTables {
            loaded_packages,
            cached_types: &self.type_cache,
        })
    }

    // Loads the set of packages into the package cache.
    fn load_and_cache_packages(
        &self,
        data_store: &impl DataStore,
        packages_to_read: BTreeSet<PackageStorageId>,
    ) -> VMResult<BTreeMap<PackageStorageId, Arc<LoadedPackage>>> {
        let allow_loading_failure = true;

        let mut seen_packages = BTreeSet::new();

        let mut cached_packages = BTreeMap::new();
        let mut pkgs_to_cache = BTreeMap::new();
        let mut work_queue: Vec<_> = packages_to_read.clone().into_iter().collect();

        // Load all packages, compute dependency order (excluding already cached
        // packages). NB: packages can be loaded out of order here (e.g., in parallel) if so
        // desired.
        while let Some(dep) = work_queue.pop() {
            if seen_packages.contains(&dep) {
                continue;
            }

            seen_packages.insert(dep);

            // Check if package is already cached. If so add it to the cached packages.
            // Note that this package will not contribute to the dependency order of packages to
            // loade since it and its types are already cached.
            if let Some(pkg) = self.cached_package_at(dep) {
                cached_packages.insert(dep, pkg);
            } else {
                let pkg =
                    self.read_package_modules_from_store(&dep, data_store, allow_loading_failure)?;
                let package_deps = Self::compute_immediate_package_dependencies(
                    &dep,
                    pkg.modules.values().collect::<Vec<_>>(),
                    data_store,
                )?;
                pkgs_to_cache.insert(dep, (pkg, package_deps));
            };
        }

        let pkgs_in_dependency_order = Self::compute_dependency_order(pkgs_to_cache)
            .map_err(|e| e.finish(Location::Undefined))?;

        // Cache each package in reverse dependency order.
        // NB: the packages must be cached in reverse dependency order otherwise types may not be cached
        // correctly.
        for (package_id, deserialized_package) in pkgs_in_dependency_order.into_iter().rev() {
            let pkg = self.cache_package(
                package_id,
                deserialized_package,
                &self.natives,
                data_store,
                &self.type_cache,
            )?;
            cached_packages.insert(package_id, pkg);
        }

        // The number of cached packages should be the same as the number of packages provided to
        // us by the linkage context.
        debug_assert!(
            cached_packages.len() == packages_to_read.len(),
            "Mismatch in number of packages in linkage table and cached packages"
        );
        Ok(cached_packages)
    }

    fn cache_package(
        &self,
        package_key: PackageStorageId,
        loading_package: BinaryFormatPackage,
        natives: &NativeFunctions,
        data_store: &impl DataStore,
        type_cache: &RwLock<TypeCache>,
    ) -> VMResult<Arc<LoadedPackage>> {
        if let Some(loaded_package) = self.cached_package_at(package_key) {
            return Ok(loaded_package);
        }

        let loaded_package = translate2::package(
            package_key,
            loading_package.runtime_id,
            loading_package.into_modules(),
            natives,
            type_cache,
            data_store,
        )
        .map_err(|e| e.finish(Location::Undefined))?;

        self.package_cache
            .write()
            .insert(package_key, Arc::new(loaded_package));

        self.package_cache
            .read()
            .get(&package_key)
            .cloned()
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Package not found in cache after loading".to_string())
                    .finish(Location::Undefined)
            })
    }

    // All native functions must be known to the loader at load time.
    fn check_natives(&self, module: &CompiledModule) -> VMResult<()> {
        fn check_natives_impl(loader: &VMCache, module: &CompiledModule) -> PartialVMResult<()> {
            for (idx, native_function) in module
                .function_defs()
                .iter()
                .filter(|fdv| fdv.is_native())
                .enumerate()
            {
                let fh = module.function_handle_at(native_function.function);
                let mh = module.module_handle_at(fh.module);
                loader
                    .natives
                    .resolve(
                        module.address_identifier_at(mh.address),
                        module.identifier_at(mh.name).as_str(),
                        module.identifier_at(fh.name).as_str(),
                    )
                    .ok_or_else(|| {
                        verification_error(
                            StatusCode::MISSING_DEPENDENCY,
                            IndexKind::FunctionHandle,
                            idx as TableIndex,
                        )
                    })?;
            }

            // TODO: fix check and error code if we leave something around for native structs.
            // For now this generates the only error test cases care about...
            for (idx, struct_def) in module.struct_defs().iter().enumerate() {
                if struct_def.field_information == StructFieldInformation::Native {
                    return Err(verification_error(
                        StatusCode::MISSING_DEPENDENCY,
                        IndexKind::FunctionHandle,
                        idx as TableIndex,
                    ));
                }
            }
            Ok(())
        }
        check_natives_impl(self, module).map_err(|e| e.finish(Location::Module(module.self_id())))
    }

    // Read the package from the data store, deserialize it, and verify it (internally).
    // NB: Does not perform cyclic dependency verification or linkage checking.
    fn read_package_modules_from_store(
        &self,
        package_id: &PackageStorageId,
        data_store: &impl DataStore,
        allow_loading_failure: bool,
    ) -> VMResult<BinaryFormatPackage> {
        // Load the package bytes
        let bytes = match data_store.load_package(package_id) {
            Ok(bytes) => bytes,
            Err(err) if allow_loading_failure => return Err(err),
            Err(err) => {
                error!("[VM] Error fetching package {package_id:?}");
                return Err(expect_no_verification_errors(err));
            }
        };
        self.deserialize_and_verify_package(bytes)
    }

    // Deserialize and verify the package.
    // NB: Does not perform cyclic dependency verification or linkage checking.
    fn deserialize_and_verify_package(&self, bytes: Vec<Vec<u8>>) -> VMResult<BinaryFormatPackage> {
        // Deserialize each module in the package
        let mut modules = vec![];
        for module_bytes in bytes.iter() {
            let module = CompiledModule::deserialize_with_config(
                module_bytes,
                &self.vm_config.binary_config,
            )
            .map_err(|err| {
                let msg = format!("Deserialization error: {:?}", err);
                PartialVMError::new(StatusCode::CODE_DESERIALIZATION_ERROR)
                    .with_message(msg)
                    .finish(Location::Undefined) // TODO(tzakian): add Location::Package
            })
            .map_err(expect_no_verification_errors)?;

            // bytecode verifier checks that can be performed with the module itself
            move_bytecode_verifier::verify_module_with_config_unmetered(
                &self.vm_config.verifier,
                &module,
            )
            .map_err(expect_no_verification_errors)?;
            self.check_natives(&module)
                .map_err(expect_no_verification_errors)?;
            modules.push(module)
        }

        // Packages must be non-empty
        if modules.is_empty() {
            return Err(PartialVMError::new(StatusCode::EMPTY_PACKAGE)
                .with_message("Empty packages are not allowed.".to_string())
                .finish(Location::Undefined));
        }

        // NB: We don't check for cycles inside of the package just yet since we may need to load
        // further packages.

        let runtime_id = *modules
            .first()
            .expect("non-empty package")
            .self_id()
            .address();

        Ok(BinaryFormatPackage::new(runtime_id, modules))
    }

    // Compute the immediate dependencies of a package in terms of their storage IDs.
    fn compute_immediate_package_dependencies<'a>(
        package_id: &PackageStorageId,
        modules: impl IntoIterator<Item = &'a CompiledModule>,
        data_store: &impl DataStore,
    ) -> VMResult<BTreeSet<PackageStorageId>> {
        modules
            .into_iter()
            .flat_map(|m| m.immediate_dependencies())
            .map(|m| Ok(*data_store.relocate(&m)?.address()))
            .filter(|m| m.as_ref().is_ok_and(|m| m != package_id))
            .collect::<PartialVMResult<BTreeSet<_>>>()
            .map_err(|e| {
                PartialVMError::new(StatusCode::MISSING_DEPENDENCY)
                    .with_message(format!(
                        "Failed to locate immediate dependencies of package {}: {}",
                        package_id, e
                    ))
                    .finish(Location::Undefined)
            })
    }

    fn compute_dependency_order(
        mut pkgs_to_cache: BTreeMap<
            PackageStorageId,
            (BinaryFormatPackage, BTreeSet<PackageStorageId>),
        >,
    ) -> PartialVMResult<Vec<(PackageStorageId, BinaryFormatPackage)>> {
        // Compute edges for the dependency graph
        let package_edges = pkgs_to_cache.iter().flat_map(|(package_id, (_, deps))| {
            deps.iter()
                .filter(|pkg| pkgs_to_cache.contains_key(pkg))
                .map(|dep_pkg| (*package_id, *dep_pkg))
        });

        let mut digraph = DiGraphMap::<PackageStorageId, ()>::from_edges(package_edges);

        // Make sure every package is in the graph (even if it has no dependencies)
        for pkg in pkgs_to_cache.keys() {
            digraph.add_node(*pkg);
        }

        Ok(toposort(&digraph, None)
            .map_err(|_| {
                PartialVMError::new(StatusCode::CYCLIC_PACKAGE_DEPENDENCY)
                    .with_message("Cyclic package dependency detected".to_string())
            })?
            .into_iter()
            .map(|pkg| {
                (
                    pkg,
                    pkgs_to_cache
                        .remove(&pkg)
                        .expect("dependency order computation error")
                        .0,
                )
            })
            .collect())
    }

    fn cached_package_at(&self, package_key: PackageStorageId) -> Option<Arc<LoadedPackage>> {
        self.package_cache.read().get(&package_key).map(Arc::clone)
    }
}
