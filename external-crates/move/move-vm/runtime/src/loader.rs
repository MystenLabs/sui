// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    config::VMConfig,
    logging::expect_no_verification_errors,
    native_functions::{NativeFunction, NativeFunctions, UnboxedNativeFunction},
    session::LoadedFunctionInstantiation,
};
use move_binary_format::{
    access::{ModuleAccess, ScriptAccess},
    binary_views::BinaryIndexedView,
    errors::{verification_error, Location, PartialVMError, PartialVMResult, VMResult},
    file_format::{
        AbilitySet, Bytecode, CompiledModule, CompiledScript, Constant, ConstantPoolIndex,
        FieldHandleIndex, FieldInstantiationIndex, FunctionDefinition, FunctionDefinitionIndex,
        FunctionHandleIndex, FunctionInstantiationIndex, Signature, SignatureIndex, SignatureToken,
        StructDefInstantiationIndex, StructDefinitionIndex, StructFieldInformation, TableIndex,
        Visibility,
    },
    IndexKind,
};
use move_bytecode_verifier::{self, cyclic_dependencies, dependencies};
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag, TypeTag},
    metadata::Metadata,
    value::{MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
    vm_status::StatusCode,
};
use move_vm_types::{
    data_store::DataStore,
    loaded_data::runtime_types::{CachedStructIndex, StructType, Type},
};
use parking_lot::RwLock;
use sha3::{Digest, Sha3_256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Debug,
    hash::Hash,
    sync::Arc,
};
use tracing::error;

type ScriptHash = [u8; 32];

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

// A script cache is a map from the hash value of a script and the `Script` itself.
// Script are added in the cache once verified and so getting a script out the cache
// does not require further verification (except for parameters and type parameters)
struct ScriptCache {
    scripts: BinaryCache<ScriptHash, LoadedScript>,
}

impl ScriptCache {
    fn new() -> Self {
        Self {
            scripts: BinaryCache::new(),
        }
    }

    fn get(&self, hash: &ScriptHash) -> Option<(Arc<Function>, Vec<Type>, Vec<Type>)> {
        self.scripts.get(hash).map(|script| {
            (
                script.entry_point(),
                script.parameter_tys.clone(),
                script.return_tys.clone(),
            )
        })
    }

    fn insert(
        &mut self,
        hash: ScriptHash,
        script: LoadedScript,
    ) -> PartialVMResult<(Arc<Function>, Vec<Type>, Vec<Type>)> {
        match self.get(&hash) {
            Some(cached) => Ok(cached),
            None => {
                let script = self.scripts.insert(hash, script)?;
                Ok((
                    script.entry_point(),
                    script.parameter_tys.clone(),
                    script.return_tys.clone(),
                ))
            }
        }
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
    structs: BinaryCache<(ModuleId, Identifier), StructType>,
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
    fn struct_at(&self, idx: CachedStructIndex) -> Arc<StructType> {
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
        let module_view = BinaryIndexedView::Module(module);

        // Add new structs and collect their field signatures
        let mut field_signatures = vec![];
        for (idx, struct_def) in module.struct_defs().iter().enumerate() {
            let struct_handle = module.struct_handle_at(struct_def.struct_handle);
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
                StructType {
                    fields: vec![],
                    field_names,
                    abilities: struct_handle.abilities,
                    type_parameters: struct_handle.type_parameters.clone(),
                    name: name.to_owned(),
                    defining_id,
                    runtime_id: runtime_id.clone(),
                    struct_def: StructDefinitionIndex(idx as u16),
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
                .map(|tok| self.make_type(module_view, tok))
                .collect::<PartialVMResult<_>>()?;
            field_types.push(tys);
        }

        // Add the field types to the newly added structs, processing them in reverse, to line them
        // up with the structs we added at the end of the global cache.
        for (fields, struct_type) in field_types
            .into_iter()
            .rev()
            .zip(self.structs.binaries.iter_mut().rev())
        {
            match Arc::get_mut(struct_type) {
                Some(struct_type) => struct_type.fields = fields,
                None => {
                    // we have pending references to the `Arc` which is impossible,
                    // given the code that adds the `Arc` is above and no reference to
                    // it should exist.
                    // So in the spirit of not crashing we just rewrite the entire `Arc`
                    // over and log the issue.
                    error!("Arc<StructType> cannot have any live reference while publishing");
                    let mut struct_copy = (**struct_type).clone();
                    struct_copy.fields = fields;
                    *struct_type = Arc::new(struct_copy);
                }
            }
        }

        for (idx, func) in module.function_defs().iter().enumerate() {
            let findex = FunctionDefinitionIndex(idx as TableIndex);
            let mut function = Function::new(natives, findex, func, module);
            function.return_types = function
                .return_
                .0
                .iter()
                .map(|tok| self.make_type(module_view, tok))
                .collect::<PartialVMResult<Vec<_>>>()?;
            function.local_types = function
                .locals
                .0
                .iter()
                .map(|tok| self.make_type(module_view, tok))
                .collect::<PartialVMResult<Vec<_>>>()?;
            function.parameter_types = function
                .parameters
                .0
                .iter()
                .map(|tok| self.make_type(module_view, tok))
                .collect::<PartialVMResult<Vec<_>>>()?;
            self.functions.push(Arc::new(function));
        }

        let module = LoadedModule::new(cursor, link_context, storage_id, module, self)?;
        self.loaded_modules
            .insert((link_context, runtime_id), module)
    }

    // `make_type` is the entry point to "translate" a `SignatureToken` to a `Type`
    fn make_type(&self, module: BinaryIndexedView, tok: &SignatureToken) -> PartialVMResult<Type> {
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
            SignatureToken::TypeParameter(idx) => Type::TyParam(*idx as usize),
            SignatureToken::Vector(inner_tok) => {
                Type::Vector(Box::new(self.make_type(module, inner_tok)?))
            }
            SignatureToken::Reference(inner_tok) => {
                Type::Reference(Box::new(self.make_type(module, inner_tok)?))
            }
            SignatureToken::MutableReference(inner_tok) => {
                Type::MutableReference(Box::new(self.make_type(module, inner_tok)?))
            }
            SignatureToken::Struct(sh_idx) => {
                let struct_handle = module.struct_handle_at(*sh_idx);
                let struct_name = module.identifier_at(struct_handle.name);
                let module_handle = module.module_handle_at(struct_handle.module);
                let runtime_id = ModuleId::new(
                    *module.address_identifier_at(module_handle.address),
                    module.identifier_at(module_handle.name).to_owned(),
                );
                let def_idx = self.resolve_struct_by_name(struct_name, &runtime_id)?.0;
                Type::Struct(def_idx)
            }
            SignatureToken::StructInstantiation(sh_idx, tys) => {
                let type_parameters: Vec<_> = tys
                    .iter()
                    .map(|tok| self.make_type(module, tok))
                    .collect::<PartialVMResult<_>>()?;
                let struct_handle = module.struct_handle_at(*sh_idx);
                let struct_name = module.identifier_at(struct_handle.name);
                let module_handle = module.module_handle_at(struct_handle.module);
                let runtime_id = ModuleId::new(
                    *module.address_identifier_at(module_handle.address),
                    module.identifier_at(module_handle.name).to_owned(),
                );
                let def_idx = self.resolve_struct_by_name(struct_name, &runtime_id)?.0;
                Type::StructInstantiation(def_idx, type_parameters)
            }
        };
        Ok(res)
    }

    // Given a ModuleId::struct_name, retrieve the `StructType` and the index associated.
    // Return and error if the type has not been loaded
    fn resolve_struct_by_name(
        &self,
        struct_name: &IdentStr,
        runtime_id: &ModuleId,
    ) -> PartialVMResult<(CachedStructIndex, Arc<StructType>)> {
        match self
            .structs
            .get_with_idx(&(runtime_id.clone(), struct_name.to_owned()))
        {
            Some((idx, struct_)) => Ok((CachedStructIndex(idx), Arc::clone(struct_))),
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

// A Loader is responsible to load scripts and modules and holds the cache of all loaded
// entities. Each cache is protected by a `RwLock`. Operation in the Loader must be thread safe
// (operating on values on the stack) and when cache needs updating the mutex must be taken.
// The `pub(crate)` API is what a Loader offers to the runtime.
pub(crate) struct Loader {
    scripts: RwLock<ScriptCache>,
    module_cache: RwLock<ModuleCache>,
    type_cache: RwLock<TypeCache>,
    natives: NativeFunctions,
    vm_config: VMConfig,
}

impl Loader {
    pub(crate) fn new(natives: NativeFunctions, vm_config: VMConfig) -> Self {
        Self {
            scripts: RwLock::new(ScriptCache::new()),
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
    // Script verification and loading
    //

    // Scripts are verified and dependencies are loaded.
    // Effectively that means modules are cached from leaf to root in the dependency DAG.
    // If a dependency error is found, loading stops and the error is returned.
    // However all modules cached up to that point stay loaded.

    // Entry point for script execution (`MoveVM::execute_script`).
    // Verifies the script if it is not in the cache of scripts loaded.
    // Type parameters are checked as well after every type is loaded.
    pub(crate) fn load_script(
        &self,
        script_blob: &[u8],
        ty_args: &[Type],
        data_store: &impl DataStore,
    ) -> VMResult<(Arc<Function>, LoadedFunctionInstantiation)> {
        // retrieve or load the script
        let mut sha3_256 = Sha3_256::new();
        sha3_256.update(script_blob);
        let hash_value: [u8; 32] = sha3_256.finalize().into();

        let link_context = data_store.link_context();
        let mut scripts = self.scripts.write();
        let (main, parameters, return_) = match scripts.get(&hash_value) {
            Some(cached) => cached,
            None => {
                let ver_script = self.deserialize_and_verify_script(script_blob, data_store)?;
                let script = LoadedScript::new(
                    ver_script,
                    link_context,
                    &hash_value,
                    &self.module_cache.read(),
                )?;
                scripts
                    .insert(hash_value, script)
                    .map_err(|e| e.finish(Location::Script))?
            }
        };

        // verify type arguments
        self.verify_ty_args(main.type_parameters(), ty_args)
            .map_err(|e| e.finish(Location::Script))?;
        let instantiation = LoadedFunctionInstantiation {
            parameters,
            return_,
        };
        Ok((main, instantiation))
    }

    // The process of deserialization and verification is not and it must not be under lock.
    // So when publishing modules through the dependency DAG it may happen that a different
    // thread had loaded the module after this process fetched it from storage.
    // Caching will take care of that by asking for each dependency module again under lock.
    fn deserialize_and_verify_script(
        &self,
        script: &[u8],
        data_store: &impl DataStore,
    ) -> VMResult<CompiledScript> {
        let script = match CompiledScript::deserialize_with_max_version(
            script,
            self.vm_config.max_binary_format_version,
        ) {
            Ok(script) => script,
            Err(err) => {
                error!("[VM] deserializer for script returned error: {:?}", err,);
                let msg = format!("Deserialization error: {:?}", err);
                return Err(PartialVMError::new(StatusCode::CODE_DESERIALIZATION_ERROR)
                    .with_message(msg)
                    .finish(Location::Script));
            }
        };

        match self.verify_script(&script) {
            Ok(_) => {
                // verify and load dependencies, fetching the verified compiled module.
                let deps: VMResult<Vec<_>> = script
                    .immediate_dependencies()
                    .into_iter()
                    .map(|dep| Ok(self.load_module(&dep, data_store)?.0))
                    .collect();

                // verify script linkage
                dependencies::verify_script(&script, deps?.iter().map(Arc::as_ref))?;

                Ok(script)
            }
            Err(err) => {
                error!(
                    "[VM] bytecode verifier returned errors for script: {:?}",
                    err
                );
                Err(err)
            }
        }
    }

    // Script verification steps.
    // See `verify_module()` for module verification steps.
    fn verify_script(&self, script: &CompiledScript) -> VMResult<()> {
        fail::fail_point!("verifier-failpoint-3", |_| { Ok(()) });

        move_bytecode_verifier::verify_script_with_config(&self.vm_config.verifier, script)
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
        let compiled_view = BinaryIndexedView::Module(compiled.as_ref());
        let idx = self
            .module_cache
            .read()
            .resolve_function_by_name(function_name, runtime_id, link_context)
            .map_err(|err| err.finish(Location::Undefined))?;
        let func = self.module_cache.read().function_at(idx);

        let parameters = func
            .parameters
            .0
            .iter()
            .map(|tok| self.module_cache.read().make_type(compiled_view, tok))
            .collect::<PartialVMResult<Vec<_>>>()
            .map_err(|err| err.finish(Location::Undefined))?;

        let return_ = func
            .return_
            .0
            .iter()
            .map(|tok| self.module_cache.read().make_type(compiled_view, tok))
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
    // See `verify_script()` for script verification steps.
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
        move_bytecode_verifier::verify_module_with_config(&self.vm_config.verifier, module)?;
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
    ) -> VMResult<(CachedStructIndex, Arc<StructType>)> {
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
                    Type::Struct(idx)
                } else {
                    let mut type_params = vec![];
                    for ty_param in &struct_tag.type_params {
                        type_params.push(self.load_type(ty_param, data_store)?);
                    }
                    self.verify_ty_args(struct_type.type_param_constraints(), &type_params)
                        .map_err(|e| e.finish(Location::Undefined))?;
                    Type::StructInstantiation(idx, type_params)
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
        let module = CompiledModule::deserialize_with_max_version(
            &bytes,
            self.vm_config.max_binary_format_version,
        )
        .map_err(|err| {
            let msg = format!("Deserialization error: {:?}", err);
            PartialVMError::new(StatusCode::CODE_DESERIALIZATION_ERROR)
                .with_message(msg)
                .finish(Location::Module(storage_id.clone()))
        })
        .map_err(expect_no_verification_errors)?;

        fail::fail_point!("verifier-failpoint-2", |_| { Ok(module.clone()) });

        if self.vm_config.paranoid_type_checks && &module.self_id() != runtime_id {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Module self id mismatch with storage".to_string())
                    .finish(Location::Module(runtime_id.clone())),
            );
        }

        // bytecode verifier checks that can be performed with the module itself
        move_bytecode_verifier::verify_module_with_config(&self.vm_config.verifier, &module)
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
        if let Type::StructInstantiation(_, struct_inst) = ty {
            let mut sum_nodes: usize = 1;
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
    // Both function and script invocation use this function to verify correctness
    // of type arguments provided
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

    fn get_script(&self, hash: &ScriptHash) -> Arc<LoadedScript> {
        Arc::clone(
            self.scripts
                .read()
                .scripts
                .get(hash)
                .expect("Script hash on Function must exist"),
        )
    }

    pub(crate) fn get_struct_type(&self, idx: CachedStructIndex) -> Option<Arc<StructType>> {
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
            Type::Struct(idx) => Ok(self.module_cache.read().struct_at(*idx).abilities),
            Type::StructInstantiation(idx, type_args) => {
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

// A simple wrapper for a `Module` or a `Script` in the `Resolver`
enum BinaryType {
    Module {
        compiled: Arc<CompiledModule>,
        loaded: Arc<LoadedModule>,
    },
    Script(Arc<LoadedScript>),
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
        let binary = BinaryType::Module { compiled, loaded };
        Self { loader, binary }
    }

    fn for_script(loader: &'a Loader, script: Arc<LoadedScript>) -> Self {
        let binary = BinaryType::Script(script);
        Self { loader, binary }
    }

    //
    // Constant resolution
    //

    pub(crate) fn constant_at(&self, idx: ConstantPoolIndex) -> &Constant {
        match &self.binary {
            BinaryType::Module { compiled, .. } => compiled.constant_at(idx),
            BinaryType::Script(script) => script.script.constant_at(idx),
        }
    }

    //
    // Function resolution
    //

    pub(crate) fn function_from_handle(&self, idx: FunctionHandleIndex) -> Arc<Function> {
        let idx = match &self.binary {
            BinaryType::Module { loaded, .. } => loaded.function_at(idx.0),
            BinaryType::Script(script) => script.function_at(idx.0),
        };
        self.loader.function_at(idx)
    }

    pub(crate) fn function_from_instantiation(
        &self,
        idx: FunctionInstantiationIndex,
    ) -> Arc<Function> {
        let func_inst = match &self.binary {
            BinaryType::Module { loaded, .. } => loaded.function_instantiation_at(idx.0),
            BinaryType::Script(script) => script.function_instantiation_at(idx.0),
        };
        self.loader.function_at(func_inst.handle)
    }

    pub(crate) fn instantiate_generic_function(
        &self,
        idx: FunctionInstantiationIndex,
        type_params: &[Type],
    ) -> PartialVMResult<Vec<Type>> {
        let func_inst = match &self.binary {
            BinaryType::Module { loaded, .. } => loaded.function_instantiation_at(idx.0),
            BinaryType::Script(script) => script.function_instantiation_at(idx.0),
        };
        let mut instantiation = vec![];
        for ty in &func_inst.instantiation {
            instantiation.push(self.subst(ty, type_params)?);
        }
        // Check if the function instantiation over all generics is larger
        // than MAX_TYPE_INSTANTIATION_NODES.
        let mut sum_nodes: usize = 1;
        for ty in type_params.iter().chain(instantiation.iter()) {
            sum_nodes = sum_nodes.saturating_add(self.loader.count_type_nodes(ty));
            if sum_nodes > MAX_TYPE_INSTANTIATION_NODES {
                return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
            }
        }
        Ok(instantiation)
    }

    #[allow(unused)]
    pub(crate) fn type_params_count(&self, idx: FunctionInstantiationIndex) -> usize {
        let func_inst = match &self.binary {
            BinaryType::Module { loaded, .. } => loaded.function_instantiation_at(idx.0),
            BinaryType::Script(script) => script.function_instantiation_at(idx.0),
        };
        func_inst.instantiation.len()
    }

    //
    // Type resolution
    //

    pub(crate) fn get_struct_type(&self, idx: StructDefinitionIndex) -> Type {
        let struct_def = match &self.binary {
            BinaryType::Module { loaded, .. } => loaded.struct_at(idx),
            BinaryType::Script(_) => unreachable!("Scripts cannot have type instructions"),
        };
        Type::Struct(struct_def)
    }

    pub(crate) fn instantiate_generic_type(
        &self,
        idx: StructDefInstantiationIndex,
        ty_args: &[Type],
    ) -> PartialVMResult<Type> {
        let struct_inst = match &self.binary {
            BinaryType::Module { loaded, .. } => loaded.struct_instantiation_at(idx.0),
            BinaryType::Script(_) => unreachable!("Scripts cannot have type instructions"),
        };

        // Before instantiating the type, count the # of nodes of all type arguments plus
        // existing type instantiation.
        // If that number is larger than MAX_TYPE_INSTANTIATION_NODES, refuse to construct this type.
        // This prevents constructing larger and lager types via struct instantiation.
        let mut sum_nodes: usize = 1;
        for ty in ty_args.iter().chain(struct_inst.instantiation.iter()) {
            sum_nodes = sum_nodes.saturating_add(self.loader.count_type_nodes(ty));
            if sum_nodes > MAX_TYPE_INSTANTIATION_NODES {
                return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
            }
        }

        Ok(Type::StructInstantiation(
            struct_inst.def,
            struct_inst
                .instantiation
                .iter()
                .map(|ty| self.subst(ty, ty_args))
                .collect::<PartialVMResult<_>>()?,
        ))
    }

    pub(crate) fn get_field_type(&self, idx: FieldHandleIndex) -> PartialVMResult<Type> {
        let handle = match &self.binary {
            BinaryType::Module { loaded, .. } => &loaded.field_handles[idx.0 as usize],
            BinaryType::Script(_) => unreachable!("Scripts cannot have type instructions"),
        };
        Ok(self
            .loader
            .get_struct_type(handle.owner)
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Struct Definition not resolved".to_string())
            })?
            .fields[handle.offset]
            .clone())
    }

    pub(crate) fn instantiate_generic_field(
        &self,
        idx: FieldInstantiationIndex,
        ty_args: &[Type],
    ) -> PartialVMResult<Type> {
        let field_instantiation = match &self.binary {
            BinaryType::Module { loaded, .. } => &loaded.field_instantiations[idx.0 as usize],
            BinaryType::Script(_) => unreachable!("Scripts cannot have type instructions"),
        };
        let struct_type = self
            .loader
            .get_struct_type(field_instantiation.owner)
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Struct Definition not resolved".to_string())
            })?;

        let instantiation_types = field_instantiation
            .instantiation
            .iter()
            .map(|inst_ty| inst_ty.subst(ty_args))
            .collect::<PartialVMResult<Vec<_>>>()?;
        struct_type.fields[field_instantiation.offset].subst(&instantiation_types)
    }

    pub(crate) fn get_struct_fields(
        &self,
        idx: StructDefinitionIndex,
    ) -> PartialVMResult<Arc<StructType>> {
        let idx = match &self.binary {
            BinaryType::Module { loaded, .. } => loaded.struct_at(idx),
            BinaryType::Script(_) => unreachable!("Scripts cannot have type instructions"),
        };
        self.loader.get_struct_type(idx).ok_or_else(|| {
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message("Struct Definition not resolved".to_string())
        })
    }

    pub(crate) fn instantiate_generic_struct_fields(
        &self,
        idx: StructDefInstantiationIndex,
        ty_args: &[Type],
    ) -> PartialVMResult<Vec<Type>> {
        let struct_inst = match &self.binary {
            BinaryType::Module { loaded, .. } => loaded.struct_instantiation_at(idx.0),
            BinaryType::Script(_) => unreachable!("Scripts cannot have type instructions"),
        };
        let struct_type = self
            .loader
            .get_struct_type(struct_inst.def)
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Struct Definition not resolved".to_string())
            })?;

        let instantiation_types = struct_inst
            .instantiation
            .iter()
            .map(|inst_ty| inst_ty.subst(ty_args))
            .collect::<PartialVMResult<Vec<_>>>()?;
        struct_type
            .fields
            .iter()
            .map(|ty| ty.subst(&instantiation_types))
            .collect::<PartialVMResult<Vec<_>>>()
    }

    fn single_type_at(&self, idx: SignatureIndex) -> &Type {
        match &self.binary {
            BinaryType::Module { loaded, .. } => loaded.single_type_at(idx),
            BinaryType::Script(script) => script.single_type_at(idx),
        }
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
        match &self.binary {
            BinaryType::Module { loaded, .. } => loaded.field_offset(idx),
            BinaryType::Script(_) => unreachable!("Scripts cannot have field instructions"),
        }
    }

    pub(crate) fn field_instantiation_offset(&self, idx: FieldInstantiationIndex) -> usize {
        match &self.binary {
            BinaryType::Module { loaded, .. } => loaded.field_instantiation_offset(idx),
            BinaryType::Script(_) => unreachable!("Scripts cannot have field instructions"),
        }
    }

    pub(crate) fn field_count(&self, idx: StructDefinitionIndex) -> u16 {
        match &self.binary {
            BinaryType::Module { loaded, .. } => loaded.field_count(idx.0),
            BinaryType::Script(_) => unreachable!("Scripts cannot have type instructions"),
        }
    }

    pub(crate) fn field_instantiation_count(&self, idx: StructDefInstantiationIndex) -> u16 {
        match &self.binary {
            BinaryType::Module { loaded, .. } => loaded.field_instantiation_count(idx.0),
            BinaryType::Script(_) => unreachable!("Scripts cannot have type instructions"),
        }
    }

    pub(crate) fn field_handle_to_struct(&self, idx: FieldHandleIndex) -> Type {
        match &self.binary {
            BinaryType::Module { loaded, .. } => {
                Type::Struct(loaded.field_handles[idx.0 as usize].owner)
            }
            BinaryType::Script(_) => unreachable!("Scripts cannot have field instructions"),
        }
    }

    pub(crate) fn field_instantiation_to_struct(
        &self,
        idx: FieldInstantiationIndex,
        args: &[Type],
    ) -> PartialVMResult<Type> {
        match &self.binary {
            BinaryType::Module { loaded, .. } => Ok(Type::StructInstantiation(
                loaded.field_instantiations[idx.0 as usize].owner,
                loaded.field_instantiations[idx.0 as usize]
                    .instantiation
                    .iter()
                    .map(|ty| ty.subst(args))
                    .collect::<PartialVMResult<Vec<_>>>()?,
            )),
            BinaryType::Script(_) => unreachable!("Scripts cannot have field instructions"),
        }
    }

    pub(crate) fn type_to_type_layout(&self, ty: &Type) -> PartialVMResult<MoveTypeLayout> {
        self.loader.type_to_type_layout(ty)
    }

    pub(crate) fn type_to_fully_annotated_layout(
        &self,
        ty: &Type,
    ) -> PartialVMResult<MoveTypeLayout> {
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
    struct_refs: Vec<CachedStructIndex>,
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
        let module_view = BinaryIndexedView::Module(module);

        let mut struct_refs = vec![];
        let mut structs = vec![];
        let mut struct_instantiations = vec![];
        let mut function_refs = vec![];
        let mut function_instantiations = vec![];
        let mut field_handles = vec![];
        let mut field_instantiations: Vec<FieldInstantiation> = vec![];
        let mut function_map = HashMap::new();
        let mut single_signature_token_map = BTreeMap::new();

        for struct_handle in module.struct_handles() {
            let struct_name = module.identifier_at(struct_handle.name);
            let module_handle = module.module_handle_at(struct_handle.module);
            let runtime_id = module.module_id_for_handle(module_handle);
            struct_refs.push(cache.resolve_struct_by_name(struct_name, &runtime_id)?.0);
        }

        for struct_def in module.struct_defs() {
            let idx = struct_refs[struct_def.struct_handle.0 as usize];
            let field_count = cache.structs.binaries[idx.0].fields.len() as u16;
            structs.push(StructDef { field_count, idx });
        }

        for struct_inst in module.struct_instantiations() {
            let def = struct_inst.def.0 as usize;
            let struct_def = &structs[def];
            let field_count = struct_def.field_count;
            let mut instantiation = vec![];
            for ty in &module.signature_at(struct_inst.type_parameters).0 {
                instantiation.push(cache.make_type(module_view, ty)?);
            }
            struct_instantiations.push(StructInstantiation {
                field_count,
                def: struct_def.idx,
                instantiation,
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
                                let ty = match module.signature_at(*si).0.get(0) {
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
                                    .insert(*si, cache.make_type(module_view, ty)?);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        for func_inst in module.function_instantiations() {
            let handle = function_refs[func_inst.handle.0 as usize];
            let mut instantiation = vec![];
            for ty in &module.signature_at(func_inst.type_parameters).0 {
                instantiation.push(cache.make_type(module_view, ty)?);
            }
            function_instantiations.push(FunctionInstantiation {
                handle,
                instantiation,
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
            let mut instantiation = vec![];
            for ty in &module.signature_at(f_inst.type_parameters).0 {
                instantiation.push(cache.make_type(module_view, ty)?);
            }
            field_instantiations.push(FieldInstantiation {
                offset,
                owner,
                instantiation,
            });
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
        })
    }

    fn struct_at(&self, idx: StructDefinitionIndex) -> CachedStructIndex {
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
}

/// A `LoadedScript` is very similar to a `CompiledScript` but data is "transformed" to a
/// representation more appropriate to execution.
/// When code executes, indexes in instructions are resolved against runtime structures
/// (rather then "compiled") to make available data needed for execution
/// #[derive(Debug)]
struct LoadedScript {
    // primitive pools
    script: CompiledScript,

    // types as indexes into the Loader type list
    // REVIEW: why is this unused?
    #[allow(dead_code)]
    struct_refs: Vec<CachedStructIndex>,

    // functions as indexes into the Loader function list
    function_refs: Vec<usize>,
    // materialized instantiations, whether partial or not
    function_instantiations: Vec<FunctionInstantiation>,

    // entry point
    main: Arc<Function>,

    // parameters of main
    parameter_tys: Vec<Type>,

    // return values
    return_tys: Vec<Type>,

    // a map of single-token signature indices to type
    single_signature_token_map: BTreeMap<SignatureIndex, Type>,
}

impl LoadedScript {
    fn new(
        script: CompiledScript,
        link_context: AccountAddress,
        script_hash: &ScriptHash,
        cache: &ModuleCache,
    ) -> VMResult<Self> {
        let script_view = BinaryIndexedView::Script(&script);
        let mut struct_refs = vec![];
        for struct_handle in script.struct_handles() {
            let struct_name = script.identifier_at(struct_handle.name);
            let module_handle = script.module_handle_at(struct_handle.module);
            let module_id = ModuleId::new(
                *script.address_identifier_at(module_handle.address),
                script.identifier_at(module_handle.name).to_owned(),
            );
            struct_refs.push(
                cache
                    .resolve_struct_by_name(struct_name, &module_id)
                    .map_err(|e| e.finish(Location::Script))?
                    .0,
            );
        }

        let mut function_refs = vec![];
        for func_handle in script.function_handles().iter() {
            let func_name = script.identifier_at(func_handle.name);
            let module_handle = script.module_handle_at(func_handle.module);
            let module_id = ModuleId::new(
                *script.address_identifier_at(module_handle.address),
                script.identifier_at(module_handle.name).to_owned(),
            );
            let ref_idx = cache
                .resolve_function_by_name(func_name, &module_id, link_context)
                .map_err(|err| err.finish(Location::Undefined))?;
            function_refs.push(ref_idx);
        }

        let mut function_instantiations = vec![];
        for func_inst in script.function_instantiations() {
            let handle = function_refs[func_inst.handle.0 as usize];
            let mut instantiation = vec![];
            for ty in &script.signature_at(func_inst.type_parameters).0 {
                instantiation.push(
                    cache
                        .make_type(script_view, ty)
                        .map_err(|e| e.finish(Location::Script))?,
                );
            }
            function_instantiations.push(FunctionInstantiation {
                handle,
                instantiation,
            });
        }

        let scope = Scope::Script(*script_hash);

        let code: Vec<Bytecode> = script.code.code.clone();
        let parameters = script.signature_at(script.parameters).clone();

        let parameter_tys = parameters
            .0
            .iter()
            .map(|tok| cache.make_type(script_view, tok))
            .collect::<PartialVMResult<Vec<_>>>()
            .map_err(|err| err.finish(Location::Undefined))?;
        let locals = Signature(
            parameters
                .0
                .iter()
                .chain(script.signature_at(script.code.locals).0.iter())
                .cloned()
                .collect(),
        );
        let local_tys = locals
            .0
            .iter()
            .map(|tok| cache.make_type(script_view, tok))
            .collect::<PartialVMResult<Vec<_>>>()
            .map_err(|err| err.finish(Location::Undefined))?;
        let return_ = Signature(vec![]);
        let return_tys = return_
            .0
            .iter()
            .map(|tok| cache.make_type(script_view, tok))
            .collect::<PartialVMResult<Vec<_>>>()
            .map_err(|err| err.finish(Location::Undefined))?;
        let type_parameters = script.type_parameters.clone();
        // TODO: main does not have a name. Revisit.
        let name = Identifier::new("main").unwrap();
        let (native, def_is_native) = (None, false); // Script entries cannot be native
        let main: Arc<Function> = Arc::new(Function {
            file_format_version: script.version(),
            index: FunctionDefinitionIndex(0),
            code,
            parameters,
            return_,
            locals,
            type_parameters,
            native,
            def_is_native,
            def_is_friend_or_private: false,
            scope,
            name,
            return_types: return_tys.clone(),
            local_types: local_tys,
            parameter_types: parameter_tys.clone(),
        });

        let mut single_signature_token_map = BTreeMap::new();
        for bc in &script.code.code {
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
                        let ty = match script.signature_at(*si).0.get(0) {
                            None => {
                                return Err(PartialVMError::new(
                                    StatusCode::VERIFIER_INVARIANT_VIOLATION,
                                )
                                .with_message(
                                    "the type argument for vector-related bytecode \
                                                expects one and only one signature token"
                                        .to_owned(),
                                )
                                .finish(Location::Script));
                            }
                            Some(sig_token) => sig_token,
                        };
                        single_signature_token_map.insert(
                            *si,
                            cache
                                .make_type(script_view, ty)
                                .map_err(|e| e.finish(Location::Script))?,
                        );
                    }
                }
                _ => {}
            }
        }

        Ok(Self {
            script,
            struct_refs,
            function_refs,
            function_instantiations,
            main,
            parameter_tys,
            return_tys,
            single_signature_token_map,
        })
    }

    fn entry_point(&self) -> Arc<Function> {
        self.main.clone()
    }

    fn function_at(&self, idx: u16) -> usize {
        self.function_refs[idx as usize]
    }

    fn function_instantiation_at(&self, idx: u16) -> &FunctionInstantiation {
        &self.function_instantiations[idx as usize]
    }

    fn single_type_at(&self, idx: SignatureIndex) -> &Type {
        self.single_signature_token_map.get(&idx).unwrap()
    }
}

// A simple wrapper for the "owner" of the function (Module or Script)
#[derive(Debug)]
enum Scope {
    Module(ModuleId),
    Script(ScriptHash),
}

// A runtime function
// #[derive(Debug)]
// https://github.com/rust-lang/rust/issues/70263
pub(crate) struct Function {
    #[allow(unused)]
    file_format_version: u32,
    index: FunctionDefinitionIndex,
    code: Vec<Bytecode>,
    parameters: Signature,
    return_: Signature,
    locals: Signature,
    type_parameters: Vec<AbilitySet>,
    native: Option<NativeFunction>,
    def_is_native: bool,
    def_is_friend_or_private: bool,
    scope: Scope,
    name: Identifier,
    return_types: Vec<Type>,
    local_types: Vec<Type>,
    parameter_types: Vec<Type>,
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
        let def_is_friend_or_private = match def.visibility {
            Visibility::Friend | Visibility::Private => true,
            Visibility::Public => false,
        };
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
        let scope = Scope::Module(module_id);
        let parameters = module.signature_at(handle.parameters).clone();
        // Native functions do not have a code unit
        let (code, locals) = match &def.code {
            Some(code) => (
                code.code.clone(),
                Signature(
                    parameters
                        .0
                        .iter()
                        .chain(module.signature_at(code.locals).0.iter())
                        .cloned()
                        .collect(),
                ),
            ),
            None => (vec![], Signature(vec![])),
        };
        let return_ = module.signature_at(handle.return_).clone();
        let type_parameters = handle.type_parameters.clone();
        Self {
            file_format_version: module.version(),
            index,
            code,
            parameters,
            return_,
            locals,
            type_parameters,
            native,
            def_is_native,
            def_is_friend_or_private,
            scope,
            name,
            local_types: vec![],
            return_types: vec![],
            parameter_types: vec![],
        }
    }

    #[allow(unused)]
    pub(crate) fn file_format_version(&self) -> u32 {
        self.file_format_version
    }

    pub(crate) fn module_id(&self) -> Option<&ModuleId> {
        match &self.scope {
            Scope::Module(module_id) => Some(module_id),
            Scope::Script(_) => None,
        }
    }

    pub(crate) fn index(&self) -> FunctionDefinitionIndex {
        self.index
    }

    pub(crate) fn get_resolver<'a>(
        &self,
        link_context: AccountAddress,
        loader: &'a Loader,
    ) -> Resolver<'a> {
        match &self.scope {
            Scope::Module(module_id) => {
                let (compiled, loaded) = loader.get_module(link_context, module_id);
                Resolver::for_module(loader, compiled, loaded)
            }
            Scope::Script(script_hash) => {
                let script = loader.get_script(script_hash);
                Resolver::for_script(loader, script)
            }
        }
    }

    pub(crate) fn local_count(&self) -> usize {
        self.locals.len()
    }

    pub(crate) fn arg_count(&self) -> usize {
        self.parameters.len()
    }

    pub(crate) fn return_type_count(&self) -> usize {
        self.return_.len()
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

    pub(crate) fn local_types(&self) -> &[Type] {
        &self.local_types
    }

    pub(crate) fn return_types(&self) -> &[Type] {
        &self.return_types
    }

    pub(crate) fn parameter_types(&self) -> &[Type] {
        &self.parameter_types
    }

    pub(crate) fn pretty_string(&self) -> String {
        match &self.scope {
            Scope::Script(_) => "Script::main".into(),
            Scope::Module(id) => format!(
                "0x{}::{}::{}",
                id.address(),
                id.name().as_str(),
                self.name.as_str()
            ),
        }
    }

    pub(crate) fn is_native(&self) -> bool {
        self.def_is_native
    }

    pub(crate) fn is_friend_or_private(&self) -> bool {
        self.def_is_friend_or_private
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
    instantiation: Vec<Type>,
}

#[derive(Debug)]
struct StructDef {
    // struct field count
    field_count: u16,
    // `ModuelCache::structs` global table index
    idx: CachedStructIndex,
}

#[derive(Debug)]
struct StructInstantiation {
    // struct field count
    field_count: u16,
    // `ModuelCache::structs` global table index. It is the generic type.
    def: CachedStructIndex,
    instantiation: Vec<Type>,
}

// A field handle. The offset is the only used information when operating on a field
#[derive(Debug)]
struct FieldHandle {
    offset: usize,
    // `ModuelCache::structs` global table index. It is the generic type.
    owner: CachedStructIndex,
}

// A field instantiation. The offset is the only used information when operating on a field
#[derive(Debug)]
struct FieldInstantiation {
    offset: usize,
    // `ModuelCache::structs` global table index. It is the generic type.
    #[allow(unused)]
    owner: CachedStructIndex,
    instantiation: Vec<Type>,
}

//
// Cache for data associated to a Struct, used for de/serialization and more
//

struct StructInfo {
    struct_tag: Option<StructTag>,
    struct_layout: Option<MoveStructLayout>,
    annotated_struct_layout: Option<MoveStructLayout>,
    node_count: Option<usize>,
    annotated_node_count: Option<usize>,
}

impl StructInfo {
    fn new() -> Self {
        Self {
            struct_tag: None,
            struct_layout: None,
            annotated_struct_layout: None,
            node_count: None,
            annotated_node_count: None,
        }
    }
}

pub(crate) struct TypeCache {
    structs: HashMap<CachedStructIndex, HashMap<Vec<Type>, StructInfo>>,
}

impl TypeCache {
    fn new() -> Self {
        Self {
            structs: HashMap::new(),
        }
    }
}

/// Maximal depth of a value in terms of type depth.
const VALUE_DEPTH_MAX: usize = 128;

/// Maximal nodes which are allowed when converting to layout. This includes the the types of
/// fields for struct types.
const MAX_TYPE_TO_LAYOUT_NODES: usize = 256;

/// Maximal nodes which are all allowed when instantiating a generic type. This does not include
/// field types of structs.
const MAX_TYPE_INSTANTIATION_NODES: usize = 128;

impl Loader {
    fn struct_gidx_to_type_tag(
        &self,
        gidx: CachedStructIndex,
        ty_args: &[Type],
    ) -> PartialVMResult<StructTag> {
        if let Some(struct_map) = self.type_cache.read().structs.get(&gidx) {
            if let Some(struct_info) = struct_map.get(ty_args) {
                if let Some(struct_tag) = &struct_info.struct_tag {
                    return Ok(struct_tag.clone());
                }
            }
        }

        let ty_arg_tags = ty_args
            .iter()
            .map(|ty| self.type_to_type_tag(ty))
            .collect::<PartialVMResult<Vec<_>>>()?;
        let struct_type = self.module_cache.read().struct_at(gidx);
        let struct_tag = StructTag {
            address: *struct_type.defining_id.address(),
            module: struct_type.defining_id.name().to_owned(),
            name: struct_type.name.clone(),
            type_params: ty_arg_tags,
        };

        self.type_cache
            .write()
            .structs
            .entry(gidx)
            .or_insert_with(HashMap::new)
            .entry(ty_args.to_vec())
            .or_insert_with(StructInfo::new)
            .struct_tag = Some(struct_tag.clone());

        Ok(struct_tag)
    }

    fn type_to_type_tag_impl(&self, ty: &Type) -> PartialVMResult<TypeTag> {
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
            Type::Vector(ty) => TypeTag::Vector(Box::new(self.type_to_type_tag(ty)?)),
            Type::Struct(gidx) => {
                TypeTag::Struct(Box::new(self.struct_gidx_to_type_tag(*gidx, &[])?))
            }
            Type::StructInstantiation(gidx, ty_args) => {
                TypeTag::Struct(Box::new(self.struct_gidx_to_type_tag(*gidx, ty_args)?))
            }
            Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("no type tag for {:?}", ty)),
                );
            }
        })
    }

    fn count_type_nodes(&self, ty: &Type) -> usize {
        let mut todo = vec![ty];
        let mut result = 0;
        while let Some(ty) = todo.pop() {
            match ty {
                Type::Vector(ty) | Type::Reference(ty) | Type::MutableReference(ty) => {
                    result += 1;
                    todo.push(ty);
                }
                Type::StructInstantiation(_, ty_args) => {
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
        gidx: CachedStructIndex,
        ty_args: &[Type],
        count: &mut usize,
        depth: usize,
    ) -> PartialVMResult<MoveStructLayout> {
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
        let struct_type = self.module_cache.read().struct_at(gidx);
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

        let struct_layout = MoveStructLayout::new(field_layouts);

        let mut cache = self.type_cache.write();
        let info = cache
            .structs
            .entry(gidx)
            .or_insert_with(HashMap::new)
            .entry(ty_args.to_vec())
            .or_insert_with(StructInfo::new);
        info.struct_layout = Some(struct_layout.clone());
        info.node_count = Some(field_node_count);

        Ok(struct_layout)
    }

    fn type_to_type_layout_impl(
        &self,
        ty: &Type,
        count: &mut usize,
        depth: usize,
    ) -> PartialVMResult<MoveTypeLayout> {
        if *count > MAX_TYPE_TO_LAYOUT_NODES {
            return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
        }
        if depth > VALUE_DEPTH_MAX {
            return Err(PartialVMError::new(StatusCode::VM_MAX_VALUE_DEPTH_REACHED));
        }
        Ok(match ty {
            Type::Bool => {
                *count += 1;
                MoveTypeLayout::Bool
            }
            Type::U8 => {
                *count += 1;
                MoveTypeLayout::U8
            }
            Type::U16 => {
                *count += 1;
                MoveTypeLayout::U16
            }
            Type::U32 => {
                *count += 1;
                MoveTypeLayout::U32
            }
            Type::U64 => {
                *count += 1;
                MoveTypeLayout::U64
            }
            Type::U128 => {
                *count += 1;
                MoveTypeLayout::U128
            }
            Type::U256 => {
                *count += 1;
                MoveTypeLayout::U256
            }
            Type::Address => {
                *count += 1;
                MoveTypeLayout::Address
            }
            Type::Signer => {
                *count += 1;
                MoveTypeLayout::Signer
            }
            Type::Vector(ty) => {
                *count += 1;
                MoveTypeLayout::Vector(Box::new(self.type_to_type_layout_impl(
                    ty,
                    count,
                    depth + 1,
                )?))
            }
            Type::Struct(gidx) => {
                *count += 1;
                MoveTypeLayout::Struct(self.struct_gidx_to_type_layout(*gidx, &[], count, depth)?)
            }
            Type::StructInstantiation(gidx, ty_args) => {
                *count += 1;
                MoveTypeLayout::Struct(
                    self.struct_gidx_to_type_layout(*gidx, ty_args, count, depth)?,
                )
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
        gidx: CachedStructIndex,
        ty_args: &[Type],
        count: &mut usize,
        depth: usize,
    ) -> PartialVMResult<MoveStructLayout> {
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

        let struct_type = self.module_cache.read().struct_at(gidx);
        if struct_type.fields.len() != struct_type.field_names.len() {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                    "Field types did not match the length of field names in loaded struct"
                        .to_owned(),
                ),
            );
        }
        let count_before = *count;
        let struct_tag = self.struct_gidx_to_type_tag(gidx, ty_args)?;
        let field_layouts = struct_type
            .field_names
            .iter()
            .zip(&struct_type.fields)
            .map(|(n, ty)| {
                let ty = self.subst(ty, ty_args)?;
                let l = self.type_to_fully_annotated_layout_impl(&ty, count, depth + 1)?;
                Ok(MoveFieldLayout::new(n.clone(), l))
            })
            .collect::<PartialVMResult<Vec<_>>>()?;
        let struct_layout = MoveStructLayout::with_types(struct_tag, field_layouts);
        let field_node_count = *count - count_before;

        let mut cache = self.type_cache.write();
        let info = cache
            .structs
            .entry(gidx)
            .or_insert_with(HashMap::new)
            .entry(ty_args.to_vec())
            .or_insert_with(StructInfo::new);
        info.annotated_struct_layout = Some(struct_layout.clone());
        info.annotated_node_count = Some(field_node_count);

        Ok(struct_layout)
    }

    fn type_to_fully_annotated_layout_impl(
        &self,
        ty: &Type,
        count: &mut usize,
        depth: usize,
    ) -> PartialVMResult<MoveTypeLayout> {
        if *count > MAX_TYPE_TO_LAYOUT_NODES {
            return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
        }
        if depth > VALUE_DEPTH_MAX {
            return Err(PartialVMError::new(StatusCode::VM_MAX_VALUE_DEPTH_REACHED));
        }
        Ok(match ty {
            Type::Bool => MoveTypeLayout::Bool,
            Type::U8 => MoveTypeLayout::U8,
            Type::U16 => MoveTypeLayout::U16,
            Type::U32 => MoveTypeLayout::U32,
            Type::U64 => MoveTypeLayout::U64,
            Type::U128 => MoveTypeLayout::U128,
            Type::U256 => MoveTypeLayout::U256,
            Type::Address => MoveTypeLayout::Address,
            Type::Signer => MoveTypeLayout::Signer,
            Type::Vector(ty) => MoveTypeLayout::Vector(Box::new(
                self.type_to_fully_annotated_layout_impl(ty, count, depth + 1)?,
            )),
            Type::Struct(gidx) => MoveTypeLayout::Struct(
                self.struct_gidx_to_fully_annotated_layout(*gidx, &[], count, depth)?,
            ),
            Type::StructInstantiation(gidx, ty_args) => MoveTypeLayout::Struct(
                self.struct_gidx_to_fully_annotated_layout(*gidx, ty_args, count, depth)?,
            ),
            Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("no type layout for {:?}", ty)),
                );
            }
        })
    }

    pub(crate) fn type_to_type_tag(&self, ty: &Type) -> PartialVMResult<TypeTag> {
        self.type_to_type_tag_impl(ty)
    }

    pub(crate) fn type_to_type_layout(&self, ty: &Type) -> PartialVMResult<MoveTypeLayout> {
        let mut count = 0;
        self.type_to_type_layout_impl(ty, &mut count, 1)
    }

    pub(crate) fn type_to_fully_annotated_layout(
        &self,
        ty: &Type,
    ) -> PartialVMResult<MoveTypeLayout> {
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
    ) -> VMResult<MoveTypeLayout> {
        let ty = self.load_type(type_tag, move_storage)?;
        self.type_to_type_layout(&ty)
            .map_err(|e| e.finish(Location::Undefined))
    }

    pub(crate) fn get_fully_annotated_type_layout(
        &self,
        type_tag: &TypeTag,
        move_storage: &impl DataStore,
    ) -> VMResult<MoveTypeLayout> {
        let ty = self.load_type(type_tag, move_storage)?;
        self.type_to_fully_annotated_layout(&ty)
            .map_err(|e| e.finish(Location::Undefined))
    }
}
