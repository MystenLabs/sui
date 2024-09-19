// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This is the "orchestrator" of loading a package.
// The package loader is responsible for the management of packages, package loading and caching,
// and publishing packages to the VM.

use crate::{
    cache::{arena::ArenaPointer, linkage_checker, type_cache::TypeCache},
    jit::{
        self,
        runtime::ast::{Function, Module, Package},
    },
    natives::functions::NativeFunctions,
    on_chain::ast::{DeserializedPackage, PackageStorageId, RuntimePackageId},
    shared::logging::expect_no_verification_errors,
    vm::runtime_vtables::RuntimeVTables,
};
use move_binary_format::{
    errors::{verification_error, Location, PartialVMError, PartialVMResult, VMResult},
    file_format::{StructFieldInformation, TableIndex},
    CompiledModule, IndexKind,
};
use move_core_types::{
    identifier::IdentStr,
    language_storage::{ModuleId, TypeTag},
    vm_status::StatusCode,
};
use move_vm_config::runtime::VMConfig;
use move_vm_types::{data_store::DataStore, loaded_data::runtime_types::Type};
use parking_lot::RwLock;
use petgraph::{algo::toposort, graphmap::DiGraphMap};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};
use tracing::error;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

type PackageCache = HashMap<PackageStorageId, Arc<Package>>;

/// A Loaded Function for driving VM Calls
pub struct LoadedFunction {
    compiled_module: Arc<CompiledModule>,
    loaded_module: Arc<Module>,
    function: ArenaPointer<Function>,
    /// Parameters for the function call
    pub parameters: Vec<Type>,
    /// Function return type
    pub return_: Vec<Type>,
}

/// The loader for the VM. This is the data structure is used to resolve packages and cache them
/// and their types. This is then used to create the VTables for the VM.
#[derive(Debug)]
pub struct VMCache {
    pub(crate) natives: Arc<NativeFunctions>,
    pub(crate) vm_config: Arc<VMConfig>,
    pub(crate) type_cache: Arc<RwLock<TypeCache>>,
    pub(crate) package_cache: Arc<RwLock<PackageCache>>,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl VMCache {
    pub fn new(natives: Arc<NativeFunctions>, vm_config: Arc<VMConfig>) -> Self {
        Self {
            natives,
            vm_config,
            package_cache: Arc::new(RwLock::new(HashMap::new())),
            type_cache: Arc::new(RwLock::new(TypeCache::new())),
        }
    }

    pub fn type_cache(&self) -> &RwLock<TypeCache> {
        &self.type_cache
    }

    pub fn package_cache(&self) -> &RwLock<PackageCache> {
        &self.package_cache
    }

    // -------------------------------------------
    // Main Entry Points
    // -------------------------------------------

    /// Given a root package id, a type cache, and a data store, this function creates a new map of
    /// loaded packages that consist of the root package and all of its dependencies as specified
    /// by the root package. This may perform loading and cachcing as part of vtable creation.
    ///
    /// The resuling map of vtables _must_ be closed under the static dependency graph of the root
    /// package w.r.t, to the current linkage context in `data_store`.
    pub fn generate_runtime_vtables<'cache, D: DataStore>(
        &'cache self,
        data_store: &D,
    ) -> VMResult<RuntimeVTables> {
        let mut loaded_packages = HashMap::new();

        // Make sure the root package and all of its dependencies (under the current linkage
        // context) are loaded.
        let cached_packages = self.resolve_link_context(data_store)?;

        // Verify that the linkage and cyclic checks pass for all packages under the current
        // linkage context.
        linkage_checker::verify_linkage_and_cyclic_checks(&cached_packages)?;
        cached_packages.into_iter().for_each(|(_, p)| {
            loaded_packages.insert(p.runtime_id, p);
        });

        RuntimeVTables::new(loaded_packages, self.type_cache.clone())
    }

    /// Verify a package using the package loader. This will load the package from the data store
    /// and validate the package, including attempting to jit-compile the package and verify
    /// linkage with its dependencies in the provided linkage context. This returns the loaded
    /// package in the case an `init` function or similar will need to run.
    pub fn verify_package_for_publication(
        &self,
        modules: Vec<Vec<u8>>,
        data_store: &impl DataStore,
        runtime_package_id: RuntimePackageId,
    ) -> VMResult<Package> {
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

        // Load the package, but don't insert it into the cache yet.
        // FIXME: This will insert it into the type cache currently, see TODO on type cache.
        self.jit_package(data_store, storage_id, loading_package)
    }

    // -------------------------------------------
    // Lookup Methods
    // -------------------------------------------

    /// Retrieves a single module from the cache. NOTE: this package is _not_ checked for cyclic
    /// dependency verification or linkage, simply retrieved from the cache (or loaded, if
    /// necessary). Also, this may trigger subsequent loads for package dependencies to build up
    /// the type cache.
    pub fn get_module(
        &self,
        data_store: &impl DataStore,
        module_id: &ModuleId,
    ) -> VMResult<(Arc<CompiledModule>, Arc<Module>)> {
        let module_id = data_store.relocate(module_id).map_err(|err| {
            err.with_message("Could not relocate module in data store".to_string())
                .finish(Location::Undefined)
        })?;
        let (package, ident) = module_id.into();
        let packages =
            self.load_and_cache_packages(data_store, BTreeSet::from([package.clone()]))?;
        let Some(package) = packages.get(&package) else {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Package not found in loaded cache".to_string())
                    .finish(Location::Undefined),
            );
        };
        let Some(compiled_module) = package.compiled_modules.get(&ident) else {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Module not found in package".to_string())
                    .finish(Location::Undefined),
            );
        };
        let Some(loaded_module) = package.loaded_modules.get(&ident) else {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Module not found in package".to_string())
                    .finish(Location::Undefined),
            );
        };
        Ok((compiled_module.clone(), loaded_module.clone()))
    }

    /// Retries a function defnitionn from the cache, handing back the contianing module, function,
    /// and paraemter and return type information. NOTE: this package is _not_ checked for cyclic
    /// dependency verification or linkage, simply retrieved from the cache (or loaded, if
    /// necessary). Also, this may trigger subsequent loads for package dependencies to build up
    /// the type cache.
    pub fn get_function(
        &self,
        data_store: &impl DataStore,
        module_id: &ModuleId,
        function_name: &IdentStr,
        ty_args: &[Type],
    ) -> VMResult<LoadedFunction> {
        let (compiled_module, loaded_module) = self.get_module(data_store, module_id)?;
        let module_id = data_store.relocate(module_id).map_err(|err| {
            err.with_message("Could not relocate module in data store".to_string())
                .finish(Location::Undefined)
        })?;
        let Some(function) = loaded_module
            .function_map
            .get(function_name)
            .map(|fun| fun.clone())
        else {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Module not found in package".to_string())
                    .finish(Location::Undefined),
            );
        };

        let fun_ref = function.to_ref();

        let parameters = compiled_module
            .signature_at(fun_ref.parameters)
            .0
            .iter()
            .map(|tok| {
                self.type_cache()
                    .read()
                    .make_type(&compiled_module, tok, data_store)
            })
            .collect::<PartialVMResult<Vec<_>>>()
            .map_err(|err| err.finish(Location::Undefined))?;

        let return_ = compiled_module
            .signature_at(fun_ref.return_)
            .0
            .iter()
            .map(|tok| {
                self.type_cache()
                    .read()
                    .make_type(&compiled_module, tok, data_store)
            })
            .collect::<PartialVMResult<Vec<_>>>()
            .map_err(|err| err.finish(Location::Undefined))?;

        // verify type arguments
        self.type_cache()
            .read()
            .verify_ty_args(fun_ref.type_parameters(), ty_args)
            .map_err(|e| e.finish(Location::Module(module_id.clone())))?;

        let loaded_function = LoadedFunction {
            compiled_module,
            loaded_module,
            function,
            parameters,
            return_,
        };
        Ok(loaded_function)
    }

    /// Load the transitive closure of packages for the current linkage context. NOTE: this does
    /// _not_ perform cyclic dependency verification or linkage checking.
    pub fn resolve_link_context(
        &self,
        data_store: &impl DataStore,
    ) -> VMResult<BTreeMap<PackageStorageId, Arc<Package>>> {
        let root_package = data_store.link_context();
        let mut all_packages = data_store.all_package_dependencies()?;
        all_packages.insert(root_package);
        self.load_and_cache_packages(data_store, all_packages)
    }

    // -------------------------------------------
    // Internal Loading, JIT Compilation, And Caching
    // -------------------------------------------

    // Loads the set of packages into the package cache.
    fn load_and_cache_packages(
        &self,
        data_store: &impl DataStore,
        packages_to_read: BTreeSet<PackageStorageId>,
    ) -> VMResult<BTreeMap<PackageStorageId, Arc<Package>>> {
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
                let package_deps = compute_immediate_package_dependencies(
                    &dep,
                    pkg.modules.values().collect::<Vec<_>>(),
                    data_store,
                )?;
                pkgs_to_cache.insert(dep, (pkg, package_deps));
            };
        }

        let pkgs_in_dependency_order =
            compute_dependency_order(pkgs_to_cache).map_err(|e| e.finish(Location::Undefined))?;

        // Cache each package in reverse dependency order.
        // NB: the packages must be cached in reverse dependency order otherwise types may not be cached
        // correctly.
        for (package_id, deserialized_package) in pkgs_in_dependency_order.into_iter().rev() {
            let pkg = self.fetch_or_jit_package(data_store, package_id, deserialized_package)?;
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

    // Read the package from the data store, deserialize it, and verify it (internally).
    // NB: Does not perform cyclic dependency verification or linkage checking.
    fn read_package_modules_from_store(
        &self,
        package_id: &PackageStorageId,
        data_store: &impl DataStore,
        allow_loading_failure: bool,
    ) -> VMResult<DeserializedPackage> {
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

    // Deserialize and interanlly verify the package.
    // NB: Does not perform cyclic dependency verification or linkage checking.
    fn deserialize_and_verify_package(&self, bytes: Vec<Vec<u8>>) -> VMResult<DeserializedPackage> {
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
            })?;

            // bytecode verifier checks that can be performed with the module itself
            move_bytecode_verifier::verify_module_with_config_unmetered(
                &self.vm_config.verifier,
                &module,
            )?;
            check_natives(&self.natives, &module)?;
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
            .get(0)
            .expect("non-empty package")
            .self_id()
            .address();

        Ok(DeserializedPackage::new(runtime_id, modules))
    }

    fn cached_package_at(&self, package_key: PackageStorageId) -> Option<Arc<Package>> {
        self.package_cache.read().get(&package_key).map(Arc::clone)
    }

    /// Retrieve a JIT-compiled package from the cache, or compile and add it to the cache.
    fn fetch_or_jit_package(
        &self,
        data_store: &impl DataStore,
        package_key: PackageStorageId,
        loading_package: DeserializedPackage,
    ) -> VMResult<Arc<Package>> {
        if let Some(loaded_package) = self.cached_package_at(package_key) {
            return Ok(loaded_package);
        }

        let loaded_package = self.jit_package(data_store, package_key, loading_package)?;

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

    /// Convert the deserialied, on-chain package into a JIT-compiled package.
    /// INVARIANT: If the package is  already in the cache, this will produce an Invariant Violation.
    fn jit_package(
        &self,
        data_store: &impl DataStore,
        package_key: PackageStorageId,
        loading_package: DeserializedPackage,
    ) -> VMResult<Package> {
        if self.cached_package_at(package_key).is_some() {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Package already cached when loading".to_string())
                    .finish(Location::Undefined),
            );
        }
        jit::translate_package(
            &self.natives,
            &self.type_cache,
            data_store,
            package_key,
            loading_package,
        )
        .map_err(|err| err.finish(Location::Undefined))
    }
}

// -------------------------------------------------------------------------------------------------
// Dependency Analysis
// -------------------------------------------------------------------------------------------------

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
        (DeserializedPackage, BTreeSet<PackageStorageId>),
    >,
) -> PartialVMResult<Vec<(PackageStorageId, DeserializedPackage)>> {
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

// All native functions must be known to the loader at load time.
fn check_natives(natives: &NativeFunctions, module: &CompiledModule) -> VMResult<()> {
    fn check_natives_impl(
        natives: &NativeFunctions,
        module: &CompiledModule,
    ) -> PartialVMResult<()> {
        for (idx, native_function) in module
            .function_defs()
            .iter()
            .filter(|fdv| fdv.is_native())
            .enumerate()
        {
            let fh = module.function_handle_at(native_function.function);
            let mh = module.module_handle_at(fh.module);
            natives
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
    check_natives_impl(natives, module).map_err(|e| e.finish(Location::Module(module.self_id())))
}

// -------------------------------------------------------------------------------------------------
// Other Impls
// -------------------------------------------------------------------------------------------------

impl Clone for VMCache {
    /// Makes a shallow copy of the VM Cache by cloning all the internal `Arc`s.
    fn clone(&self) -> Self {
        let VMCache {
            natives,
            vm_config,
            type_cache,
            package_cache,
        } = self;
        Self {
            natives: natives.clone(),
            vm_config: vm_config.clone(),
            type_cache: type_cache.clone(),
            package_cache: package_cache.clone(),
        }
    }
}
