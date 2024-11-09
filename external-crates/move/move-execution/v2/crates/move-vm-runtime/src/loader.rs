// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    logging::expect_no_verification_errors,
    native_functions::{NativeFunction, NativeFunctions, UnboxedNativeFunction},
    session::LoadedFunctionInstantiation,
};
use move_binary_format::{
    errors::{verification_error, Location, PartialVMError, PartialVMResult, VMResult},
    file_format::{
        AbilitySet, Bytecode, CompiledModule, Constant, ConstantPoolIndex, FieldHandleIndex,
        FieldInstantiationIndex, FunctionDefinition, FunctionDefinitionIndex, FunctionHandleIndex,
        FunctionInstantiationIndex, SignatureIndex, SignatureToken, StructDefInstantiationIndex,
        StructDefinitionIndex, StructFieldInformation, TableIndex, TypeParameterIndex,
    },
    IndexKind,
};
use move_bytecode_verifier::{self, cyclic_dependencies, dependencies};
use move_core_types::{
    account_address::AccountAddress,
    annotated_value as A,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag, TypeTag},
    metadata::Metadata,
    runtime_value as R,
    vm_status::StatusCode,
};
use move_vm_config::runtime::VMConfig;
use move_vm_types::{
    data_store::DataStore,
    loaded_data::runtime_types::{
        CachedDatatype, CachedTypeIndex, Datatype, DepthFormula, StructType, Type,
    },
};
use parking_lot::RwLock;
use std::{
    collections::{btree_map::Entry, BTreeMap, BTreeSet, HashMap},
    fmt::Debug,
    hash::Hash,
    sync::Arc,
};
use tracing::error;

// A simple cache that offers both a HashMap and a Vector lookup.
// Values are forced into a `Arc` so they can be used from multiple thread.
// Access to this cache is always under a `RwLock`.
struct BinaryCache<K, V> {
    id_map: HashMap<K, usize>,
    binaries: Vec<Arc<V>>,
}

impl<K, V> BinaryCache<K, V>
where
    K: Eq + Hash,
{
    fn new() -> Self {
        Self {
            id_map: HashMap::new(),
            binaries: vec![],
        }
    }

    fn insert(&mut self, key: K, binary: V) -> PartialVMResult<&Arc<V>> {
        let idx = self.binaries.len();
        // Last write wins in the binary cache -- it's up to the callee to not make conflicting
        // writes.
        self.id_map.insert(key, idx);
        self.binaries.push(Arc::new(binary));
        Ok(&self.binaries[idx])
    }

    fn get_with_idx(&self, key: &K) -> Option<(usize, &Arc<V>)> {
        let idx = self.id_map.get(key)?;
        Some((*idx, self.binaries.get(*idx)?))
    }

    fn get(&self, key: &K) -> Option<&Arc<V>> {
        Some(self.get_with_idx(key)?.1)
    }

    fn contains(&self, key: &K) -> bool {
        self.id_map.contains_key(key)
    }

    fn len(&self) -> usize {
        self.binaries.len()
    }
}

/// The ModuleCache holds all verified modules as well as loaded modules, structs, and functions.
/// Structs and functions are pushed into a global structure, handles in compiled modules are
/// replaced with indices into these global structures in loaded modules.  All access to the
/// ModuleCache via the Loader is under an RWLock.
pub struct ModuleCache {
    /// Compiled modules go in this cache once they have been individually verified.
    compiled_modules: BinaryCache<ModuleId, CompiledModule>,
    /// Modules whose dependencies have been verified already (during publishing or loading).
    verified_dependencies: BTreeSet<(AccountAddress, ModuleId)>,
    /// Loaded modules go in this cache once their compiled modules have been link-checked, and
    /// structs and functions have populated `structs` and `functions` below.
    ///
    /// The `AccountAddress` in the key is the "link context", and the `ModuleId` is the ID of the
    /// module whose load was requested. A mapping `(ctx, id) => module` means that when `id` was
    /// requested in context `ctx`, `module` was loaded.
    loaded_modules: BinaryCache<(AccountAddress, ModuleId), LoadedModule>,

    /// Global cache of loaded structs, shared among all modules.
    structs: BinaryCache<(ModuleId, Identifier), CachedDatatype>,
    /// Global list of loaded functions, shared among all modules.
    functions: Vec<Arc<Function>>,
}

/// Tracks the current end point of the `ModuleCache`'s `struct`s and `function`s, so that we can
/// roll-back to that point in case of error.
struct CacheCursor {
    last_struct: usize,
    last_function: usize,
}

impl ModuleCache {
    fn new() -> Self {
        Self {
            compiled_modules: BinaryCache::new(),
            verified_dependencies: BTreeSet::new(),
            loaded_modules: BinaryCache::new(),
            structs: BinaryCache::new(),
            functions: vec![],
        }
    }

    //
    // Common "get" operations
    //

    // Retrieve a module by `ModuleId`. The module may have not been loaded yet in which
    // case `None` is returned
    fn compiled_module_at(&self, id: &ModuleId) -> Option<Arc<CompiledModule>> {
        self.compiled_modules.get(id).map(Arc::clone)
    }

    // Retrieve a module by `ModuleId`. The module may have not been loaded yet in which
    // case `None` is returned
    fn loaded_module_at(
        &self,
        link_context: AccountAddress,
        runtime_id: &ModuleId,
    ) -> Option<Arc<LoadedModule>> {
        self.loaded_modules
            .get(&(link_context, runtime_id.clone()))
            .map(Arc::clone)
    }

    // Retrieve a function by index
    fn function_at(&self, idx: usize) -> Arc<Function> {
        Arc::clone(&self.functions[idx])
    }

    // Retrieve a struct by index
    fn struct_at(&self, idx: CachedTypeIndex) -> Arc<CachedDatatype> {
        Arc::clone(&self.structs.binaries[idx.0])
    }

    //
    // Insertion is under lock and it's a pretty heavy operation.
    // The VM is pretty much stopped waiting for this to finish
    //

    fn insert(
        &mut self,
        natives: &NativeFunctions,
        data_store: &impl DataStore,
        storage_id: ModuleId,
        module: &CompiledModule,
    ) -> VMResult<Arc<LoadedModule>> {
        let runtime_id = module.self_id();
        if let Some(cached) = self.loaded_module_at(data_store.link_context(), &runtime_id) {
            return Ok(cached);
        }

        // Make sure the modules of dependencies are in the cache.
        for runtime_dep in module.immediate_dependencies() {
            let storage_dep = data_store
                .relocate(&runtime_dep)
                .map_err(|e| e.finish(Location::Undefined))?;
            let compiled_dep = self.compiled_module_at(&storage_dep).ok_or_else(|| {
                PartialVMError::new(StatusCode::MISSING_DEPENDENCY)
                    .with_message(format!(
                        "Cannot load dependency {runtime_dep} of {storage_id} from {storage_dep}"
                    ))
                    .finish(Location::Undefined)
            })?;

            self.insert(natives, data_store, storage_dep, compiled_dep.as_ref())?;
        }

        let cursor = self.cursor();
        match self.add_module(&cursor, natives, data_store, storage_id, module) {
            Ok(module) => Ok(Arc::clone(module)),
            Err(err) => {
                // we need this operation to be transactional, if an error occurs we must
                // leave a clean state
                self.reset(cursor);
                Err(err.finish(Location::Undefined))
            }
        }
    }

    fn add_module(
        &mut self,
        cursor: &CacheCursor,
        natives: &NativeFunctions,
        data_store: &impl DataStore,
        storage_id: ModuleId,
        module: &CompiledModule,
    ) -> PartialVMResult<&Arc<LoadedModule>> {
        let link_context = data_store.link_context();
        let runtime_id = module.self_id();

        // Add new structs and collect their field signatures
        let mut field_signatures = vec![];
        for (idx, struct_def) in module.struct_defs().iter().enumerate() {
            let struct_handle = module.datatype_handle_at(struct_def.struct_handle);
            let name = module.identifier_at(struct_handle.name);
            let struct_key = (runtime_id.clone(), name.to_owned());

            if self.structs.contains(&struct_key) {
                continue;
            }

            let field_names = match &struct_def.field_information {
                StructFieldInformation::Native => vec![],
                StructFieldInformation::Declared(field_info) => field_info
                    .iter()
                    .map(|f| module.identifier_at(f.name).to_owned())
                    .collect(),
            };

            let defining_id = data_store.defining_module(&runtime_id, name)?;

            self.structs.insert(
                struct_key,
                CachedDatatype {
                    abilities: struct_handle.abilities,
                    type_parameters: struct_handle.type_parameters.clone(),
                    name: name.to_owned(),
                    defining_id,
                    runtime_id: runtime_id.clone(),
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

        // Convert field signatures into types after adding all structs because field types might
        // refer to structs in the same module.
        let mut field_types = vec![];
        for signature in field_signatures {
            let tys: Vec<_> = signature
                .iter()
                .map(|tok| self.make_type(module, tok))
                .collect::<PartialVMResult<_>>()?;
            field_types.push(tys);
        }

        let field_types_len = field_types.len();

        // Add the field types to the newly added structs, processing them in reverse, to line them
        // up with the structs we added at the end of the global cache.
        for (fields, struct_type) in field_types
            .into_iter()
            .rev()
            .zip(self.structs.binaries.iter_mut().rev())
        {
            match Arc::get_mut(struct_type) {
                Some(ref mut x) => match &mut x.datatype_info {
                    Datatype::Enum(_) => {
                        unreachable!("enum types cannot be loaded into the cache in v2")
                    }
                    Datatype::Struct(ref mut struct_type) => {
                        struct_type.fields = fields;
                    }
                },
                None => {
                    // we have pending references to the `Arc` which is impossible,
                    // given the code that adds the `Arc` is above and no reference to
                    // it should exist.
                    // So in the spirit of not crashing we just rewrite the entire `Arc`
                    // over and log the issue.
                    error!("Arc<StructType> cannot have any live reference while publishing");
                    let mut struct_copy = (**struct_type).clone();
                    match struct_copy.datatype_info {
                        Datatype::Enum(_) => {
                            unreachable!("enum types cannot be loaded into the cache in v2")
                        }
                        Datatype::Struct(ref mut s_info) => {
                            s_info.fields = fields;
                            struct_copy.datatype_info = Datatype::Struct(s_info.clone());
                            *struct_type = Arc::new(struct_copy);
                        }
                    }
                }
            }
        }

        let mut depth_cache = BTreeMap::new();
        for struct_type in self.structs.binaries.iter().rev().take(field_types_len) {
            self.calculate_depth_of_struct(struct_type, &mut depth_cache)?;
        }

        debug_assert!(depth_cache.len() == field_types_len);
        for (cache_idx, depth) in depth_cache {
            match Arc::get_mut(self.structs.binaries.get_mut(cache_idx.0).unwrap()) {
                Some(struct_type) => struct_type.depth = Some(depth),
                None => {
                    // we have pending references to the `Arc` which is impossible,
                    // given the code that adds the `Arc` is above and no reference to
                    // it should exist.
                    // So in the spirit of not crashing we log the issue and move on leaving the
                    // structs depth as `None` -- if we try to access it later we will treat this
                    // as too deep.
                    error!("Arc<StructType> cannot have any live reference while publishing");
                }
            }
        }

        for (idx, func) in module.function_defs().iter().enumerate() {
            let findex = FunctionDefinitionIndex(idx as TableIndex);
            let function = Function::new(natives, findex, func, module);
            self.functions.push(Arc::new(function));
        }

        let loaded_module = LoadedModule::new(cursor, link_context, storage_id, module, self)?;
        self.loaded_modules
            .insert((link_context, runtime_id), loaded_module)
    }

    // `make_type` is the entry point to "translate" a `SignatureToken` to a `Type`
    fn make_type(&self, module: &CompiledModule, tok: &SignatureToken) -> PartialVMResult<Type> {
        let res = match tok {
            SignatureToken::Bool => Type::Bool,
            SignatureToken::U8 => Type::U8,
            SignatureToken::U16 => Type::U16,
            SignatureToken::U32 => Type::U32,
            SignatureToken::U64 => Type::U64,
            SignatureToken::U128 => Type::U128,
            SignatureToken::U256 => Type::U256,
            SignatureToken::Address => Type::Address,
            SignatureToken::Signer => Type::Signer,
            SignatureToken::TypeParameter(idx) => Type::TyParam(*idx),
            SignatureToken::Vector(inner_tok) => {
                Type::Vector(Box::new(self.make_type(module, inner_tok)?))
            }
            SignatureToken::Reference(inner_tok) => {
                Type::Reference(Box::new(self.make_type(module, inner_tok)?))
            }
            SignatureToken::MutableReference(inner_tok) => {
                Type::MutableReference(Box::new(self.make_type(module, inner_tok)?))
            }
            SignatureToken::Datatype(sh_idx) => {
                let struct_handle = module.datatype_handle_at(*sh_idx);
                let struct_name = module.identifier_at(struct_handle.name);
                let module_handle = module.module_handle_at(struct_handle.module);
                let runtime_id = ModuleId::new(
                    *module.address_identifier_at(module_handle.address),
                    module.identifier_at(module_handle.name).to_owned(),
                );
                let def_idx = self.resolve_struct_by_name(struct_name, &runtime_id)?.0;
                Type::Datatype(def_idx)
            }
            SignatureToken::DatatypeInstantiation(inst) => {
                let (sh_idx, tys) = &**inst;
                let type_parameters: Vec<_> = tys
                    .iter()
                    .map(|tok| self.make_type(module, tok))
                    .collect::<PartialVMResult<_>>()?;
                let struct_handle = module.datatype_handle_at(*sh_idx);
                let struct_name = module.identifier_at(struct_handle.name);
                let module_handle = module.module_handle_at(struct_handle.module);
                let runtime_id = ModuleId::new(
                    *module.address_identifier_at(module_handle.address),
                    module.identifier_at(module_handle.name).to_owned(),
                );
                let def_idx = self.resolve_struct_by_name(struct_name, &runtime_id)?.0;
                Type::DatatypeInstantiation(Box::new((def_idx, type_parameters)))
            }
        };
        Ok(res)
    }

    fn calculate_depth_of_struct(
        &self,
        struct_type: &CachedDatatype,
        depth_cache: &mut BTreeMap<CachedTypeIndex, DepthFormula>,
    ) -> PartialVMResult<DepthFormula> {
        let def_idx = self
            .resolve_struct_by_name(&struct_type.name, &struct_type.runtime_id)?
            .0;

        // If we've already computed this structs depth, no more work remains to be done.
        if let Some(form) = &struct_type.depth {
            return Ok(form.clone());
        }
        if let Some(form) = depth_cache.get(&def_idx) {
            return Ok(form.clone());
        }

        let formulas = match &struct_type.datatype_info {
            Datatype::Enum(_) => unreachable!("enum types cannot be loaded into the cache in v2"),
            Datatype::Struct(s_info) => s_info
                .fields
                .iter()
                .map(|field_type| self.calculate_depth_of_type(field_type, depth_cache))
                .collect::<PartialVMResult<Vec<_>>>()?,
        };
        let mut formula = DepthFormula::normalize(formulas);
        // add 1 for the struct itself
        formula.add(1);
        let prev = depth_cache.insert(def_idx, formula.clone());
        if prev.is_some() {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Recursive type?".to_owned()),
            );
        }
        Ok(formula)
    }

    fn calculate_depth_of_type(
        &self,
        ty: &Type,
        depth_cache: &mut BTreeMap<CachedTypeIndex, DepthFormula>,
    ) -> PartialVMResult<DepthFormula> {
        Ok(match ty {
            Type::Bool
            | Type::U8
            | Type::U64
            | Type::U128
            | Type::Address
            | Type::Signer
            | Type::U16
            | Type::U32
            | Type::U256 => DepthFormula::constant(1),
            // we should not see the reference here, we could instead give an invariant violation
            Type::Vector(ty) | Type::Reference(ty) | Type::MutableReference(ty) => {
                let mut inner = self.calculate_depth_of_type(ty, depth_cache)?;
                // add 1 for the vector itself
                inner.add(1);
                inner
            }
            Type::TyParam(ty_idx) => DepthFormula::type_parameter(*ty_idx),
            Type::Datatype(cache_idx) => {
                let struct_type = self.struct_at(*cache_idx);
                let struct_formula = self.calculate_depth_of_struct(&struct_type, depth_cache)?;
                debug_assert!(struct_formula.terms.is_empty());
                struct_formula
            }
            Type::DatatypeInstantiation(inst) => {
                let (cache_idx, ty_args) = &**inst;
                let struct_type = self.struct_at(*cache_idx);
                let ty_arg_map = ty_args
                    .iter()
                    .enumerate()
                    .map(|(idx, ty)| {
                        let var = idx as TypeParameterIndex;
                        Ok((var, self.calculate_depth_of_type(ty, depth_cache)?))
                    })
                    .collect::<PartialVMResult<BTreeMap<_, _>>>()?;
                let struct_formula = self.calculate_depth_of_struct(&struct_type, depth_cache)?;

                struct_formula.subst(ty_arg_map)?
            }
        })
    }

    // Given a ModuleId::struct_name, retrieve the `StructType` and the index associated.
    // Return and error if the type has not been loaded
    fn resolve_struct_by_name(
        &self,
        struct_name: &IdentStr,
        runtime_id: &ModuleId,
    ) -> PartialVMResult<(CachedTypeIndex, Arc<CachedDatatype>)> {
        match self
            .structs
            .get_with_idx(&(runtime_id.clone(), struct_name.to_owned()))
        {
            Some((idx, struct_)) => Ok((CachedTypeIndex(idx), Arc::clone(struct_))),
            None => Err(PartialVMError::new(StatusCode::TYPE_RESOLUTION_FAILURE)
                .with_message(format!("Cannot find {runtime_id}::{struct_name} in cache",))),
        }
    }

    // Given a ModuleId::func_name, retrieve the `Function` and the index associated.
    // Return and error if the function has not been loaded
    fn resolve_function_by_name(
        &self,
        func_name: &IdentStr,
        runtime_id: &ModuleId,
        link_context: AccountAddress,
    ) -> PartialVMResult<usize> {
        match self
            .loaded_modules
            .get(&(link_context, runtime_id.clone()))
            .and_then(|module| module.function_map.get(func_name))
        {
            Some(func_idx) => Ok(*func_idx),
            None => Err(
                PartialVMError::new(StatusCode::FUNCTION_RESOLUTION_FAILURE).with_message(format!(
                    "Cannot find {:?}::{:?} in cache for context {:?}",
                    runtime_id, func_name, link_context,
                )),
            ),
        }
    }

    /// Return the current high watermark of structs and functions in the cache.
    fn cursor(&self) -> CacheCursor {
        CacheCursor {
            last_struct: self.structs.len(),
            last_function: self.functions.len(),
        }
    }

    /// Rollback the cache's structs and functions to the point at which the cache cursor was
    /// created.
    fn reset(
        &mut self,
        CacheCursor {
            last_struct,
            last_function,
        }: CacheCursor,
    ) {
        // Remove entries from `structs.id_map` corresponding to the newly added structs.
        for (idx, struct_) in self.structs.binaries.iter().enumerate().rev() {
            if idx < last_struct {
                break;
            }

            let key = (struct_.runtime_id.clone(), struct_.name.clone());
            match self.structs.id_map.remove(&key) {
                Some(jdx) if jdx == idx => {
                    continue;
                }
                Some(jdx) => unreachable!(
                    "Expected to find {}::{} at index {idx} but found at {jdx}.",
                    struct_.defining_id, struct_.name,
                ),
                None => unreachable!(
                    "Expected to find {}::{} at index {idx} but not found.",
                    struct_.defining_id, struct_.name,
                ),
            }
        }

        self.structs.binaries.truncate(last_struct);
        self.functions.truncate(last_function);
    }
}

//
// Loader
//

// A Loader is responsible to load modules and holds the cache of all loaded
// entities. Each cache is protected by a `RwLock`. Operation in the Loader must be thread safe
// (operating on values on the stack) and when cache needs updating the mutex must be taken.
// The `pub(crate)` API is what a Loader offers to the runtime.
pub(crate) struct Loader {
    module_cache: RwLock<ModuleCache>,
    type_cache: RwLock<TypeCache>,
    natives: NativeFunctions,
    vm_config: VMConfig,
}

impl Loader {
    pub(crate) fn new(natives: NativeFunctions, vm_config: VMConfig) -> Self {
        Self {
            module_cache: RwLock::new(ModuleCache::new()),
            type_cache: RwLock::new(TypeCache::new()),
            natives,
            vm_config,
        }
    }

    pub(crate) fn vm_config(&self) -> &VMConfig {
        &self.vm_config
    }

    /// Copies metadata out of a modules bytecode if available.
    pub(crate) fn get_metadata(&self, module: ModuleId, key: &[u8]) -> Option<Metadata> {
        let cache = self.module_cache.read();
        cache
            .compiled_modules
            .get(&module)
            .and_then(|module| module.metadata.iter().find(|md| md.key == key))
            .cloned()
    }

    //
    // Module verification and loading
    //

    // Entry point for function execution (`MoveVM::execute_function`).
    // Loading verifies the module if it was never loaded.
    // Type parameters are checked as well after every type is loaded.
    pub(crate) fn load_function(
        &self,
        runtime_id: &ModuleId,
        function_name: &IdentStr,
        ty_args: &[Type],
        data_store: &impl DataStore,
    ) -> VMResult<(
        Arc<CompiledModule>,
        Arc<LoadedModule>,
        Arc<Function>,
        LoadedFunctionInstantiation,
    )> {
        let link_context = data_store.link_context();
        let (compiled, loaded) = self.load_module(runtime_id, data_store)?;
        let idx = self
            .module_cache
            .read()
            .resolve_function_by_name(function_name, runtime_id, link_context)
            .map_err(|err| err.finish(Location::Undefined))?;
        let func = self.module_cache.read().function_at(idx);

        let parameters = compiled
            .signature_at(func.parameters)
            .0
            .iter()
            .map(|tok| self.module_cache.read().make_type(&compiled, tok))
            .collect::<PartialVMResult<Vec<_>>>()
            .map_err(|err| err.finish(Location::Undefined))?;

        let return_ = compiled
            .signature_at(func.return_)
            .0
            .iter()
            .map(|tok| self.module_cache.read().make_type(&compiled, tok))
            .collect::<PartialVMResult<Vec<_>>>()
            .map_err(|err| err.finish(Location::Undefined))?;

        // verify type arguments
        self.verify_ty_args(func.type_parameters(), ty_args)
            .map_err(|e| e.finish(Location::Module(runtime_id.clone())))?;

        let inst = LoadedFunctionInstantiation {
            parameters,
            return_,
        };
        Ok((compiled, loaded, func, inst))
    }

    // Entry point for module publishing (`MoveVM::publish_module_bundle`).
    //
    // All modules in the bundle to be published must be loadable. This function performs all
    // verification steps to load these modules without actually loading them into the code cache.
    pub(crate) fn verify_module_bundle_for_publication(
        &self,
        modules: &[CompiledModule],
        data_store: &mut impl DataStore,
    ) -> VMResult<()> {
        fail::fail_point!("verifier-failpoint-1", |_| { Ok(()) });

        let mut bundle_verified = BTreeMap::new();
        for module in modules {
            let module_id = module.self_id();

            self.verify_module_for_publication(module, &bundle_verified, data_store)?;
            bundle_verified.insert(module_id.clone(), module.clone());
        }
        Ok(())
    }

    // A module to be published must be loadable.
    //
    // This step performs all verification steps to load the module without loading it.
    // The module is not added to the code cache. It is simply published to the data cache.
    //
    // If a module `M` is published together with a bundle of modules (i.e., a vector of modules),
    // the `bundle_verified` argument tracks the modules that have already been verified in the
    // bundle. Basically, this represents the modules appears before `M` in the bundle vector.
    fn verify_module_for_publication(
        &self,
        module: &CompiledModule,
        bundle_verified: &BTreeMap<ModuleId, CompiledModule>,
        data_store: &impl DataStore,
    ) -> VMResult<()> {
        // Performs all verification steps to load the module without loading it, i.e., the new
        // module will NOT show up in `module_cache`.
        move_bytecode_verifier::verify_module_with_config_unmetered(
            &self.vm_config.verifier,
            module,
        )?;
        self.check_natives(module)?;

        let mut visiting = BTreeSet::new();
        visiting.insert(module.self_id());

        // downward exploration of the module's dependency graph. Since we know nothing about this
        // target module, we don't know what the module may specify as its dependencies and hence,
        // we allow the loading of dependencies and the subsequent linking to fail.
        self.verify_dependencies(
            module,
            bundle_verified,
            data_store,
            &mut visiting,
            /* allow_dependency_loading_failure */ true,
            /* dependencies_depth */ 0,
        )?;

        // make sure there is no cyclic dependency
        self.verify_module_cyclic_relations(module, bundle_verified, data_store)
    }

    fn verify_module_cyclic_relations(
        &self,
        module: &CompiledModule,
        bundle_verified: &BTreeMap<ModuleId, CompiledModule>,
        data_store: &impl DataStore,
    ) -> VMResult<()> {
        let module_cache = self.module_cache.read();
        cyclic_dependencies::verify_module(module, |runtime_id| {
            let module = if let Some(bundled) = bundle_verified.get(runtime_id) {
                Some(bundled)
            } else {
                let storage_id = data_store.relocate(runtime_id)?;
                module_cache
                    .compiled_modules
                    .get(&storage_id)
                    .map(Arc::as_ref)
            };

            module
                .map(|m| m.immediate_dependencies())
                .ok_or_else(|| PartialVMError::new(StatusCode::MISSING_DEPENDENCY))
        })
    }

    // All native functions must be known to the loader, unless we are compiling with feature
    // `lazy_natives`.
    fn check_natives(&self, module: &CompiledModule) -> VMResult<()> {
        fn check_natives_impl(loader: &Loader, module: &CompiledModule) -> PartialVMResult<()> {
            if !cfg!(feature = "lazy_natives") {
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

    //
    // Helpers for loading and verification
    //

    pub(crate) fn load_struct_by_name(
        &self,
        name: &IdentStr,
        runtime_id: &ModuleId,
        data_store: &impl DataStore,
    ) -> VMResult<(CachedTypeIndex, Arc<CachedDatatype>)> {
        self.load_module(runtime_id, data_store)?;
        self.module_cache
            .read()
            // Should work if the type exists, because module was loaded above.
            .resolve_struct_by_name(name, runtime_id)
            .map_err(|e| e.finish(Location::Undefined))
    }

    pub(crate) fn load_type(
        &self,
        type_tag: &TypeTag,
        data_store: &impl DataStore,
    ) -> VMResult<Type> {
        Ok(match type_tag {
            TypeTag::Bool => Type::Bool,
            TypeTag::U8 => Type::U8,
            TypeTag::U16 => Type::U16,
            TypeTag::U32 => Type::U32,
            TypeTag::U64 => Type::U64,
            TypeTag::U128 => Type::U128,
            TypeTag::U256 => Type::U256,
            TypeTag::Address => Type::Address,
            TypeTag::Signer => Type::Signer,
            TypeTag::Vector(tt) => Type::Vector(Box::new(self.load_type(tt, data_store)?)),
            TypeTag::Struct(struct_tag) => {
                let runtime_id = ModuleId::new(struct_tag.address, struct_tag.module.clone());
                let (idx, struct_type) =
                    self.load_struct_by_name(&struct_tag.name, &runtime_id, data_store)?;
                if struct_type.type_parameters.is_empty() && struct_tag.type_params.is_empty() {
                    Type::Datatype(idx)
                } else {
                    let mut type_params = vec![];
                    for ty_param in &struct_tag.type_params {
                        type_params.push(self.load_type(ty_param, data_store)?);
                    }
                    self.verify_ty_args(struct_type.type_param_constraints(), &type_params)
                        .map_err(|e| e.finish(Location::Undefined))?;
                    Type::DatatypeInstantiation(Box::new((idx, type_params)))
                }
            }
        })
    }

    // The interface for module loading. Aligned with `load_type` and `load_function`, this function
    // verifies that the module is OK instead of expect it.
    pub(crate) fn load_module(
        &self,
        runtime_id: &ModuleId,
        data_store: &impl DataStore,
    ) -> VMResult<(Arc<CompiledModule>, Arc<LoadedModule>)> {
        self.load_module_internal(runtime_id, &BTreeMap::new(), data_store)
    }

    // Load the transitive closure of the target module first, and then verify that the modules in
    // the closure do not have cyclic dependencies.
    fn load_module_internal(
        &self,
        runtime_id: &ModuleId,
        bundle_verified: &BTreeMap<ModuleId, CompiledModule>,
        data_store: &impl DataStore,
    ) -> VMResult<(Arc<CompiledModule>, Arc<LoadedModule>)> {
        let link_context = data_store.link_context();

        {
            let locked_cache = self.module_cache.read();
            if let Some(loaded) = locked_cache.loaded_module_at(link_context, runtime_id) {
                let Some(compiled) = locked_cache.compiled_module_at(&loaded.id) else {
                    unreachable!(
                        "Loaded module without verified compiled module.\n\
                         Context:    {link_context}\n\
                         Runtime ID: {runtime_id}\n\
                         Loaded module: {loaded:#?}"
                    );
                };

                return Ok((compiled, loaded));
            }
        }

        // otherwise, load the transitive closure of the target module
        let mut visiting = BTreeSet::new();
        let allow_module_loading_failure = true;
        let dependencies_depth = 0;
        let (storage_id, compiled) = self.verify_module_and_dependencies(
            runtime_id,
            bundle_verified,
            data_store,
            &mut visiting,
            allow_module_loading_failure,
            dependencies_depth,
        )?;

        // verify that the transitive closure does not have cycles
        self.verify_module_cyclic_relations(compiled.as_ref(), bundle_verified, data_store)
            .map_err(expect_no_verification_errors)?;

        // load the compiled module
        let loaded = self.module_cache.write().insert(
            &self.natives,
            data_store,
            storage_id,
            compiled.as_ref(),
        )?;

        Ok((compiled, loaded))
    }

    /// Read the module that will be referred to at runtime as `runtime_id`, but is found in the
    /// store at `storage_id`.  Verify it without linking or interacting with caches, and return
    /// the deserialized module on success.
    fn read_module_from_store(
        &self,
        runtime_id: &ModuleId,
        storage_id: &ModuleId,
        data_store: &impl DataStore,
        allow_loading_failure: bool,
    ) -> VMResult<CompiledModule> {
        // bytes fetching, allow loading to fail if the flag is set
        let bytes = match data_store.load_module(storage_id) {
            Ok(bytes) => bytes,
            Err(err) if allow_loading_failure => return Err(err),
            Err(err) => {
                error!(
                    "[VM] Error fetching module with id {runtime_id:?} from storage at \
                     {storage_id:?}",
                );
                return Err(expect_no_verification_errors(err));
            }
        };

        // for bytes obtained from the data store, they should always deserialize and verify.
        // It is an invariant violation if they don't.
        let module = CompiledModule::deserialize_with_config(&bytes, &self.vm_config.binary_config)
            .map_err(|err| {
                let msg = format!("Deserialization error: {:?}", err);
                PartialVMError::new(StatusCode::CODE_DESERIALIZATION_ERROR)
                    .with_message(msg)
                    .finish(Location::Module(storage_id.clone()))
            })
            .map_err(expect_no_verification_errors)?;

        fail::fail_point!("verifier-failpoint-2", |_| { Ok(module.clone()) });

        // bytecode verifier checks that can be performed with the module itself
        move_bytecode_verifier::verify_module_with_config_unmetered(
            &self.vm_config.verifier,
            &module,
        )
        .map_err(expect_no_verification_errors)?;
        self.check_natives(&module)
            .map_err(expect_no_verification_errors)?;

        Ok(module)
    }

    /// Deserialize and check the module with the bytecode verifier, without linking.  Cache the
    /// `CompiledModule` on success, and return a reference to it.
    fn verify_module(
        &self,
        runtime_id: &ModuleId,
        data_store: &impl DataStore,
        allow_loading_failure: bool,
    ) -> VMResult<(ModuleId, Arc<CompiledModule>)> {
        let storage_id = data_store
            .relocate(runtime_id)
            .map_err(|e| e.finish(Location::Undefined))?;
        if let Some(cached) = self.module_cache.read().compiled_module_at(&storage_id) {
            return Ok((storage_id, cached));
        }

        let module = self.read_module_from_store(
            runtime_id,
            &storage_id,
            data_store,
            allow_loading_failure,
        )?;

        let cached = self
            .module_cache
            .write()
            .compiled_modules
            .insert(storage_id.clone(), module)
            .map_err(|e| e.finish(Location::Module(storage_id.clone())))?
            .clone();

        Ok((storage_id, cached))
    }

    /// Recursively read the module at ID and its transitive dependencies, verify them individually
    /// and verify that they link together.  Returns the `CompiledModule` for `runtime_id`, written
    /// to the module cache, on success, as well as the `ModuleId` it was read from, in storage.
    fn verify_module_and_dependencies(
        &self,
        runtime_id: &ModuleId,
        bundle_verified: &BTreeMap<ModuleId, CompiledModule>,
        data_store: &impl DataStore,
        visiting: &mut BTreeSet<ModuleId>,
        allow_module_loading_failure: bool,
        dependencies_depth: usize,
    ) -> VMResult<(ModuleId, Arc<CompiledModule>)> {
        // dependency loading does not permit cycles
        if !visiting.insert(runtime_id.clone()) {
            return Err(PartialVMError::new(StatusCode::CYCLIC_MODULE_DEPENDENCY)
                .finish(Location::Undefined));
        }

        // module self-check
        let (storage_id, module) =
            self.verify_module(runtime_id, data_store, allow_module_loading_failure)?;

        // If this module is already in the "verified dependencies" cache, then no need to check it
        // again -- it has already been verified against its dependencies in this link context.
        let cache_key = (data_store.link_context(), runtime_id.clone());
        if !self
            .module_cache
            .read()
            .verified_dependencies
            .contains(&cache_key)
        {
            // downward exploration of the module's dependency graph. For a module that is loaded from
            // the data_store, we should never allow its dependencies to fail to load.
            let allow_dependency_loading_failure = false;
            self.verify_dependencies(
                module.as_ref(),
                bundle_verified,
                data_store,
                visiting,
                allow_dependency_loading_failure,
                dependencies_depth,
            )?;

            self.module_cache
                .write()
                .verified_dependencies
                .insert(cache_key);
        }

        visiting.remove(runtime_id);
        Ok((storage_id, module))
    }

    // downward exploration of the module's dependency graph
    fn verify_dependencies(
        &self,
        module: &CompiledModule,
        bundle_verified: &BTreeMap<ModuleId, CompiledModule>,
        data_store: &impl DataStore,
        visiting: &mut BTreeSet<ModuleId>,
        allow_dependency_loading_failure: bool,
        dependencies_depth: usize,
    ) -> VMResult<()> {
        if let Some(max_dependency_depth) = self.vm_config.verifier.max_dependency_depth {
            if dependencies_depth > max_dependency_depth {
                return Err(
                    PartialVMError::new(StatusCode::MAX_DEPENDENCY_DEPTH_REACHED)
                        .finish(Location::Undefined),
                );
            }
        }

        // all immediate dependencies of the module being verified should be in one of the locations
        // - the verified portion of the bundle (e.g., verified before this module)
        // - the compiled module cache (i.e., module has been self-checked but not link checked)
        // - the data store (i.e., not self-checked yet)
        let mut bundle_deps = vec![];
        let mut cached_deps = vec![];
        for runtime_dep in module.immediate_dependencies() {
            if let Some(cached) = bundle_verified.get(&runtime_dep) {
                bundle_deps.push(cached);
                continue;
            }

            let (_, loaded) = self.verify_module_and_dependencies(
                &runtime_dep,
                bundle_verified,
                data_store,
                visiting,
                allow_dependency_loading_failure,
                dependencies_depth + 1,
            )?;

            cached_deps.push(loaded);
        }

        fail::fail_point!("verifier-failpoint-4", |_| { Ok(()) });

        // once all dependencies are loaded, do the linking check
        let deps = bundle_deps
            .into_iter()
            .chain(cached_deps.iter().map(Arc::as_ref));
        let result = dependencies::verify_module(module, deps);

        // if dependencies loading is not allowed to fail, the linking should not fail as well
        if allow_dependency_loading_failure {
            result
        } else {
            result.map_err(expect_no_verification_errors)
        }?;

        Ok(())
    }

    // Return an instantiated type given a generic and an instantiation.
    // Stopgap to avoid a recursion that is either taking too long or using too
    // much memory
    fn subst(&self, ty: &Type, ty_args: &[Type]) -> PartialVMResult<Type> {
        // Before instantiating the type, count the # of nodes of all type arguments plus
        // existing type instantiation.
        // If that number is larger than MAX_TYPE_INSTANTIATION_NODES, refuse to construct this type.
        // This prevents constructing larger and lager types via struct instantiation.
        if let Type::DatatypeInstantiation(box_struct_inst) = ty {
            let (_, struct_inst) = &**box_struct_inst;
            let mut sum_nodes = 1u64;
            for ty in ty_args.iter().chain(struct_inst.iter()) {
                sum_nodes = sum_nodes.saturating_add(self.count_type_nodes(ty));
                if sum_nodes > MAX_TYPE_INSTANTIATION_NODES {
                    return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
                }
            }
        }
        ty.subst(ty_args)
    }

    // Verify the kind (constraints) of an instantiation.
    // Function invocations call this function to verify correctness of type arguments provided
    fn verify_ty_args<'a, I>(&self, constraints: I, ty_args: &[Type]) -> PartialVMResult<()>
    where
        I: IntoIterator<Item = &'a AbilitySet>,
        I::IntoIter: ExactSizeIterator,
    {
        let constraints = constraints.into_iter();
        if constraints.len() != ty_args.len() {
            return Err(PartialVMError::new(
                StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH,
            ));
        }
        for (ty, expected_k) in ty_args.iter().zip(constraints) {
            if !expected_k.is_subset(self.abilities(ty)?) {
                return Err(PartialVMError::new(StatusCode::CONSTRAINT_NOT_SATISFIED));
            }
        }
        Ok(())
    }

    //
    // Internal helpers
    //

    fn function_at(&self, idx: usize) -> Arc<Function> {
        self.module_cache.read().function_at(idx)
    }

    fn get_module(
        &self,
        link_context: AccountAddress,
        runtime_id: &ModuleId,
    ) -> (Arc<CompiledModule>, Arc<LoadedModule>) {
        let locked_cache = self.module_cache.read();
        let loaded = locked_cache
            .loaded_module_at(link_context, runtime_id)
            .expect("ModuleId on Function must exist");
        let compiled = locked_cache
            .compiled_module_at(&loaded.id)
            .expect("ModuleId on Function must exist");
        (compiled, loaded)
    }

    pub(crate) fn get_struct_type(&self, idx: CachedTypeIndex) -> Option<Arc<CachedDatatype>> {
        self.module_cache
            .read()
            .structs
            .binaries
            .get(idx.0)
            .map(Arc::clone)
    }

    pub(crate) fn abilities(&self, ty: &Type) -> PartialVMResult<AbilitySet> {
        match ty {
            Type::Bool
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::U128
            | Type::U256
            | Type::Address => Ok(AbilitySet::PRIMITIVES),

            // Technically unreachable but, no point in erroring if we don't have to
            Type::Reference(_) | Type::MutableReference(_) => Ok(AbilitySet::REFERENCES),
            Type::Signer => Ok(AbilitySet::SIGNER),

            Type::TyParam(_) => Err(PartialVMError::new(StatusCode::UNREACHABLE).with_message(
                "Unexpected TyParam type after translating from TypeTag to Type".to_string(),
            )),

            Type::Vector(ty) => AbilitySet::polymorphic_abilities(
                AbilitySet::VECTOR,
                vec![false],
                vec![self.abilities(ty)?],
            ),
            Type::Datatype(idx) => Ok(self.module_cache.read().struct_at(*idx).abilities),
            Type::DatatypeInstantiation(inst) => {
                let (idx, type_args) = &**inst;
                let struct_type = self.module_cache.read().struct_at(*idx);
                let declared_phantom_parameters = struct_type
                    .type_parameters
                    .iter()
                    .map(|param| param.is_phantom);
                let type_argument_abilities = type_args
                    .iter()
                    .map(|arg| self.abilities(arg))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                AbilitySet::polymorphic_abilities(
                    struct_type.abilities,
                    declared_phantom_parameters,
                    type_argument_abilities,
                )
            }
        }
    }
}

//
// Resolver
//

// A simple wrapper for a `Module` in the `Resolver`
struct BinaryType {
    compiled: Arc<CompiledModule>,
    loaded: Arc<LoadedModule>,
}

// A Resolver is a simple and small structure allocated on the stack and used by the
// interpreter. It's the only API known to the interpreter and it's tailored to the interpreter
// needs.
pub(crate) struct Resolver<'a> {
    loader: &'a Loader,
    binary: BinaryType,
}

impl<'a> Resolver<'a> {
    fn for_module(
        loader: &'a Loader,
        compiled: Arc<CompiledModule>,
        loaded: Arc<LoadedModule>,
    ) -> Self {
        let binary = BinaryType { compiled, loaded };
        Self { loader, binary }
    }

    //
    // Constant resolution
    //

    pub(crate) fn constant_at(&self, idx: ConstantPoolIndex) -> &Constant {
        self.binary.compiled.constant_at(idx)
    }

    //
    // Function resolution
    //

    pub(crate) fn function_from_handle(&self, idx: FunctionHandleIndex) -> Arc<Function> {
        let idx = self.binary.loaded.function_at(idx.0);
        self.loader.function_at(idx)
    }

    pub(crate) fn function_from_instantiation(
        &self,
        idx: FunctionInstantiationIndex,
    ) -> Arc<Function> {
        let func_inst = self.binary.loaded.function_instantiation_at(idx.0);
        self.loader.function_at(func_inst.handle)
    }

    pub(crate) fn instantiate_generic_function(
        &self,
        idx: FunctionInstantiationIndex,
        type_params: &[Type],
    ) -> PartialVMResult<Vec<Type>> {
        let loaded_module = &*self.binary.loaded;
        let func_inst = loaded_module.function_instantiation_at(idx.0);
        let instantiation: Vec<_> = loaded_module
            .instantiation_signature_at(func_inst.instantiation_idx)?
            .iter()
            .map(|ty| self.subst(ty, type_params))
            .collect::<PartialVMResult<_>>()?;

        // Check if the function instantiation over all generics is larger
        // than MAX_TYPE_INSTANTIATION_NODES.
        let mut sum_nodes = 1u64;
        for ty in type_params.iter().chain(instantiation.iter()) {
            sum_nodes = sum_nodes.saturating_add(self.loader.count_type_nodes(ty));
            if sum_nodes > MAX_TYPE_INSTANTIATION_NODES {
                return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
            }
        }
        Ok(instantiation)
    }

    //
    // Type resolution
    //

    pub(crate) fn get_struct_type(&self, idx: StructDefinitionIndex) -> Type {
        let struct_def = self.binary.loaded.struct_at(idx);
        Type::Datatype(struct_def)
    }

    pub(crate) fn instantiate_generic_type(
        &self,
        idx: StructDefInstantiationIndex,
        ty_args: &[Type],
    ) -> PartialVMResult<Type> {
        let loaded_module = &*self.binary.loaded;
        let struct_inst = loaded_module.struct_instantiation_at(idx.0);
        let instantiation =
            loaded_module.instantiation_signature_at(struct_inst.instantiation_idx)?;

        // Before instantiating the type, count the # of nodes of all type arguments plus
        // existing type instantiation.
        // If that number is larger than MAX_TYPE_INSTANTIATION_NODES, refuse to construct this type.
        // This prevents constructing larger and larger types via struct instantiation.
        let mut sum_nodes = 1u64;
        for ty in ty_args.iter().chain(instantiation.iter()) {
            sum_nodes = sum_nodes.saturating_add(self.loader.count_type_nodes(ty));
            if sum_nodes > MAX_TYPE_INSTANTIATION_NODES {
                return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
            }
        }

        Ok(Type::DatatypeInstantiation(Box::new((
            struct_inst.def,
            instantiation
                .iter()
                .map(|ty| self.subst(ty, ty_args))
                .collect::<PartialVMResult<_>>()?,
        ))))
    }

    fn single_type_at(&self, idx: SignatureIndex) -> &Type {
        self.binary.loaded.single_type_at(idx)
    }

    pub(crate) fn instantiate_single_type(
        &self,
        idx: SignatureIndex,
        ty_args: &[Type],
    ) -> PartialVMResult<Type> {
        let ty = self.single_type_at(idx);
        if !ty_args.is_empty() {
            self.subst(ty, ty_args)
        } else {
            Ok(ty.clone())
        }
    }

    pub(crate) fn subst(&self, ty: &Type, ty_args: &[Type]) -> PartialVMResult<Type> {
        self.loader.subst(ty, ty_args)
    }

    //
    // Fields resolution
    //

    pub(crate) fn field_offset(&self, idx: FieldHandleIndex) -> usize {
        self.binary.loaded.field_offset(idx)
    }

    pub(crate) fn field_instantiation_offset(&self, idx: FieldInstantiationIndex) -> usize {
        self.binary.loaded.field_instantiation_offset(idx)
    }

    pub(crate) fn field_count(&self, idx: StructDefinitionIndex) -> u16 {
        self.binary.loaded.field_count(idx.0)
    }

    pub(crate) fn field_instantiation_count(&self, idx: StructDefInstantiationIndex) -> u16 {
        self.binary.loaded.field_instantiation_count(idx.0)
    }

    pub(crate) fn type_to_type_layout(&self, ty: &Type) -> PartialVMResult<R::MoveTypeLayout> {
        self.loader.type_to_type_layout(ty)
    }

    pub(crate) fn type_to_fully_annotated_layout(
        &self,
        ty: &Type,
    ) -> PartialVMResult<A::MoveTypeLayout> {
        self.loader.type_to_fully_annotated_layout(ty)
    }

    // get the loader
    pub(crate) fn loader(&self) -> &Loader {
        self.loader
    }
}

// A LoadedModule is very similar to a CompiledModule but data is "transformed" to a representation
// more appropriate to execution.
// When code executes indexes in instructions are resolved against those runtime structure
// so that any data needed for execution is immediately available
#[derive(Debug)]
pub(crate) struct LoadedModule {
    #[allow(dead_code)]
    id: ModuleId,

    //
    // types as indexes into the Loader type list
    //

    // struct references carry the index into the global vector of types.
    // That is effectively an indirection over the ref table:
    // the instruction carries an index into this table which contains the index into the
    // glabal table of types. No instantiation of generic types is saved into the global table.
    #[allow(dead_code)]
    struct_refs: Vec<CachedTypeIndex>,
    structs: Vec<StructDef>,
    // materialized instantiations, whether partial or not
    struct_instantiations: Vec<StructInstantiation>,

    // functions as indexes into the Loader function list
    // That is effectively an indirection over the ref table:
    // the instruction carries an index into this table which contains the index into the
    // glabal table of functions. No instantiation of generic functions is saved into
    // the global table.
    function_refs: Vec<usize>,
    // materialized instantiations, whether partial or not
    function_instantiations: Vec<FunctionInstantiation>,

    // fields as a pair of index, first to the type, second to the field position in that type
    field_handles: Vec<FieldHandle>,
    // materialized instantiations, whether partial or not
    field_instantiations: Vec<FieldInstantiation>,

    // function name to index into the Loader function list.
    // This allows a direct access from function name to `Function`
    function_map: HashMap<Identifier, usize>,

    // a map of single-token signature indices to type.
    // Single-token signatures are usually indexed by the `SignatureIndex` in bytecode. For example,
    // `VecMutBorrow(SignatureIndex)`, the `SignatureIndex` maps to a single `SignatureToken`, and
    // hence, a single type.
    single_signature_token_map: BTreeMap<SignatureIndex, Type>,

    // a map from signatures in instantiations to the `Vec<Type>` that reperesent it.
    instantiation_signatures: BTreeMap<SignatureIndex, Vec<Type>>,
}

impl LoadedModule {
    fn new(
        cursor: &CacheCursor,
        link_context: AccountAddress,
        storage_id: ModuleId,
        module: &CompiledModule,
        cache: &ModuleCache,
    ) -> Result<Self, PartialVMError> {
        let self_id = module.self_id();

        let mut instantiation_signatures: BTreeMap<SignatureIndex, Vec<Type>> = BTreeMap::new();
        // helper to build the sparse signature vector
        fn cache_signatures(
            instantiation_signatures: &mut BTreeMap<SignatureIndex, Vec<Type>>,
            module: &CompiledModule,
            instantiation_idx: SignatureIndex,
            cache: &ModuleCache,
        ) -> Result<(), PartialVMError> {
            if let Entry::Vacant(e) = instantiation_signatures.entry(instantiation_idx) {
                let instantiation = module
                    .signature_at(instantiation_idx)
                    .0
                    .iter()
                    .map(|ty| cache.make_type(module, ty))
                    .collect::<Result<Vec<_>, _>>()?;
                e.insert(instantiation);
            }
            Ok(())
        }

        let mut struct_refs = vec![];
        let mut structs = vec![];
        let mut struct_instantiations = vec![];
        let mut function_refs = vec![];
        let mut function_instantiations = vec![];
        let mut field_handles = vec![];
        let mut field_instantiations: Vec<FieldInstantiation> = vec![];
        let mut function_map = HashMap::new();
        let mut single_signature_token_map = BTreeMap::new();

        for struct_handle in module.datatype_handles() {
            let struct_name = module.identifier_at(struct_handle.name);
            let module_handle = module.module_handle_at(struct_handle.module);
            let runtime_id = module.module_id_for_handle(module_handle);
            struct_refs.push(cache.resolve_struct_by_name(struct_name, &runtime_id)?.0);
        }

        for struct_def in module.struct_defs() {
            let idx = struct_refs[struct_def.struct_handle.0 as usize];
            let field_count = cache.structs.binaries[idx.0]
                .get_struct()
                .unwrap()
                .fields
                .len() as u16;
            structs.push(StructDef { field_count, idx });
        }

        for struct_inst in module.struct_instantiations() {
            let def = struct_inst.def.0 as usize;
            let struct_def = &structs[def];
            let field_count = struct_def.field_count;

            let instantiation_idx = struct_inst.type_parameters;
            cache_signatures(
                &mut instantiation_signatures,
                module,
                instantiation_idx,
                cache,
            )?;
            struct_instantiations.push(StructInstantiation {
                field_count,
                def: struct_def.idx,
                instantiation_idx,
            });
        }

        for func_handle in module.function_handles() {
            let func_name = module.identifier_at(func_handle.name);
            let module_handle = module.module_handle_at(func_handle.module);
            let runtime_id = module.module_id_for_handle(module_handle);
            if runtime_id == self_id {
                // module has not been published yet, loop through the functions
                for (idx, function) in cache.functions.iter().enumerate().rev() {
                    if idx < cursor.last_function {
                        return Err(PartialVMError::new(StatusCode::FUNCTION_RESOLUTION_FAILURE)
                            .with_message(format!(
                                "Cannot find {:?}::{:?} in publishing module",
                                runtime_id, func_name
                            )));
                    }
                    if function.name.as_ident_str() == func_name {
                        function_refs.push(idx);
                        break;
                    }
                }
            } else {
                function_refs.push(cache.resolve_function_by_name(
                    func_name,
                    &runtime_id,
                    link_context,
                )?);
            }
        }

        for func_def in module.function_defs() {
            let idx = function_refs[func_def.function.0 as usize];
            let name = module.identifier_at(module.function_handle_at(func_def.function).name);
            function_map.insert(name.to_owned(), idx);

            if let Some(code_unit) = &func_def.code {
                for bc in &code_unit.code {
                    match bc {
                        Bytecode::VecPack(si, _)
                        | Bytecode::VecLen(si)
                        | Bytecode::VecImmBorrow(si)
                        | Bytecode::VecMutBorrow(si)
                        | Bytecode::VecPushBack(si)
                        | Bytecode::VecPopBack(si)
                        | Bytecode::VecUnpack(si, _)
                        | Bytecode::VecSwap(si) => {
                            if !single_signature_token_map.contains_key(si) {
                                let ty = match module.signature_at(*si).0.first() {
                                    None => {
                                        return Err(PartialVMError::new(
                                            StatusCode::VERIFIER_INVARIANT_VIOLATION,
                                        )
                                        .with_message(
                                            "the type argument for vector-related bytecode \
                                                expects one and only one signature token"
                                                .to_owned(),
                                        ));
                                    }
                                    Some(sig_token) => sig_token,
                                };
                                single_signature_token_map
                                    .insert(*si, cache.make_type(module, ty)?);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        for func_inst in module.function_instantiations() {
            let handle = function_refs[func_inst.handle.0 as usize];

            let instantiation_idx = func_inst.type_parameters;
            cache_signatures(
                &mut instantiation_signatures,
                module,
                instantiation_idx,
                cache,
            )?;
            function_instantiations.push(FunctionInstantiation {
                handle,
                instantiation_idx,
            });
        }

        for f_handle in module.field_handles() {
            let def_idx = f_handle.owner;
            let owner = structs[def_idx.0 as usize].idx;
            let offset = f_handle.field as usize;
            field_handles.push(FieldHandle { offset, owner });
        }

        for f_inst in module.field_instantiations() {
            let fh_idx = f_inst.handle;
            let owner = field_handles[fh_idx.0 as usize].owner;
            let offset = field_handles[fh_idx.0 as usize].offset;

            field_instantiations.push(FieldInstantiation { offset, owner });
        }

        Ok(Self {
            id: storage_id,
            struct_refs,
            structs,
            struct_instantiations,
            function_refs,
            function_instantiations,
            field_handles,
            field_instantiations,
            function_map,
            single_signature_token_map,
            instantiation_signatures,
        })
    }

    fn struct_at(&self, idx: StructDefinitionIndex) -> CachedTypeIndex {
        self.structs[idx.0 as usize].idx
    }

    fn struct_instantiation_at(&self, idx: u16) -> &StructInstantiation {
        &self.struct_instantiations[idx as usize]
    }

    fn function_at(&self, idx: u16) -> usize {
        self.function_refs[idx as usize]
    }

    fn function_instantiation_at(&self, idx: u16) -> &FunctionInstantiation {
        &self.function_instantiations[idx as usize]
    }

    fn field_count(&self, idx: u16) -> u16 {
        self.structs[idx as usize].field_count
    }

    fn field_instantiation_count(&self, idx: u16) -> u16 {
        self.struct_instantiations[idx as usize].field_count
    }

    fn field_offset(&self, idx: FieldHandleIndex) -> usize {
        self.field_handles[idx.0 as usize].offset
    }

    fn field_instantiation_offset(&self, idx: FieldInstantiationIndex) -> usize {
        self.field_instantiations[idx.0 as usize].offset
    }

    fn single_type_at(&self, idx: SignatureIndex) -> &Type {
        self.single_signature_token_map.get(&idx).unwrap()
    }

    fn instantiation_signature_at(
        &self,
        idx: SignatureIndex,
    ) -> Result<&Vec<Type>, PartialVMError> {
        self.instantiation_signatures.get(&idx).ok_or_else(|| {
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message("Instantiation signature not found".to_string())
        })
    }
}

// A runtime function
// #[derive(Debug)]
// https://github.com/rust-lang/rust/issues/70263
pub(crate) struct Function {
    #[allow(unused)]
    file_format_version: u32,
    index: FunctionDefinitionIndex,
    code: Vec<Bytecode>,
    parameters: SignatureIndex,
    return_: SignatureIndex,
    type_parameters: Vec<AbilitySet>,
    native: Option<NativeFunction>,
    def_is_native: bool,
    module: ModuleId,
    name: Identifier,
    parameters_len: usize,
    locals_len: usize,
    return_len: usize,
}

impl Function {
    fn new(
        natives: &NativeFunctions,
        index: FunctionDefinitionIndex,
        def: &FunctionDefinition,
        module: &CompiledModule,
    ) -> Self {
        let handle = module.function_handle_at(def.function);
        let name = module.identifier_at(handle.name).to_owned();
        let module_id = module.self_id();
        let (native, def_is_native) = if def.is_native() {
            (
                natives.resolve(
                    module_id.address(),
                    module_id.name().as_str(),
                    name.as_str(),
                ),
                true,
            )
        } else {
            (None, false)
        };
        let parameters = handle.parameters;
        let parameters_len = module.signature_at(parameters).0.len();
        // Native functions do not have a code unit
        let (code, locals_len) = match &def.code {
            Some(code) => (
                code.code.clone(),
                parameters_len + module.signature_at(code.locals).0.len(),
            ),
            None => (vec![], 0),
        };
        let return_ = handle.return_;
        let return_len = module.signature_at(return_).0.len();
        let type_parameters = handle.type_parameters.clone();
        Self {
            file_format_version: module.version(),
            index,
            code,
            parameters,
            return_,
            type_parameters,
            native,
            def_is_native,
            module: module_id,
            name,
            parameters_len,
            locals_len,
            return_len,
        }
    }

    #[allow(unused)]
    pub(crate) fn file_format_version(&self) -> u32 {
        self.file_format_version
    }

    pub(crate) fn module_id(&self) -> &ModuleId {
        &self.module
    }

    pub(crate) fn index(&self) -> FunctionDefinitionIndex {
        self.index
    }

    pub(crate) fn get_resolver<'a>(
        &self,
        link_context: AccountAddress,
        loader: &'a Loader,
    ) -> Resolver<'a> {
        let (compiled, loaded) = loader.get_module(link_context, &self.module);
        Resolver::for_module(loader, compiled, loaded)
    }

    pub(crate) fn local_count(&self) -> usize {
        self.locals_len
    }

    pub(crate) fn arg_count(&self) -> usize {
        self.parameters_len
    }

    pub(crate) fn return_type_count(&self) -> usize {
        self.return_len
    }

    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    pub(crate) fn code(&self) -> &[Bytecode] {
        &self.code
    }

    pub(crate) fn type_parameters(&self) -> &[AbilitySet] {
        &self.type_parameters
    }

    pub(crate) fn pretty_string(&self) -> String {
        let id = &self.module;
        format!(
            "0x{}::{}::{}",
            id.address(),
            id.name().as_str(),
            self.name.as_str()
        )
    }

    #[cfg(any(debug_assertions, feature = "debugging"))]
    pub(crate) fn pretty_short_string(&self) -> String {
        let id = &self.module;
        format!(
            "0x{}::{}::{}",
            id.address().short_str_lossless(),
            id.name().as_str(),
            self.name.as_str()
        )
    }

    pub(crate) fn is_native(&self) -> bool {
        self.def_is_native
    }

    pub(crate) fn get_native(&self) -> PartialVMResult<&UnboxedNativeFunction> {
        if cfg!(feature = "lazy_natives") {
            // If lazy_natives is configured, this is a MISSING_DEPENDENCY error, as we skip
            // checking those at module loading time.
            self.native.as_deref().ok_or_else(|| {
                PartialVMError::new(StatusCode::MISSING_DEPENDENCY)
                    .with_message(format!("Missing Native Function `{}`", self.name))
            })
        } else {
            // Otherwise this error should not happen, hence UNREACHABLE
            self.native.as_deref().ok_or_else(|| {
                PartialVMError::new(StatusCode::UNREACHABLE)
                    .with_message("Missing Native Function".to_string())
            })
        }
    }
}

//
// Internal structures that are saved at the proper index in the proper tables to access
// execution information (interpreter).
// The following structs are internal to the loader and never exposed out.
// The `Loader` will create those struct and the proper table when loading a module.
// The `Resolver` uses those structs to return information to the `Interpreter`.
//

// A function instantiation.
#[derive(Debug)]
struct FunctionInstantiation {
    // index to `ModuleCache::functions` global table
    handle: usize,
    instantiation_idx: SignatureIndex,
}

#[derive(Debug)]
struct StructDef {
    // struct field count
    field_count: u16,
    // `ModuelCache::structs` global table index
    idx: CachedTypeIndex,
}

#[derive(Debug)]
struct StructInstantiation {
    // struct field count
    field_count: u16,
    // `ModuelCache::structs` global table index. It is the generic type.
    def: CachedTypeIndex,
    instantiation_idx: SignatureIndex,
}

// A field handle. The offset is the only used information when operating on a field
#[derive(Debug)]
struct FieldHandle {
    offset: usize,
    // `ModuelCache::structs` global table index. It is the generic type.
    owner: CachedTypeIndex,
}

// A field instantiation. The offset is the only used information when operating on a field
#[derive(Debug)]
struct FieldInstantiation {
    offset: usize,
    // `ModuleCache::structs` global table index. It is the generic type.
    #[allow(unused)]
    owner: CachedTypeIndex,
}

//
// Cache for data associated to a Struct, used for de/serialization and more
//

struct StructInfo {
    runtime_struct_tag: Option<StructTag>,
    defining_struct_tag: Option<StructTag>,
    struct_layout: Option<R::MoveStructLayout>,
    annotated_struct_layout: Option<A::MoveStructLayout>,
    node_count: Option<u64>,
    annotated_node_count: Option<u64>,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum StructTagType {
    Runtime,
    Defining,
}

impl StructInfo {
    fn new() -> Self {
        Self {
            runtime_struct_tag: None,
            defining_struct_tag: None,
            struct_layout: None,
            annotated_struct_layout: None,
            node_count: None,
            annotated_node_count: None,
        }
    }
}

pub(crate) struct TypeCache {
    structs: HashMap<CachedTypeIndex, HashMap<Vec<Type>, StructInfo>>,
}

impl TypeCache {
    fn new() -> Self {
        Self {
            structs: HashMap::new(),
        }
    }
}

/// Maximal depth of a value in terms of type depth.
pub const VALUE_DEPTH_MAX: u64 = 128;

/// Maximal nodes which are allowed when converting to layout. This includes the types of
/// fields for struct types.
const MAX_TYPE_TO_LAYOUT_NODES: u64 = 256;

/// Maximal nodes which are all allowed when instantiating a generic type. This does not include
/// field types of structs.
const MAX_TYPE_INSTANTIATION_NODES: u64 = 128;

impl Loader {
    fn read_cached_struct_tag(
        &self,
        gidx: CachedTypeIndex,
        ty_args: &[Type],
        tag_type: StructTagType,
    ) -> Option<StructTag> {
        let cache = self.type_cache.read();
        let map = cache.structs.get(&gidx)?;
        let info = map.get(ty_args)?;

        match tag_type {
            StructTagType::Runtime => info.runtime_struct_tag.clone(),
            StructTagType::Defining => info.defining_struct_tag.clone(),
        }
    }

    fn struct_gidx_to_type_tag(
        &self,
        gidx: CachedTypeIndex,
        ty_args: &[Type],
        tag_type: StructTagType,
    ) -> PartialVMResult<StructTag> {
        if let Some(cached) = self.read_cached_struct_tag(gidx, ty_args, tag_type) {
            return Ok(cached);
        }

        let ty_arg_tags = ty_args
            .iter()
            .map(|ty| self.type_to_type_tag_impl(ty, tag_type))
            .collect::<PartialVMResult<Vec<_>>>()?;
        let struct_type = self.module_cache.read().struct_at(gidx);

        let mut cache = self.type_cache.write();
        let info = cache
            .structs
            .entry(gidx)
            .or_default()
            .entry(ty_args.to_vec())
            .or_insert_with(StructInfo::new);

        match tag_type {
            StructTagType::Runtime => {
                let tag = StructTag {
                    address: *struct_type.runtime_id.address(),
                    module: struct_type.runtime_id.name().to_owned(),
                    name: struct_type.name.clone(),
                    type_params: ty_arg_tags,
                };

                info.runtime_struct_tag = Some(tag.clone());
                Ok(tag)
            }

            StructTagType::Defining => {
                let tag = StructTag {
                    address: *struct_type.defining_id.address(),
                    module: struct_type.defining_id.name().to_owned(),
                    name: struct_type.name.clone(),
                    type_params: ty_arg_tags,
                };

                info.defining_struct_tag = Some(tag.clone());
                Ok(tag)
            }
        }
    }

    fn type_to_type_tag_impl(
        &self,
        ty: &Type,
        tag_type: StructTagType,
    ) -> PartialVMResult<TypeTag> {
        Ok(match ty {
            Type::Bool => TypeTag::Bool,
            Type::U8 => TypeTag::U8,
            Type::U16 => TypeTag::U16,
            Type::U32 => TypeTag::U32,
            Type::U64 => TypeTag::U64,
            Type::U128 => TypeTag::U128,
            Type::U256 => TypeTag::U256,
            Type::Address => TypeTag::Address,
            Type::Signer => TypeTag::Signer,
            Type::Vector(ty) => {
                TypeTag::Vector(Box::new(self.type_to_type_tag_impl(ty, tag_type)?))
            }
            Type::Datatype(gidx) => TypeTag::Struct(Box::new(self.struct_gidx_to_type_tag(
                *gidx,
                &[],
                tag_type,
            )?)),
            Type::DatatypeInstantiation(inst) => {
                let (gidx, ty_args) = &**inst;
                TypeTag::Struct(Box::new(
                    self.struct_gidx_to_type_tag(*gidx, ty_args, tag_type)?,
                ))
            }
            Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("no type tag for {:?}", ty)),
                );
            }
        })
    }

    fn count_type_nodes(&self, ty: &Type) -> u64 {
        let mut todo = vec![ty];
        let mut result = 0;
        while let Some(ty) = todo.pop() {
            match ty {
                Type::Vector(ty) | Type::Reference(ty) | Type::MutableReference(ty) => {
                    result += 1;
                    todo.push(ty);
                }
                Type::DatatypeInstantiation(inst) => {
                    let (_, ty_args) = &**inst;
                    result += 1;
                    todo.extend(ty_args.iter())
                }
                _ => {
                    result += 1;
                }
            }
        }
        result
    }

    fn struct_gidx_to_type_layout(
        &self,
        gidx: CachedTypeIndex,
        ty_args: &[Type],
        count: &mut u64,
        depth: u64,
    ) -> PartialVMResult<R::MoveStructLayout> {
        if let Some(struct_map) = self.type_cache.read().structs.get(&gidx) {
            if let Some(struct_info) = struct_map.get(ty_args) {
                if let Some(node_count) = &struct_info.node_count {
                    *count += *node_count
                }
                if let Some(layout) = &struct_info.struct_layout {
                    return Ok(layout.clone());
                }
            }
        }

        let count_before = *count;
        let binding = self.module_cache.read().struct_at(gidx);
        let struct_type = binding.get_struct().unwrap();
        let field_tys = struct_type
            .fields
            .iter()
            .map(|ty| self.subst(ty, ty_args))
            .collect::<PartialVMResult<Vec<_>>>()?;
        let field_layouts = field_tys
            .iter()
            .map(|ty| self.type_to_type_layout_impl(ty, count, depth + 1))
            .collect::<PartialVMResult<Vec<_>>>()?;
        let field_node_count = *count - count_before;

        let struct_layout = R::MoveStructLayout::new(field_layouts);

        let mut cache = self.type_cache.write();
        let info = cache
            .structs
            .entry(gidx)
            .or_default()
            .entry(ty_args.to_vec())
            .or_insert_with(StructInfo::new);
        info.struct_layout = Some(struct_layout.clone());
        info.node_count = Some(field_node_count);

        Ok(struct_layout)
    }

    fn type_to_type_layout_impl(
        &self,
        ty: &Type,
        count: &mut u64,
        depth: u64,
    ) -> PartialVMResult<R::MoveTypeLayout> {
        if *count > MAX_TYPE_TO_LAYOUT_NODES {
            return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
        }
        if depth > VALUE_DEPTH_MAX {
            return Err(PartialVMError::new(StatusCode::VM_MAX_VALUE_DEPTH_REACHED));
        }
        *count += 1;
        Ok(match ty {
            Type::Bool => R::MoveTypeLayout::Bool,
            Type::U8 => R::MoveTypeLayout::U8,
            Type::U16 => R::MoveTypeLayout::U16,
            Type::U32 => R::MoveTypeLayout::U32,
            Type::U64 => R::MoveTypeLayout::U64,
            Type::U128 => R::MoveTypeLayout::U128,
            Type::U256 => R::MoveTypeLayout::U256,
            Type::Address => R::MoveTypeLayout::Address,
            Type::Signer => R::MoveTypeLayout::Signer,
            Type::Vector(ty) => R::MoveTypeLayout::Vector(Box::new(
                self.type_to_type_layout_impl(ty, count, depth + 1)?,
            )),
            Type::Datatype(gidx) => R::MoveTypeLayout::Struct(Box::new(
                self.struct_gidx_to_type_layout(*gidx, &[], count, depth)?,
            )),
            Type::DatatypeInstantiation(struct_inst) => {
                let (gidx, ty_args) = &**struct_inst;
                R::MoveTypeLayout::Struct(Box::new(
                    self.struct_gidx_to_type_layout(*gidx, ty_args, count, depth)?,
                ))
            }
            Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("no type layout for {:?}", ty)),
                );
            }
        })
    }

    fn struct_gidx_to_fully_annotated_layout(
        &self,
        gidx: CachedTypeIndex,
        ty_args: &[Type],
        count: &mut u64,
        depth: u64,
    ) -> PartialVMResult<A::MoveStructLayout> {
        if let Some(struct_map) = self.type_cache.read().structs.get(&gidx) {
            if let Some(struct_info) = struct_map.get(ty_args) {
                if let Some(annotated_node_count) = &struct_info.annotated_node_count {
                    *count += *annotated_node_count
                }
                if let Some(layout) = &struct_info.annotated_struct_layout {
                    return Ok(layout.clone());
                }
            }
        }

        let binding = self.module_cache.read().struct_at(gidx);
        let struct_type = binding.get_struct().unwrap();
        if struct_type.fields.len() != struct_type.field_names.len() {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                    "Field types did not match the length of field names in loaded struct"
                        .to_owned(),
                ),
            );
        }
        let count_before = *count;
        let struct_tag = self.struct_gidx_to_type_tag(gidx, ty_args, StructTagType::Defining)?;
        let field_layouts = struct_type
            .field_names
            .iter()
            .zip(&struct_type.fields)
            .map(|(n, ty)| {
                let ty = self.subst(ty, ty_args)?;
                let l = self.type_to_fully_annotated_layout_impl(&ty, count, depth + 1)?;
                Ok(A::MoveFieldLayout::new(n.clone(), l))
            })
            .collect::<PartialVMResult<Vec<_>>>()?;
        let struct_layout = A::MoveStructLayout::new(struct_tag, field_layouts);
        let field_node_count = *count - count_before;

        let mut cache = self.type_cache.write();
        let info = cache
            .structs
            .entry(gidx)
            .or_default()
            .entry(ty_args.to_vec())
            .or_insert_with(StructInfo::new);
        info.annotated_struct_layout = Some(struct_layout.clone());
        info.annotated_node_count = Some(field_node_count);

        Ok(struct_layout)
    }

    fn type_to_fully_annotated_layout_impl(
        &self,
        ty: &Type,
        count: &mut u64,
        depth: u64,
    ) -> PartialVMResult<A::MoveTypeLayout> {
        if *count > MAX_TYPE_TO_LAYOUT_NODES {
            return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
        }
        if depth > VALUE_DEPTH_MAX {
            return Err(PartialVMError::new(StatusCode::VM_MAX_VALUE_DEPTH_REACHED));
        }
        *count += 1;
        Ok(match ty {
            Type::Bool => A::MoveTypeLayout::Bool,
            Type::U8 => A::MoveTypeLayout::U8,
            Type::U16 => A::MoveTypeLayout::U16,
            Type::U32 => A::MoveTypeLayout::U32,
            Type::U64 => A::MoveTypeLayout::U64,
            Type::U128 => A::MoveTypeLayout::U128,
            Type::U256 => A::MoveTypeLayout::U256,
            Type::Address => A::MoveTypeLayout::Address,
            Type::Signer => A::MoveTypeLayout::Signer,
            Type::Vector(ty) => A::MoveTypeLayout::Vector(Box::new(
                self.type_to_fully_annotated_layout_impl(ty, count, depth + 1)?,
            )),
            Type::Datatype(gidx) => A::MoveTypeLayout::Struct(Box::new(
                self.struct_gidx_to_fully_annotated_layout(*gidx, &[], count, depth)?,
            )),
            Type::DatatypeInstantiation(struct_inst) => {
                let (gidx, ty_args) = &**struct_inst;
                A::MoveTypeLayout::Struct(Box::new(
                    self.struct_gidx_to_fully_annotated_layout(*gidx, ty_args, count, depth)?,
                ))
            }
            Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("no type layout for {:?}", ty)),
                );
            }
        })
    }

    pub(crate) fn type_to_type_tag(&self, ty: &Type) -> PartialVMResult<TypeTag> {
        self.type_to_type_tag_impl(ty, StructTagType::Defining)
    }

    pub(crate) fn type_to_runtime_type_tag(&self, ty: &Type) -> PartialVMResult<TypeTag> {
        self.type_to_type_tag_impl(ty, StructTagType::Runtime)
    }

    pub(crate) fn type_to_type_layout(&self, ty: &Type) -> PartialVMResult<R::MoveTypeLayout> {
        let mut count = 0;
        self.type_to_type_layout_impl(ty, &mut count, 1)
    }

    pub(crate) fn type_to_fully_annotated_layout(
        &self,
        ty: &Type,
    ) -> PartialVMResult<A::MoveTypeLayout> {
        let mut count = 0;
        self.type_to_fully_annotated_layout_impl(ty, &mut count, 1)
    }
}

// Public APIs for external uses.
impl Loader {
    pub(crate) fn get_type_layout(
        &self,
        type_tag: &TypeTag,
        move_storage: &impl DataStore,
    ) -> VMResult<R::MoveTypeLayout> {
        let ty = self.load_type(type_tag, move_storage)?;
        self.type_to_type_layout(&ty)
            .map_err(|e| e.finish(Location::Undefined))
    }

    pub(crate) fn get_fully_annotated_type_layout(
        &self,
        type_tag: &TypeTag,
        move_storage: &impl DataStore,
    ) -> VMResult<A::MoveTypeLayout> {
        let ty = self.load_type(type_tag, move_storage)?;
        self.type_to_fully_annotated_layout(&ty)
            .map_err(|e| e.finish(Location::Undefined))
    }
}
