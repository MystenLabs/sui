// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// This module contains the caching logic for packages. It assumes:
// * The package loader has already loaded the package from the data store, and has fully verified the package.
// * All dependencies of the package have been loaded and verified and have already been added to the cache.
// Given this, it will:
// 1. load the types into the type cache,
// 2. create VTables for function in the package; and
// 3. Compile the `CompiledModule` (i.e., file format) bytecode into the internal (runtime/loaded) representation of
//    the bytecode.
//
// A couple things to note about the package cache because of the assumptions above:
//
// 1. The package cache is _not_ responsible for verifying the bytecode. This is done by the
//    package loader.
// 2. The package cache is _not_ responsible for loading the dependencies of the package, or for
//    managing the dependencies of the package. This is done by the package loader.
// 3. The package cache is _not_ responsible for loading the package from the data store. This is
//    done by the package loader.
// 4. The caching of types is "expensive" (currently) because it will acquire a global write lock
//    on the type cache.
//    TODO(tzakian): Optimize this see if this can be done in a more efficient way.

use super::{
    arena::ArenaPointer,
    ast::{Function, LoadedModule},
    package_loader::LoadingPackage,
    translate2,
    type_cache::TypeCache,
    BinaryCache,
};
use crate::{
    loader::{Arena, FieldTypeInfo},
    native_functions::NativeFunctions,
};
use move_binary_format::{
    errors::{Location, PartialVMError, PartialVMResult, VMResult},
    file_format::{EnumDefinitionIndex, StructDefinitionIndex, StructFieldInformation},
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, language_storage::ModuleId,
    vm_status::StatusCode,
};
use move_vm_types::{
    data_store::DataStore,
    loaded_data::runtime_types::{CachedDatatype, Datatype, EnumType, StructType, VariantType},
};
use parking_lot::RwLock;
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

pub type PackageStorageId = AccountAddress;
pub type RuntimePackageId = AccountAddress;

/// A `PackageVTable` is a collection of function pointers indexed by the module and function name
/// within the package.
pub type PackageVTable = BinaryCache<(Identifier, Identifier), ArenaPointer<Function>>;

/// The package cache holds all the packages that have been loaded into the VM. They are stored as
/// refcounted `Arena`s so that they can be shared across threads. For each transaction that uses a
/// package the refcount is bumped, and when the transaction is doen the refcount is decremented.
///
/// A package is safe to remove if the refcount is 0.
pub struct PackageCache {
    /// The backing store for the packages.
    pub package_cache: HashMap<PackageStorageId, Arc<LoadedPackage>>,
}

/// Representation of a loaded package.
pub struct LoadedPackage {
    pub storage_id: PackageStorageId,
    pub runtime_id: RuntimePackageId,

    // NB: this is under the package's context so we don't need to further resolve by
    // address in this table.
    pub loaded_modules: BinaryCache<Identifier, LoadedModule>,

    // NB: this is needed for the bytecode verifier. If we update the bytecode verifier we should
    // be able to remove this.
    pub compiled_modules: BinaryCache<Identifier, CompiledModule>,

    // NB: All things except for types are allocated into this arena.
    pub package_arena: Arena,
    pub vtable: PackageVTable,
}

/// runtime_address::module_name::function_name
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct VTableKey {
    pub package_key: RuntimePackageId,
    pub module_name: Identifier,
    pub function_name: Identifier,
}

impl PackageCache {
    pub fn new() -> Self {
        Self {
            package_cache: HashMap::new(),
        }
    }

    // Retrieve a module by `ModuleId`. The module may have not been loaded yet in which
    // case `None` is returned
    pub(crate) fn loaded_module_at(
        &self,
        link_context: PackageStorageId,
        runtime_id: &ModuleId,
    ) -> Option<Arc<LoadedModule>> {
        self.package_cache.get(&link_context).and_then(|package| {
            package
                .loaded_modules
                .get(&runtime_id.name().to_owned())
                .map(Arc::clone)
        })
    }

    pub(crate) fn loaded_package_at(
        &self,
        package_key: PackageStorageId,
    ) -> Option<Arc<LoadedPackage>> {
        self.package_cache.get(&package_key).map(Arc::clone)
    }

    pub(crate) fn cache_package(
        &mut self,
        package_key: PackageStorageId,
        loading_package: LoadingPackage,
        natives: &NativeFunctions,
        data_store: &impl DataStore,
        type_cache: &RwLock<TypeCache>,
    ) -> VMResult<Arc<LoadedPackage>> {
        if let Some(loaded_package) = self.loaded_package_at(package_key) {
            return Ok(loaded_package);
        }

        let package_runtime_id = loading_package.runtime_id;
        let mut loading_package = loading_package.into_modules();

        let module_ids_in_pkg = loading_package
            .iter()
            .map(|m| m.self_id())
            .collect::<BTreeSet<_>>();

        let mut loaded_package = LoadedPackage::new(package_key, package_runtime_id);

        // Load modules in dependency order within the package. Needed for both static call
        // resolution and type caching.
        while let Some(module) = loading_package.pop() {
            let mut immediate_dependencies = module
                .immediate_dependencies()
                .into_iter()
                .filter(|dep| module_ids_in_pkg.contains(dep) && dep != &module.self_id());

            // If we haven't processed the immediate dependencies yet, push the module back onto
            // the front and process other modules first.
            if !immediate_dependencies.all(|dep| {
                loaded_package
                    .loaded_modules
                    .contains(&dep.name().to_owned())
            }) {
                loading_package.insert(0, module);
                continue;
            }

            loaded_package
                .load_module(module, natives, data_store, type_cache)
                .map_err(|e| e.finish(Location::Undefined))?;
        }

        self.package_cache
            .insert(package_key, Arc::new(loaded_package));

        self.package_cache
            .get(&package_key)
            .cloned()
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Package not found in cache after loading".to_string())
                    .finish(Location::Undefined)
            })
    }
}

impl LoadedPackage {
    pub fn new(package_id: PackageStorageId, runtime_id: RuntimePackageId) -> Self {
        Self {
            storage_id: package_id,
            runtime_id,
            loaded_modules: BinaryCache::new(),
            compiled_modules: BinaryCache::new(),
            vtable: PackageVTable::new(),
            package_arena: Arena::new(),
        }
    }

    pub(crate) fn insert_and_make_module_vtable(
        &mut self,
        module_name: Identifier,
        vtable: impl IntoIterator<Item = (Identifier, ArenaPointer<Function>)>,
    ) -> PartialVMResult<()> {
        for (name, func) in vtable {
            self.vtable.insert((module_name.clone(), name), func)?;
        }
        Ok(())
    }

    pub(crate) fn try_resolve_function(
        &self,
        vtable_entry: &VTableKey,
    ) -> Option<ArenaPointer<Function>> {
        self.vtable
            .get(&(
                vtable_entry.module_name.clone(),
                vtable_entry.function_name.clone(),
            ))
            .map(|f| ArenaPointer::new(f.to_ref()))
    }

    /// Loads the information from the module into the different caches
    fn load_module(
        &mut self,
        module: CompiledModule,
        natives: &NativeFunctions,
        data_store: &impl DataStore,
        type_cache: &RwLock<TypeCache>,
    ) -> PartialVMResult<()> {
        let module_id = module.self_id();

        // Add new structs and collect their field signatures
        let mut field_signatures = vec![];
        for (idx, struct_def) in module.struct_defs().iter().enumerate() {
            let struct_handle = module.datatype_handle_at(struct_def.struct_handle);
            let name = module.identifier_at(struct_handle.name);
            let struct_key = (
                self.storage_id,
                module_id.name().to_owned(),
                name.to_owned(),
            );

            if type_cache.read().cached_types.contains(&struct_key) {
                continue;
            }

            let field_names = match &struct_def.field_information {
                StructFieldInformation::Native => vec![],
                StructFieldInformation::Declared(field_info) => field_info
                    .iter()
                    .map(|f| module.identifier_at(f.name).to_owned())
                    .collect(),
            };

            let defining_id = data_store.defining_module(&module_id, name)?;

            type_cache.write().cache_datatype(
                dbg!(struct_key),
                CachedDatatype {
                    abilities: struct_handle.abilities,
                    type_parameters: struct_handle.type_parameters.clone(),
                    name: name.to_owned(),
                    defining_id,
                    runtime_id: module_id.clone(),
                    depth: None,
                    datatype_info: Datatype::Struct(StructType {
                        fields: vec![],
                        field_names,
                        struct_def: StructDefinitionIndex(idx as u16),
                    }),
                },
            )?;

            let StructFieldInformation::Declared(fields) = &struct_def.field_information else {
                unreachable!("native structs have been removed");
            };

            let signatures: Vec<_> = fields.iter().map(|f| &f.signature.0).collect();
            field_signatures.push(signatures)
        }

        let mut variant_defs = vec![];
        for (idx, enum_def) in module.enum_defs().iter().enumerate() {
            let enum_handle = module.datatype_handle_at(enum_def.enum_handle);
            let name = module.identifier_at(enum_handle.name);
            let enum_key = (
                self.storage_id,
                module_id.name().to_owned(),
                name.to_owned(),
            );

            if type_cache.read().cached_types.contains(&enum_key) {
                continue;
            }

            let variant_info: Vec<_> = enum_def
                .variants
                .iter()
                .enumerate()
                .map(|(variant_tag, variant_def)| {
                    (variant_tag, &variant_def.variant_name, &variant_def.fields)
                })
                .collect();

            variant_defs.push((idx, variant_info));

            let defining_id = data_store.defining_module(&module_id, name)?;
            type_cache.write().cache_datatype(
                enum_key,
                CachedDatatype {
                    abilities: enum_handle.abilities,
                    type_parameters: enum_handle.type_parameters.clone(),
                    name: name.to_owned(),
                    defining_id,
                    runtime_id: module_id.clone(),
                    depth: None,
                    datatype_info: Datatype::Enum(EnumType {
                        variants: vec![],
                        enum_def: EnumDefinitionIndex(idx as u16),
                    }),
                },
            )?;
        }

        // Convert field signatures into types after adding all structs because field types might
        // refer to other datatypes in the same module.
        let mut field_types = vec![];
        for signature in field_signatures {
            let tys: Vec<_> = signature
                .iter()
                .map(|tok| type_cache.read().make_type(&module, tok))
                .collect::<PartialVMResult<_>>()?;
            field_types.push(FieldTypeInfo::Struct(tys));
        }

        for (enum_def_idx, infos) in variant_defs.into_iter() {
            let mut variant_fields = vec![];
            for (tag, name_idx, field_defs) in infos.iter() {
                let mut fields = vec![];
                let mut field_names = vec![];
                for field in field_defs.iter() {
                    fields.push(type_cache.read().make_type(&module, &field.signature.0)?);
                    field_names.push(module.identifier_at(field.name).to_owned());
                }
                variant_fields.push(VariantType {
                    fields,
                    field_names,
                    enum_def: EnumDefinitionIndex(enum_def_idx as u16),
                    variant_tag: *tag as u16,
                    variant_name: module.identifier_at(**name_idx).to_owned(),
                })
            }

            field_types.push(FieldTypeInfo::Enum(variant_fields));
        }

        let field_types_len = field_types.len();

        // Add the field types to the newly added structs and enums, processing them in reverse, to line them
        // up with the structs and enums we added at the end of the global cache.
        for (field_info, cached_type) in field_types
            .into_iter()
            .rev()
            .zip(type_cache.write().cached_types.binaries.iter_mut().rev())
        {
            match Arc::get_mut(cached_type) {
                Some(ref mut x) => match (&mut x.datatype_info, field_info) {
                    (Datatype::Enum(ref mut enum_type), FieldTypeInfo::Enum(field_info)) => {
                        enum_type.variants = field_info;
                    }
                    (Datatype::Struct(ref mut struct_type), FieldTypeInfo::Struct(field_info)) => {
                        struct_type.fields = field_info;
                    }
                    _ => {
                        return Err(
                            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                                .with_message(
                                    "Type mismatch when loading type into module cache -- enum and structs out of synch".to_owned()
                                ),
                        );
                    }
                },
                None => {
                    return Err(
                        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                            .with_message(
                                "Unable to populate cached type in module cache".to_owned(),
                            ),
                    );
                }
            }
        }

        // let mut depth_cache = BTreeMap::new();
        // for datatype in self.datatypes.binaries.iter().rev().take(field_types_len) {
        //     self.calculate_depth_of_datatype(datatype, &mut depth_cache)?;
        // }

        // debug_assert!(depth_cache.len() == field_types_len);
        // for (cache_idx, depth) in depth_cache {
        //     match Arc::get_mut(self.datatypes.binaries.get_mut(cache_idx.0).unwrap()) {
        //         Some(datatype) => datatype.depth = Some(depth),
        //         None => {
        //             // we have pending references to the `Arc` which is impossible,
        //             // given the code that adds the `Arc` is above and no reference to
        //             // it should exist.
        //             // So in the spirit of not crashing we log the issue and move on leaving the
        //             // datatypes depth as `None` -- if we try to access it later we will treat this
        //             // as too deep.
        //             error!("Arc<Datatype> cannot have any live reference while publishing");
        //         }
        //     }
        // }

        // Load the module, build its vtable (allocating functions into the package's arena) and
        // then insert the module into the cache along with its vtable.
        let loaded_module =
            translate2::module(natives, self.storage_id, &module, self, type_cache)?;
        self.loaded_modules
            .insert(module_id.name().to_owned(), loaded_module)?;

        self.compiled_modules
            .insert(module_id.name().to_owned(), module)?;
        Ok(())
    }
}
