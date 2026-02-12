// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::{
        arena::{Arena, ArenaBox, ArenaVec},
        identifier_interner::{IdentifierInterner, IdentifierKey},
    },
    dbg_println,
    execution::{
        dispatch_tables::{DefinitionMap, IntraPackageKey, PackageVirtualTable, VirtualTableKey},
        values::Value,
    },
    jit::{execution::ast::*, optimization::ast as input},
    natives::functions::NativeFunctions,
    shared::{
        safe_ops::{SafeArithmetic as _, SafeIndex as _},
        types::{DefiningTypeId, OriginalId, VersionId},
        unique_map,
        vm_pointer::VMPointer,
    },
};
use move_binary_format::{
    checked_as,
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        self as FF, CompiledModule, FunctionDefinition, FunctionDefinitionIndex,
        FunctionHandleIndex, SignatureIndex, SignatureToken, StructFieldInformation, TableIndex,
    },
    partial_vm_error,
};
use move_core_types::{
    identifier::Identifier, language_storage::ModuleId, resolver::IntraPackageName,
};

use indexmap::IndexMap;

use std::collections::{BTreeMap, BTreeSet, HashMap};

// -------------------------------------------------------------------------------------------------
// Translation Context and Definitions
// -------------------------------------------------------------------------------------------------

struct PackageContext<'borrows> {
    pub natives: &'borrows NativeFunctions,
    pub interner: &'borrows IdentifierInterner,

    pub type_origin_table: HashMap<IntraPackageKey, DefiningTypeId>,

    pub version_id: VersionId,
    pub original_id: OriginalId,
    // NB: this is under the package's context so we don't need to further resolve by
    // address in this table.
    pub loaded_modules: IndexMap<IdentifierKey, Module>,

    // NB: All things except for types are allocated into this arena.
    pub package_arena: Arena,

    pub vtable_funs: DefinitionMap<VMPointer<Function>>,
    pub vtable_types: DefinitionMap<VMPointer<DatatypeDescriptor>>,
}

struct FunctionContext<'pkg_ctxt, 'natives> {
    package_context: &'pkg_ctxt PackageContext<'natives>,
    module: &'pkg_ctxt CompiledModule,
    definitions: Definitions,
}

struct Definitions {
    structs: Vec<VMPointer<StructDef>>,
    struct_instantiations: Vec<VMPointer<StructInstantiation>>,
    #[allow(unused)]
    enums: Vec<VMPointer<EnumDef>>,
    #[allow(unused)]
    enum_instantiations: Vec<VMPointer<EnumInstantiation>>,
    variants: Vec<VMPointer<VariantDef>>,
    variant_instantiations: Vec<VMPointer<VariantInstantiation>>,
    field_handles: Vec<VMPointer<FieldHandle>>,
    field_instantiations: Vec<VMPointer<FieldInstantiation>>,
    function_instantiations: Vec<VMPointer<FunctionInstantiation>>,
    signatures: Vec<VMPointer<ArenaVec<ArenaType>>>,
    constants: Vec<VMPointer<Constant>>,
}

impl PackageContext<'_> {
    fn insert_vtable_functions(
        &mut self,
        functions: impl IntoIterator<Item = VMPointer<Function>>,
    ) -> PartialVMResult<()> {
        let funs = functions
            .into_iter()
            .map(|ptr| {
                let name = *ptr.name.intra_package_key();
                (name, ptr)
            })
            .collect::<Vec<_>>();
        self.vtable_funs.extend(funs)
    }

    fn insert_vtable_datatypes(
        &mut self,
        datatype_descriptors: Vec<VMPointer<DatatypeDescriptor>>,
    ) -> PartialVMResult<()> {
        let datatypes = datatype_descriptors
            .into_iter()
            .map(|ptr| {
                let name = ptr.intra_package_key();
                (name, ptr)
            })
            .collect::<Vec<_>>();
        self.vtable_types.extend(datatypes)
    }

    /// Try to resolve a function call (vtable entry) to a direct call (i.e. a call to a function
    /// in the same package). If the vtable key represents an inter-package call this function
    /// will return `None` as the call cannot be resolved to a direct call.
    fn try_resolve_direct_function_call(
        &self,
        vtable_entry: &VirtualTableKey,
    ) -> PartialVMResult<Option<VMPointer<Function>>> {
        // We are calling into a different package so we cannot resolve this to a direct call.
        if vtable_entry.package_key() != self.original_id {
            return Ok(None);
        }
        match self.vtable_funs.get(vtable_entry.intra_package_key()) {
            Some(fun_ptr) => Ok(Some(fun_ptr.ptr_clone())),
            None => Err(partial_vm_error!(
                FUNCTION_RESOLUTION_FAILURE,
                "Function not found in vtable with name: {}::{}",
                self.version_id,
                self.interner.resolve_ident(
                    &vtable_entry.intra_package_key().member_name,
                    "function name"
                )
            )),
        }
    }

    fn arena_vec<T>(
        &self,
        items: impl ExactSizeIterator<Item = T>,
    ) -> PartialVMResult<ArenaVec<T>> {
        self.package_arena.alloc_vec(items)
    }

    fn arena_box<T>(&self, item: T) -> PartialVMResult<ArenaBox<T>> {
        self.package_arena.alloc_box(item)
    }
}

impl FunctionContext<'_, '_> {
    fn get_vec_type(
        &self,
        signature_index: &SignatureIndex,
    ) -> PartialVMResult<VMPointer<ArenaType>> {
        let Some(tys) = self.definitions.signatures.get(signature_index.0 as usize) else {
            return Err(partial_vm_error!(
                VERIFIER_INVARIANT_VIOLATION,
                "could not find the signature for a vector-related bytecode \
                        in the signature table"
            ));
        };
        if tys.to_ref().len() != 1 {
            return Err(partial_vm_error!(
                VERIFIER_INVARIANT_VIOLATION,
                "the type argument for vector-related bytecode \
                    expects one and only one signature token"
            ));
        };
        let ty = VMPointer::from_ref(tys.to_ref().safe_get(0)?);
        Ok(ty)
    }
}

// -------------------------------------------------------------------------------------------------
// Package Translation
// -------------------------------------------------------------------------------------------------

pub fn package(
    natives: &NativeFunctions,
    interner: &IdentifierInterner,
    verified_package: input::Package,
) -> PartialVMResult<Package> {
    let version_id = verified_package.version_id;
    let original_id = verified_package.original_id;
    let (module_ids_in_pkg, package_modules): (BTreeSet<_>, Vec<_>) =
        verified_package.modules.into_iter().unzip();

    let type_origin_table = verified_package
        .type_origin_table
        .into_iter()
        .map(
            |(
                IntraPackageName {
                    module_name,
                    type_name,
                },
                origin_id,
            )| {
                Ok((
                    IntraPackageKey {
                        module_name: interner.intern_identifier(&module_name),
                        member_name: interner.intern_identifier(&type_name),
                    },
                    origin_id,
                ))
            },
        )
        .collect::<PartialVMResult<_>>()?;

    let mut package_context = PackageContext {
        natives,
        interner,
        version_id,
        original_id,
        loaded_modules: IndexMap::new(),
        package_arena: Arena::new_bounded(),
        vtable_funs: DefinitionMap::empty(),
        vtable_types: DefinitionMap::empty(),
        type_origin_table,
    };

    modules(&mut package_context, &module_ids_in_pkg, &package_modules)?;

    let PackageContext {
        version_id,
        natives: _,
        interner: _,
        original_id,
        loaded_modules,
        package_arena,
        vtable_funs,
        vtable_types,
        type_origin_table: _,
    } = package_context;

    let vtable = PackageVirtualTable::new(vtable_funs, vtable_types);

    Ok(Package {
        version_id,
        original_id,
        loaded_modules,
        package_arena,
        vtable,
    })
}

fn modules(
    package_context: &mut PackageContext<'_>,
    pkg_module_ids: &BTreeSet<ModuleId>,
    package_modules: &[input::Module],
) -> PartialVMResult<()> {
    use std::collections::BTreeMap;

    #[derive(Copy, Clone, Eq, PartialEq)]
    enum State {
        NotVisited,
        Visiting,
        Visited,
    }

    let input_modules: BTreeMap<ModuleId, &input::Module> = package_modules
        .iter()
        .map(|m| (m.compiled_module.self_id(), m))
        .collect();

    // Model a DFS over the module dependency graph to load modules in dependency order.
    let mut state: BTreeMap<ModuleId, State> = BTreeMap::new();

    for root_id in input_modules.keys() {
        let root_key = package_context.interner.intern_ident_str(root_id.name());

        // Skip if we already fully processed this module.
        if matches!(state.get(root_id), Some(State::Visited)) {
            debug_assert!(package_context.loaded_modules.contains_key(&root_key));
            continue;
        }

        let mut stack: Vec<ModuleId> = Vec::new();
        stack.push(root_id.clone());

        while let Some(cur_id) = stack.pop() {
            let cur_state = *state.get(&cur_id).unwrap_or(&State::NotVisited);
            let cur_key = package_context.interner.intern_ident_str(cur_id.name());

            match cur_state {
                State::Visited => {
                    debug_assert!(package_context.loaded_modules.contains_key(&cur_key));
                    continue;
                }
                State::Visiting => {
                    // All deps done, now load. No need to check if already loaded: a module
                    // enters Visiting only from NotVisited, and transitions to Visited
                    // immediately after loading, so we never reach here twice for the same module.
                    let input_module = input_modules.get(&cur_id).ok_or_else(|| {
                        partial_vm_error!(
                            UNKNOWN_INVARIANT_VIOLATION_ERROR,
                            "Module {} not found in initial modules",
                            cur_id
                        )
                    })?;
                    let loaded_module =
                        module(package_context, package_context.version_id, input_module)?;
                    if package_context
                        .loaded_modules
                        .insert(cur_key, loaded_module)
                        .is_some()
                    {
                        return Err(partial_vm_error!(
                            UNKNOWN_INVARIANT_VIOLATION_ERROR,
                            "Module {} already loaded in package context",
                            cur_id
                        ));
                    }
                    state.insert(cur_id, State::Visited);
                }
                State::NotVisited => {
                    let input_module = input_modules.get(&cur_id).ok_or_else(|| {
                        partial_vm_error!(
                            UNKNOWN_INVARIANT_VIOLATION_ERROR,
                            "Module {} not found in initial modules",
                            cur_id
                        )
                    })?;

                    // Collect unvisited dependencies, checking for cycles.
                    let unvisited_deps: Vec<_> = input_module
                        .compiled_module
                        .immediate_dependencies()
                        .iter()
                        .filter(|dep| pkg_module_ids.contains(dep) && *dep != &cur_id)
                        .filter_map(|dep| {
                            match state.get(dep).copied().unwrap_or(State::NotVisited) {
                                State::Visited => None,
                                State::Visiting => Some(Err(partial_vm_error!(
                                    UNKNOWN_INVARIANT_VIOLATION_ERROR,
                                    "Cycle detected when loading module for package: {}",
                                    dep
                                ))),
                                State::NotVisited => Some(Ok(dep.clone())),
                            }
                        })
                        .collect::<Result<_, _>>()?;

                    if unvisited_deps.is_empty() {
                        // All dependencies are loaded, we are ready to load.
                        let loaded_module =
                            module(package_context, package_context.version_id, input_module)?;
                        if package_context
                            .loaded_modules
                            .insert(cur_key, loaded_module)
                            .is_some()
                        {
                            return Err(partial_vm_error!(
                                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                                "Module {} already loaded in package context",
                                cur_id
                            ));
                        }
                        state.insert(cur_id, State::Visited);
                    } else {
                        // Has unvisited dependencies: mark as Visiting and process deps first.
                        if state.insert(cur_id.clone(), State::Visiting).is_some() {
                            return Err(partial_vm_error!(
                                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                                "Module {} added to load queue as unvisited twice",
                                cur_id
                            ));
                        }
                        stack.push(cur_id);
                        stack.extend(unvisited_deps);
                    }
                }
            }
        }
    }

    Ok(())
}

// -------------------------------------------------------------------------------------------------
// Module Translation

fn module(
    context: &mut PackageContext<'_>,
    version_id: VersionId,
    module: &input::Module,
) -> PartialVMResult<Module> {
    let self_id = module.compiled_module.self_id();
    dbg_println!("Loading module: {}", self_id);

    let mkey = context.interner.intern_ident_str(self_id.name());

    let cmodule = &module.compiled_module;

    // Initialize module data
    let type_refs = initialize_type_refs(context, cmodule)?;

    let (structs, enums, datatype_descriptors) = datatypes(context, &version_id, &mkey, cmodule)?;
    let (instantiation_signatures, _signature_map) = cache_signatures(context, cmodule)?;
    dbg_println!("Module types loaded");

    let sig_pointers = instantiation_signatures
        .iter()
        .map(VMPointer::from_ref)
        .collect::<Vec<_>>();

    context.insert_vtable_datatypes(datatype_descriptors.to_ptrs())?;

    let struct_instantiations = struct_instantiations(context, cmodule, &structs, &sig_pointers)?;
    let enum_instantiations = enum_instantiations(context, cmodule, &enums, &sig_pointers)?;

    // Process field handles and instantiations
    let field_handles = field_handles(context, cmodule, &structs)?;
    let field_instantiations = field_instantiations(context, cmodule, &field_handles)?;

    let constants = constants(context, cmodule)?;

    let variant_handles = variant_handles(cmodule, &enums)?;
    let variant_instantiations = variant_instantiations(context, cmodule, &enum_instantiations)?;

    // Function loading is effectful; they all go into the arena.
    // This happens last because it relies on the definitions above to rewrite the bytecode appropriately.
    // It happens in three steps:
    // 1. Preallocate all functions (without bodies) so that we have stable pointers for vtable
    //    entries.
    // 2. Add the function pointers to the package context's vtable.
    // 3. Process function instantiations, which require the stable function pointers.
    // 4. Finally, process the function bodies.

    let (functions, fun_map) = preallocate_functions(context, &mkey, cmodule)?;
    context.insert_vtable_functions(fun_map.into_values())?;

    let function_instantiations = function_instantiations(context, cmodule, &sig_pointers)?;

    let definitions = Definitions {
        variants: variant_handles,
        structs: structs.to_ptrs(),
        struct_instantiations: struct_instantiations.to_ptrs(),
        enums: enums.to_ptrs(),
        enum_instantiations: enum_instantiations.to_ptrs(),
        variant_instantiations: variant_instantiations.to_ptrs(),
        field_handles: field_handles.to_ptrs(),
        field_instantiations: field_instantiations.to_ptrs(),
        function_instantiations: function_instantiations.to_ptrs(),
        constants: constants.to_ptrs(),
        signatures: instantiation_signatures.to_ptrs(),
    };

    let functions = function_bodies(context, module, definitions, functions)?;

    // Build and return the module
    Ok(Module {
        id: self_id,
        datatype_descriptors,
        type_refs,
        structs,
        struct_instantiations,
        enums,
        enum_instantiations,
        functions,
        function_instantiations,
        field_handles,
        field_instantiations,
        instantiation_signatures,
        variant_instantiations,
        constants,
    })
}

// -------------------------------------------------------------------------------------------------
// Type Reference Translation
// -------------------------------------------------------------------------------------------------

fn initialize_type_refs(
    context: &mut PackageContext<'_>,
    module: &CompiledModule,
) -> PartialVMResult<ArenaVec<IntraPackageKey>> {
    let type_refs = module
        .datatype_handles()
        .iter()
        .map(|datatype_handle| {
            let struct_name = context
                .interner
                .intern_ident_str(module.identifier_at(datatype_handle.name));
            let module_handle = module.module_handle_at(datatype_handle.module);
            let original_id = module.module_id_for_handle(module_handle);
            let module_name = context.interner.intern_ident_str(original_id.name());
            Ok(IntraPackageKey {
                module_name,
                member_name: struct_name,
            })
        })
        .collect::<PartialVMResult<Vec<_>>>()?;
    context.arena_vec(type_refs.into_iter())
}

// -------------------------------------------------------------------------------------------------
// Datatype Translation
// -------------------------------------------------------------------------------------------------

/// Loads structs and enums, returning them and their datatype descriptors (for vtable entry).
fn datatypes(
    context: &mut PackageContext,
    version_id: &VersionId,
    module_name: &IdentifierKey,
    module: &CompiledModule,
) -> PartialVMResult<(
    ArenaVec<StructDef>,
    ArenaVec<EnumDef>,
    ArenaVec<DatatypeDescriptor>,
)> {
    fn resolve_member_name(context: &PackageContext, name: &VirtualTableKey) -> Identifier {
        context
            .interner
            .resolve_ident(&name.intra_package_key().member_name, "datatype name")
    }

    // NB: It is the responsibility of the adapter to determine the correct type origin table,
    // and pass a full and complete representation of it in with the package.
    fn defining_id(
        context: &PackageContext,
        version_id: &VersionId,
        name: &VirtualTableKey,
    ) -> PartialVMResult<ModuleIdKey> {
        let defining_address = context
            .type_origin_table
            .get(name.intra_package_key())
            .ok_or_else(|| {
                partial_vm_error!(
                    LOOKUP_FAILED,
                    "Type origin not found for type {}",
                    name.to_string(context.interner),
                )
            })?;
        dbg_println!("Package ID: {:?}", version_id);
        dbg_println!("Defining Address: {:?}", defining_address);
        let module_id = name.intra_package_key().module_name;
        Ok(ModuleIdKey::from_parts(*defining_address, module_id))
    }

    let original_id = module.self_id();
    let original_address = *original_id.address();

    let structs: ArenaVec<StructDef> = structs(context, &original_address, module_name, module)?;
    let enums: ArenaVec<EnumDef> = enums(context, &original_address, module_name, module)?;

    let module_original_id = ModuleIdKey::from_parts(original_address, *module_name);

    let struct_descriptors = structs
        .iter()
        .map(|struct_| {
            let name = resolve_member_name(context, &struct_.def_vtable_key);
            let defining_id = defining_id(context, version_id, &struct_.def_vtable_key)?;
            let original_id = module_original_id;
            let datatype_info =
                context.arena_box(Datatype::Struct(VMPointer::from_ref(struct_)))?;
            let name = context.interner.intern_identifier(&name);
            let descriptor = DatatypeDescriptor::new(name, defining_id, original_id, datatype_info);
            Ok(descriptor)
        })
        .collect::<PartialVMResult<Vec<_>>>()?;

    let enum_descriptors = enums
        .iter()
        .map(|enum_| {
            let name = resolve_member_name(context, &enum_.def_vtable_key);
            let defining_id = defining_id(context, version_id, &enum_.def_vtable_key)?;
            let original_id = module_original_id;
            let datatype_info = context.arena_box(Datatype::Enum(VMPointer::from_ref(enum_)))?;
            let name = context.interner.intern_identifier(&name);
            let descriptor = DatatypeDescriptor::new(name, defining_id, original_id, datatype_info);
            Ok(descriptor)
        })
        .collect::<PartialVMResult<Vec<_>>>()?;

    let datatype_descriptors = struct_descriptors
        .into_iter()
        .chain(enum_descriptors)
        .collect::<Vec<_>>();
    let datatype_descriptors = context.arena_vec(datatype_descriptors.into_iter())?;

    Ok((structs, enums, datatype_descriptors))
}

fn structs(
    context: &mut PackageContext<'_>,
    original_id: &OriginalId,
    module_name: &IdentifierKey,
    module: &CompiledModule,
) -> PartialVMResult<ArenaVec<StructDef>> {
    let struct_defs = module
        .struct_defs()
        .iter()
        .map(|struct_def| {
            let struct_handle = module.datatype_handle_at(struct_def.struct_handle);

            let name = module.identifier_at(struct_handle.name);
            let member_name = context.interner.intern_ident_str(name);
            let def_vtable_key =
                VirtualTableKey::from_parts(*original_id, *module_name, member_name);

            let abilities = struct_handle.abilities;

            let struct_module_handle = module.module_handle_at(struct_handle.module);
            dbg_println!("Indexing type {:?} at {:?}", name, struct_module_handle);

            let StructFieldInformation::Declared(fields) = &struct_def.field_information else {
                return Err(partial_vm_error!(
                    UNKNOWN_INVARIANT_VIOLATION_ERROR,
                    "Expected declared fields for struct definition, found native struct"
                ));
            };

            let fields = fields
                .iter()
                .map(|f| make_arena_type(context, module, &f.signature.0))
                .collect::<PartialVMResult<Vec<ArenaType>>>()?;
            let fields = context.arena_vec(fields.into_iter())?;

            let field_names = match &struct_def.field_information {
                StructFieldInformation::Native => vec![],
                StructFieldInformation::Declared(field_info) => field_info
                    .iter()
                    .map(|f| module.identifier_at(f.name).to_owned())
                    .collect(),
            };
            let field_names: Vec<IdentifierKey> = field_names
                .iter()
                .map(|name| context.interner.intern_identifier(name))
                .collect::<Vec<_>>();
            let field_names = context.arena_vec(field_names.into_iter())?;

            let type_parameters =
                context.arena_vec(struct_handle.type_parameters.iter().cloned())?;

            let struct_ = StructDef {
                def_vtable_key,
                type_parameters,
                abilities,
                fields,
                field_names,
            };
            Ok(struct_)
        })
        .collect::<PartialVMResult<Vec<_>>>()?;
    context.arena_vec(struct_defs.into_iter())
}

fn enums(
    context: &mut PackageContext<'_>,
    original_id: &OriginalId,
    module_name: &IdentifierKey,
    module: &CompiledModule,
) -> PartialVMResult<ArenaVec<EnumDef>> {
    // We do this in two passes:
    // 1. We make each outer EnumDef and place it in an ArenaVec so its location is fixed.
    // 2. We generate the variants with backpointers to the enum def.

    let enum_defs = module
        .enum_defs()
        .iter()
        .map(|enum_def| {
            let enum_handle = module.datatype_handle_at(enum_def.enum_handle);

            let name = module.identifier_at(enum_handle.name);
            let member_name = context.interner.intern_ident_str(name);
            let def_vtable_key =
                VirtualTableKey::from_parts(*original_id, *module_name, member_name);

            let enum_module_handle = module.module_handle_at(enum_handle.module);
            dbg_println!("Indexing type {:?} at {:?}", name, enum_module_handle);

            let abilities = enum_handle.abilities;

            let type_parameters = context.arena_vec(enum_handle.type_parameters.iter().cloned())?;

            let variant_count = checked_as!(enum_def.variants.len(), u16)?;

            // Initialize the EnumDef
            let enum_ = EnumDef {
                def_vtable_key,
                abilities,
                type_parameters,
                variant_count,
                variants: ArenaVec::empty(),
            };
            Ok(enum_)
        })
        .collect::<PartialVMResult<Vec<_>>>()?;
    let mut enum_defs = context.arena_vec(enum_defs.into_iter())?;

    debug_assert!(module.enum_defs().len() == enum_defs.len());

    for (module_defn, enum_) in module.enum_defs().iter().zip(enum_defs.iter_mut()) {
        let enum_def = VMPointer::from_ref(enum_ as &EnumDef);
        let variants: Vec<VariantDef> = module_defn
            .variants
            .iter()
            .enumerate()
            .map(|(variant_tag, variant_def)| {
                let variant_tag = checked_as!(variant_tag, u16)?;
                let variant_name = context
                    .interner
                    .intern_ident_str(module.identifier_at(variant_def.variant_name));

                let fields = variant_def
                    .fields
                    .iter()
                    .map(|f| make_arena_type(context, module, &f.signature.0))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                let fields = context.arena_vec(fields.into_iter())?;

                let field_names = variant_def
                    .fields
                    .iter()
                    .map(|f| module.identifier_at(f.name));
                let field_names: Vec<IdentifierKey> = field_names
                    .map(|name| context.interner.intern_ident_str(name))
                    .collect::<Vec<_>>();
                let field_names = context.arena_vec(field_names.into_iter())?;

                let variant = VariantDef {
                    variant_tag,
                    variant_name,
                    fields,
                    field_names,
                    enum_def: enum_def.ptr_clone(),
                };

                Ok(variant)
            })
            .collect::<PartialVMResult<_>>()?;
        let variants = context.arena_vec(variants.into_iter())?;

        // Tie the knot
        enum_.variants = variants;
    }

    Ok(enum_defs)
}

fn cache_signatures(
    context: &mut PackageContext<'_>,
    module: &CompiledModule,
) -> PartialVMResult<(
    ArenaVec<ArenaVec<ArenaType>>,
    BTreeMap<SignatureIndex, VMPointer<ArenaVec<ArenaType>>>,
)> {
    let signatures = module
        .signatures()
        .iter()
        .map(|sig| {
            let tys = sig
                .0
                .iter()
                .map(|ty| make_arena_type(context, module, ty))
                .collect::<PartialVMResult<Vec<_>>>()?;
            context.arena_vec(tys.into_iter())
        })
        .collect::<PartialVMResult<Vec<_>>>()?;
    let signatures = context.arena_vec(signatures.into_iter())?;
    let signature_map = signatures
        .iter()
        .enumerate()
        .map(|(ndx, entry)| {
            Ok((
                SignatureIndex::new(checked_as!(ndx, u16)?),
                VMPointer::from_ref(entry),
            ))
        })
        .collect::<PartialVMResult<BTreeMap<_, _>>>()?;
    Ok((signatures, signature_map))
}

// -------------------------------------------------------------------------------------------------
// Handle Translation
// -------------------------------------------------------------------------------------------------

fn field_handles(
    context: &mut PackageContext<'_>,
    module: &CompiledModule,
    structs: &[StructDef],
) -> PartialVMResult<ArenaVec<FieldHandle>> {
    let field_handles = module
        .field_handles()
        .iter()
        .map(|f_handle| {
            let def_idx = f_handle.owner;
            let owner = structs.safe_get(def_idx.0 as usize)?.def_vtable_key.clone();
            let offset = f_handle.field as usize;
            Ok(FieldHandle { offset, owner })
        })
        .collect::<PartialVMResult<Vec<_>>>()?;
    context.arena_vec(field_handles.into_iter())
}

/// [SAFETY] This assumes the elements in `enums` are stable and will not move.
/// NB: This returns a vector of pointers, as we do not need to store these -- they are already
/// fixed in the arena under the EnumDefs provided.
fn variant_handles(
    module: &CompiledModule,
    enums: &[EnumDef],
) -> PartialVMResult<Vec<VMPointer<VariantDef>>> {
    module
        .variant_handles()
        .iter()
        .map(|variant_handle| {
            let FF::VariantHandle { enum_def, variant } = variant_handle;
            let enum_ = enums.safe_get(enum_def.0 as usize)?;
            let variant_ = enum_.variants.safe_get(*variant as usize)?;
            Ok(VMPointer::from_ref(variant_))
        })
        .collect::<PartialVMResult<Vec<_>>>()
}

// -------------------------------------------------------------------------------------------------
// Instantiation Translation
// -------------------------------------------------------------------------------------------------

fn struct_instantiations(
    context: &mut PackageContext<'_>,
    module: &CompiledModule,
    structs: &[StructDef],
    signatures: &[VMPointer<ArenaVec<ArenaType>>],
) -> PartialVMResult<ArenaVec<StructInstantiation>> {
    let struct_insts = module
        .struct_instantiations()
        .iter()
        .map(|struct_inst| {
            let def = struct_inst.def.0 as usize;
            let struct_def = &structs.safe_get(def)?;
            let field_count = checked_as!(struct_def.fields.len(), u16)?;
            let instantiation_idx = struct_inst.type_parameters;
            let type_params = signatures
                .safe_get(instantiation_idx.0 as usize)?
                .ptr_clone();

            Ok(StructInstantiation {
                field_count,
                def_vtable_key: struct_def.def_vtable_key.clone(),
                type_params,
            })
        })
        .collect::<PartialVMResult<Vec<_>>>()?;
    context.arena_vec(struct_insts.into_iter())
}

fn enum_instantiations(
    context: &mut PackageContext<'_>,
    module: &CompiledModule,
    enums: &[EnumDef],
    signatures: &[VMPointer<ArenaVec<ArenaType>>],
) -> PartialVMResult<ArenaVec<EnumInstantiation>> {
    let enum_insts = module
        .enum_instantiations()
        .iter()
        .map(|enum_inst| {
            let def = enum_inst.def.0 as usize;
            let enum_def = enums.safe_get(def)?;
            let variant_count_map = context.arena_vec(
                enum_def
                    .variants
                    .iter()
                    .map(|v| checked_as!(v.fields.len(), u16))
                    .collect::<PartialVMResult<Vec<_>>>()?
                    .into_iter(),
            )?;
            let instantiation_idx = enum_inst.type_parameters;
            let type_params = signatures
                .safe_get(instantiation_idx.0 as usize)?
                .ptr_clone();

            let def_vtable_key = enum_def.def_vtable_key.clone();
            let enum_def = VMPointer::from_ref(enum_def);

            Ok(EnumInstantiation {
                variant_count_map,
                enum_def,
                def_vtable_key,
                type_params,
            })
        })
        .collect::<PartialVMResult<Vec<_>>>()?;
    context.arena_vec(enum_insts.into_iter())
}

fn function_instantiations(
    package_context: &mut PackageContext,
    module: &CompiledModule,
    signatures: &[VMPointer<ArenaVec<ArenaType>>],
) -> PartialVMResult<ArenaVec<FunctionInstantiation>> {
    dbg_println!(flag: function_list_sizes, "handle size: {}", module.function_handles().len());

    let fun_insts = module
        .function_instantiations()
        .iter()
        .map(|fun_inst| {
            let handle = call(package_context, module, fun_inst.handle)?;
            let instantiation = signatures
                .safe_get(fun_inst.type_parameters.0 as usize)?
                .ptr_clone();

            Ok(FunctionInstantiation {
                handle,
                instantiation,
            })
        })
        .collect::<PartialVMResult<Vec<_>>>()?;
    package_context.arena_vec(fun_insts.into_iter())
}

fn field_instantiations(
    context: &mut PackageContext<'_>,
    module: &CompiledModule,
    field_handles: &[FieldHandle],
) -> PartialVMResult<ArenaVec<FieldInstantiation>> {
    let field_instantiations = module
        .field_instantiations()
        .iter()
        .map(|f_inst| {
            let fh_idx = f_inst.handle;
            let owner = field_handles.safe_get(fh_idx.0 as usize)?.owner.clone();
            let offset = field_handles.safe_get(fh_idx.0 as usize)?.offset;

            Ok(FieldInstantiation { offset, owner })
        })
        .collect::<PartialVMResult<Vec<_>>>()?;
    context.arena_vec(field_instantiations.into_iter())
}

/// [SAFETY] This assumes the elements in `enum_instantiations` are stable and will not move.
fn variant_instantiations(
    context: &mut PackageContext<'_>,
    module: &CompiledModule,
    enum_instantiations: &[EnumInstantiation],
) -> PartialVMResult<ArenaVec<VariantInstantiation>> {
    let variant_insts = module
        .variant_instantiation_handles()
        .iter()
        .map(|v_inst| {
            let FF::VariantInstantiationHandle { enum_def, variant } = v_inst;
            let enum_inst = VMPointer::from_ref(enum_instantiations.safe_get(enum_def.0 as usize)?);
            let variant =
                VMPointer::from_ref(enum_inst.enum_def.variants.safe_get(*variant as usize)?);
            Ok(VariantInstantiation { enum_inst, variant })
        })
        .collect::<PartialVMResult<Vec<_>>>()?;
    context.arena_vec(variant_insts.into_iter())
}

// -------------------------------------------------------------------------------------------------
// Constant Translation
// -------------------------------------------------------------------------------------------------

fn constants(
    context: &mut PackageContext<'_>,
    module: &CompiledModule,
) -> PartialVMResult<ArenaVec<Constant>> {
    let constants = module
        .constant_pool()
        .iter()
        .map(|constant| {
            let value = Value::deserialize_constant(constant)
                .ok_or_else(|| {
                    partial_vm_error!(
                        VERIFIER_INVARIANT_VIOLATION,
                        "Verifier failed to verify the deserialization of constants"
                    )
                })?
                .into_constant_value(&context.package_arena)?;
            let type_ = make_arena_type(context, module, &constant.type_)?;
            let size = constant.data.len() as u64;
            let const_ = Constant { value, type_, size };
            Ok(const_)
        })
        .collect::<PartialVMResult<Vec<_>>>()?;
    context.arena_vec(constants.into_iter())
}

// -------------------------------------------------------------------------------------------------
// Function Translation
// -------------------------------------------------------------------------------------------------

fn preallocate_functions(
    package_context: &mut PackageContext<'_>,
    module_name: &IdentifierKey,
    module: &CompiledModule,
) -> Result<
    (
        ArenaVec<Function>,
        HashMap<VirtualTableKey, VMPointer<Function>>,
    ),
    PartialVMError,
> {
    dbg_println!(flag: function_list_sizes, "allocating {} functions", module.function_defs().len());

    let prealloc_functions: Vec<Function> = module
        .function_defs()
        .iter()
        .enumerate()
        .map(|(ndx, fun)| {
            let findex = FunctionDefinitionIndex(checked_as!(ndx, TableIndex)?);
            alloc_function(package_context, module_name, module, findex, fun)
        })
        .collect::<PartialVMResult<Vec<_>>>()?;
    let loaded_functions = package_context.arena_vec(prealloc_functions.into_iter())?;
    let fun_map = unique_map(
        loaded_functions
            .iter()
            .map(|fun| (fun.name.clone(), VMPointer::from_ref(fun))),
    )
    .map_err(|key| {
        partial_vm_error!(
            UNKNOWN_INVARIANT_VIOLATION_ERROR,
            "Duplicate function key {}::{}",
            package_context.version_id,
            key.member_name(package_context.interner)
        )
    })?;
    Ok((loaded_functions, fun_map))
}

fn function_bodies(
    package_context: &mut PackageContext,
    module: &input::Module,
    definitions: Definitions,
    functions: ArenaVec<Function>,
) -> PartialVMResult<ArenaVec<Function>> {
    let input::Module {
        compiled_module: module,
        functions: optimized_fns,
    } = module;

    dbg_println!(flag: function_list_sizes, "processing {} functions", functions.len());

    let mut functions = functions;

    let mut module_context = FunctionContext {
        package_context,
        module,
        definitions,
    };

    let mut optimized_fns = optimized_fns.clone();

    for fun in functions.iter_mut() {
        let Some(opt_fun) = optimized_fns.remove(&fun.index) else {
            return Err(partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "failed to find function {}::{} in optimized function list",
                package_context.version_id,
                fun.name.to_short_string(package_context.interner)
            ));
        };
        let input::Function {
            ndx: _,
            code: opt_code,
        } = opt_fun;
        if let Some(opt_code) = opt_code {
            let (code, jump_tables) =
                code(&mut module_context, opt_code.jump_tables, opt_code.code)?;
            fun.code = code;
            fun.jump_tables = jump_tables;
        }
    }

    let FunctionContext { .. } = module_context;

    Ok(functions)
}

fn alloc_function(
    context: &PackageContext,
    module_name: &IdentifierKey,
    module: &CompiledModule,
    index: FunctionDefinitionIndex,
    def: &FunctionDefinition,
) -> PartialVMResult<Function> {
    let handle = module.function_handle_at(def.function);
    let name_ident_str = module.identifier_at(handle.name);
    let module_id = module.self_id();
    let is_entry = def.is_entry;
    let (native, def_is_native) = if def.is_native() {
        (
            context.natives.resolve(
                module_id.address(),
                module_id.name().as_str(),
                name_ident_str.as_str(),
            ),
            true,
        )
    } else {
        (None, false)
    };
    let name = {
        let package_key = context.original_id;
        let module_name = *module_name;
        let member_name = context.interner.intern_ident_str(name_ident_str);
        VirtualTableKey::from_parts(package_key, module_name, member_name)
    };
    let parameters = module
        .signature_at(handle.parameters)
        .0
        .iter()
        .map(|tok| make_arena_type(context, module, tok))
        .collect::<PartialVMResult<Vec<_>>>()?;
    let parameters = context.arena_vec(parameters.into_iter())?;
    // Native functions do not have a code unit
    let (locals_len, locals) = match &def.code {
        Some(code) => {
            let locals_len = parameters
                .len()
                .safe_add(module.signature_at(code.locals).0.len())?;
            let locals = context.arena_vec(
                module
                    .signature_at(code.locals)
                    .0
                    .iter()
                    .map(|tok| make_arena_type(context, module, tok))
                    .collect::<PartialVMResult<Vec<_>>>()?
                    .into_iter(),
            )?;
            (locals_len, locals)
        }
        None => (0, ArenaVec::empty()),
    };
    let return_ = module
        .signature_at(handle.return_)
        .0
        .iter()
        .map(|tok| make_arena_type(context, module, tok))
        .collect::<PartialVMResult<Vec<_>>>()?;
    let return_ = context.arena_vec(return_.into_iter())?;
    let type_parameters = context.arena_vec(handle.type_parameters.clone().into_iter())?;
    let fun = Function {
        file_format_version: module.version(),
        index,
        is_entry,
        visibility: def.visibility,
        parameters,
        locals,
        return_,
        type_parameters,
        native,
        def_is_native,
        name,
        locals_len,
        // replaced in the next step of compilation
        code: ArenaVec::empty(),
        jump_tables: ArenaVec::empty(),
    };
    Ok(fun)
}

// [ALLOC] Bytecode result is allocated in the arena
fn code(
    context: &mut FunctionContext,
    jump_tables: Vec<FF::VariantJumpTable>,
    blocks: BTreeMap<u16, Vec<input::Bytecode>>,
) -> PartialVMResult<(ArenaVec<Bytecode>, ArenaVec<VariantJumpTable>)> {
    // Compute the initial jump tables
    let jump_tables = jump_tables.iter().map(jump_table).collect::<Vec<_>>();

    // Flatten and renumber any changed jump tables and bytecode blocks
    let (fn_bytecode, jump_tables) =
        flatten_and_renumber_input_bytcode_and_jumptables(blocks, jump_tables)?;

    // Grab an arena for the jump tables
    let arena_jump_tables = jump_tables
        .into_iter()
        .map(|jt| context.package_context.arena_vec(jt.into_iter()))
        .collect::<PartialVMResult<Vec<_>>>()?;
    let final_jump_tables = context
        .package_context
        .arena_vec(arena_jump_tables.into_iter())?;

    // Generate the final bytecode with jump table pointers
    let jump_table_ptrs = final_jump_tables.to_ptrs();
    let final_bytecode = context.package_context.package_arena.alloc_vec(
        fn_bytecode
            .into_iter()
            .map(|bc| bytecode(context, &jump_table_ptrs, bc))
            .collect::<PartialVMResult<Vec<Bytecode>>>()?
            .into_iter(),
    )?;

    // Return the final bytecode and jump tables
    Ok((final_bytecode, final_jump_tables))
}

fn jump_table(table: &FF::VariantJumpTable) -> Vec<FF::CodeOffset> {
    match &table.jump_table {
        FF::JumpTableInner::Full(items) => items.clone(),
    }
}

pub(crate) fn flatten_and_renumber_input_bytcode_and_jumptables(
    blocks: BTreeMap<u16, Vec<input::Bytecode>>,
    jump_tables: Vec<Vec<FF::CodeOffset>>,
) -> PartialVMResult<(Vec<input::Bytecode>, Vec<Vec<FF::CodeOffset>>)> {
    dbg_println!("Input: {:#?}", blocks);
    let mut offset_map = BTreeMap::new(); // Map line name (u16) -> new bytecode offset
    let mut concatenated_bytecode = Vec::new();

    // Calculate new offsets and build concatenated bytecode
    let mut current_offset: u16 = 0;
    for (line_name, bytecodes) in &blocks {
        offset_map.insert(*line_name, current_offset);

        // Check for overflow when adding bytecode length
        let bytecode_len_u16 = u16::try_from(bytecodes.len()).map_err(|_| {
            partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "Bytecode block size {} exceeds u16::MAX",
                bytecodes.len(),
            )
        })?;

        current_offset = current_offset
            .checked_add(bytecode_len_u16)
            .ok_or_else(|| {
                partial_vm_error!(
                    UNKNOWN_INVARIANT_VIOLATION_ERROR,
                    "Bytecode offset overflow: {} + {} exceeds u16::MAX",
                    current_offset,
                    bytecode_len_u16
                )
            })?;

        concatenated_bytecode.extend_from_slice(bytecodes);
    }
    dbg_println!("Concatenated: {:#?}", concatenated_bytecode);

    // Rewrite jump tables with new offsets
    let jump_tables = compute_renumbered_jump_tables(jump_tables, &offset_map)?;

    // Rewrite branch instructions with new offsets
    let byte_code = compute_renumbered_bytecode(&offset_map, concatenated_bytecode)?;

    Ok((byte_code, jump_tables))
}

fn compute_renumbered_jump_tables(
    jump_tables: Vec<Vec<FF::CodeOffset>>,
    offset_map: &BTreeMap<FF::CodeOffset, FF::CodeOffset>,
) -> PartialVMResult<Vec<Vec<FF::CodeOffset>>> {
    jump_tables
        .into_iter()
        .map(|table| {
            table
                .into_iter()
                .map(|target| match offset_map.get(&target) {
                    Some(&new_offset) => Ok(new_offset),
                    None => Err(partial_vm_error!(
                        UNKNOWN_INVARIANT_VIOLATION_ERROR,
                        "Invalid jump table offset {}",
                        target
                    )),
                })
                .collect::<PartialVMResult<Vec<_>>>()
        })
        .collect::<PartialVMResult<Vec<_>>>()
}

fn compute_renumbered_bytecode(
    offset_map: &BTreeMap<FF::CodeOffset, FF::CodeOffset>,
    bytecode: Vec<input::Bytecode>,
) -> PartialVMResult<Vec<input::Bytecode>> {
    fn find_new_offset(
        offset_map: &BTreeMap<FF::CodeOffset, FF::CodeOffset>,
        target: FF::CodeOffset,
    ) -> PartialVMResult<FF::CodeOffset> {
        match offset_map.get(&target) {
            Some(&new_offset) => Ok(new_offset),
            None => Err(partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "Invalid branch target {}",
                target,
            )),
        }
    }

    fn update_bytecode_offset(
        offset_map: &BTreeMap<FF::CodeOffset, FF::CodeOffset>,
        instr: input::Bytecode,
    ) -> PartialVMResult<input::Bytecode> {
        match instr {
            input::Bytecode::BrFalse(target) => {
                let new_target = find_new_offset(offset_map, target)?;
                Ok(input::Bytecode::BrFalse(new_target))
            }
            input::Bytecode::BrTrue(target) => {
                let new_target = find_new_offset(offset_map, target)?;
                Ok(input::Bytecode::BrTrue(new_target))
            }
            input::Bytecode::Branch(target) => {
                let new_target = find_new_offset(offset_map, target)?;
                Ok(input::Bytecode::Branch(new_target))
            }
            instr @ (input::Bytecode::Pop
            | input::Bytecode::Ret
            | input::Bytecode::LdU8(_)
            | input::Bytecode::LdU64(_)
            | input::Bytecode::LdU128(_)
            | input::Bytecode::CastU8
            | input::Bytecode::CastU64
            | input::Bytecode::CastU128
            | input::Bytecode::LdConst(..)
            | input::Bytecode::LdTrue
            | input::Bytecode::LdFalse
            | input::Bytecode::CopyLoc(_)
            | input::Bytecode::MoveLoc(_)
            | input::Bytecode::StLoc(_)
            | input::Bytecode::Call(..)
            | input::Bytecode::CallGeneric(..)
            | input::Bytecode::Pack(..)
            | input::Bytecode::PackGeneric(..)
            | input::Bytecode::Unpack(..)
            | input::Bytecode::UnpackGeneric(..)
            | input::Bytecode::ReadRef
            | input::Bytecode::WriteRef
            | input::Bytecode::FreezeRef
            | input::Bytecode::MutBorrowLoc(_)
            | input::Bytecode::ImmBorrowLoc(_)
            | input::Bytecode::MutBorrowField(..)
            | input::Bytecode::MutBorrowFieldGeneric(..)
            | input::Bytecode::ImmBorrowField(..)
            | input::Bytecode::ImmBorrowFieldGeneric(..)
            | input::Bytecode::Add
            | input::Bytecode::Sub
            | input::Bytecode::Mul
            | input::Bytecode::Mod
            | input::Bytecode::Div
            | input::Bytecode::BitOr
            | input::Bytecode::BitAnd
            | input::Bytecode::Xor
            | input::Bytecode::Or
            | input::Bytecode::And
            | input::Bytecode::Not
            | input::Bytecode::Eq
            | input::Bytecode::Neq
            | input::Bytecode::Lt
            | input::Bytecode::Gt
            | input::Bytecode::Le
            | input::Bytecode::Ge
            | input::Bytecode::Abort
            | input::Bytecode::Nop
            | input::Bytecode::Shl
            | input::Bytecode::Shr
            | input::Bytecode::VecPack(..)
            | input::Bytecode::VecLen(..)
            | input::Bytecode::VecImmBorrow(..)
            | input::Bytecode::VecMutBorrow(..)
            | input::Bytecode::VecPushBack(..)
            | input::Bytecode::VecPopBack(..)
            | input::Bytecode::VecUnpack(..)
            | input::Bytecode::VecSwap(..)
            | input::Bytecode::LdU16(_)
            | input::Bytecode::LdU32(_)
            | input::Bytecode::LdU256(..)
            | input::Bytecode::CastU16
            | input::Bytecode::CastU32
            | input::Bytecode::CastU256
            | input::Bytecode::PackVariant(..)
            | input::Bytecode::PackVariantGeneric(..)
            | input::Bytecode::UnpackVariant(..)
            | input::Bytecode::UnpackVariantImmRef(..)
            | input::Bytecode::UnpackVariantMutRef(..)
            | input::Bytecode::UnpackVariantGeneric(..)
            | input::Bytecode::UnpackVariantGenericImmRef(..)
            | input::Bytecode::UnpackVariantGenericMutRef(..)
            | input::Bytecode::VariantSwitch(..)) => Ok(instr),
        }
    }

    bytecode
        .into_iter()
        .map(|instr| update_bytecode_offset(offset_map, instr))
        .collect::<PartialVMResult<Vec<_>>>()
}

fn bytecode(
    context: &mut FunctionContext,
    jump_tables: &[VMPointer<VariantJumpTable>],
    bytecode: input::Bytecode,
) -> PartialVMResult<Bytecode> {
    let bytecode = match bytecode {
        // Calls -- these get compiled to something more-direct here
        input::Bytecode::Call(ndx) => {
            let call_type = call(context.package_context, context.module, ndx)?;
            match call_type {
                CallType::Direct(func) => Bytecode::DirectCall(func),
                CallType::Virtual(vtable) => Bytecode::VirtualCall(vtable),
            }
        }

        // For now, generic calls retain an index so we can look up their signature as well.
        input::Bytecode::CallGeneric(ndx) => {
            let generic_ptr = &context
                .definitions
                .function_instantiations
                .safe_get(ndx.0 as usize)?;
            Bytecode::CallGeneric(generic_ptr.ptr_clone())
        }

        // Standard Codes
        input::Bytecode::Pop => Bytecode::Pop,
        input::Bytecode::Ret => Bytecode::Ret,
        input::Bytecode::BrTrue(n) => Bytecode::BrTrue(n),
        input::Bytecode::BrFalse(n) => Bytecode::BrFalse(n),
        input::Bytecode::Branch(n) => Bytecode::Branch(n),

        input::Bytecode::LdU256(n) => Bytecode::LdU256(context.package_context.arena_box(*n)?),
        input::Bytecode::LdU128(n) => Bytecode::LdU128(context.package_context.arena_box(*n)?),
        input::Bytecode::LdU16(n) => Bytecode::LdU16(n),
        input::Bytecode::LdU32(n) => Bytecode::LdU32(n),
        input::Bytecode::LdU64(n) => Bytecode::LdU64(n),
        input::Bytecode::LdU8(n) => Bytecode::LdU8(n),

        input::Bytecode::LdConst(ndx) => {
            let const_ptr = &context.definitions.constants.safe_get(ndx.0 as usize)?;
            Bytecode::LdConst(const_ptr.ptr_clone())
        }
        input::Bytecode::LdTrue => Bytecode::LdTrue,
        input::Bytecode::LdFalse => Bytecode::LdFalse,

        input::Bytecode::CopyLoc(ndx) => Bytecode::CopyLoc(ndx),
        input::Bytecode::MoveLoc(ndx) => Bytecode::MoveLoc(ndx),
        input::Bytecode::StLoc(ndx) => Bytecode::StLoc(ndx),
        input::Bytecode::ReadRef => Bytecode::ReadRef,
        input::Bytecode::WriteRef => Bytecode::WriteRef,
        input::Bytecode::FreezeRef => Bytecode::FreezeRef,
        input::Bytecode::MutBorrowLoc(ndx) => Bytecode::MutBorrowLoc(ndx),
        input::Bytecode::ImmBorrowLoc(ndx) => Bytecode::ImmBorrowLoc(ndx),

        // Structs and Fields
        input::Bytecode::Pack(ndx) => {
            let struct_ptr = &context.definitions.structs.safe_get(ndx.0 as usize)?;
            Bytecode::Pack(struct_ptr.ptr_clone())
        }
        input::Bytecode::Unpack(ndx) => {
            let struct_ptr = &context.definitions.structs.safe_get(ndx.0 as usize)?;
            Bytecode::Unpack(struct_ptr.ptr_clone())
        }

        input::Bytecode::PackGeneric(ndx) => {
            let struct_inst_ptr = &context
                .definitions
                .struct_instantiations
                .safe_get(ndx.0 as usize)?;
            Bytecode::PackGeneric(struct_inst_ptr.ptr_clone())
        }
        input::Bytecode::UnpackGeneric(ndx) => {
            let struct_inst_ptr = &context
                .definitions
                .struct_instantiations
                .safe_get(ndx.0 as usize)?;
            Bytecode::UnpackGeneric(struct_inst_ptr.ptr_clone())
        }

        input::Bytecode::MutBorrowField(ndx) => {
            let field_ptr = &context.definitions.field_handles.safe_get(ndx.0 as usize)?;
            Bytecode::MutBorrowField(field_ptr.ptr_clone())
        }

        input::Bytecode::ImmBorrowField(ndx) => {
            let field_ptr = &context.definitions.field_handles.safe_get(ndx.0 as usize)?;
            Bytecode::ImmBorrowField(field_ptr.ptr_clone())
        }

        input::Bytecode::MutBorrowFieldGeneric(ndx) => {
            let field_inst_ptr = &context
                .definitions
                .field_instantiations
                .safe_get(ndx.0 as usize)?;
            Bytecode::MutBorrowFieldGeneric(field_inst_ptr.ptr_clone())
        }
        input::Bytecode::ImmBorrowFieldGeneric(ndx) => {
            let field_inst_ptr = &context
                .definitions
                .field_instantiations
                .safe_get(ndx.0 as usize)?;
            Bytecode::ImmBorrowFieldGeneric(field_inst_ptr.ptr_clone())
        }

        // Math Operations
        input::Bytecode::Add => Bytecode::Add,
        input::Bytecode::Sub => Bytecode::Sub,
        input::Bytecode::Mul => Bytecode::Mul,
        input::Bytecode::Mod => Bytecode::Mod,
        input::Bytecode::Div => Bytecode::Div,
        input::Bytecode::BitOr => Bytecode::BitOr,
        input::Bytecode::BitAnd => Bytecode::BitAnd,
        input::Bytecode::Xor => Bytecode::Xor,
        input::Bytecode::Or => Bytecode::Or,
        input::Bytecode::And => Bytecode::And,
        input::Bytecode::Not => Bytecode::Not,
        input::Bytecode::Eq => Bytecode::Eq,
        input::Bytecode::Neq => Bytecode::Neq,
        input::Bytecode::Lt => Bytecode::Lt,
        input::Bytecode::Gt => Bytecode::Gt,
        input::Bytecode::Le => Bytecode::Le,
        input::Bytecode::Ge => Bytecode::Ge,
        input::Bytecode::Abort => Bytecode::Abort,
        input::Bytecode::Nop => Bytecode::Nop,
        input::Bytecode::Shl => Bytecode::Shl,
        input::Bytecode::Shr => Bytecode::Shr,

        input::Bytecode::CastU256 => Bytecode::CastU256,
        input::Bytecode::CastU128 => Bytecode::CastU128,
        input::Bytecode::CastU16 => Bytecode::CastU16,
        input::Bytecode::CastU32 => Bytecode::CastU32,
        input::Bytecode::CastU64 => Bytecode::CastU64,
        input::Bytecode::CastU8 => Bytecode::CastU8,

        // Vectors
        input::Bytecode::VecPack(si, size) => {
            let ty_ptr = context.get_vec_type(&si)?;
            Bytecode::VecPack(ty_ptr, size)
        }
        input::Bytecode::VecLen(si) => {
            let ty_ptr = context.get_vec_type(&si)?;
            Bytecode::VecLen(ty_ptr)
        }
        input::Bytecode::VecImmBorrow(si) => {
            let ty_ptr = context.get_vec_type(&si)?;
            Bytecode::VecImmBorrow(ty_ptr)
        }
        input::Bytecode::VecMutBorrow(si) => {
            let ty_ptr = context.get_vec_type(&si)?;
            Bytecode::VecMutBorrow(ty_ptr)
        }
        input::Bytecode::VecPushBack(si) => {
            let ty_ptr = context.get_vec_type(&si)?;
            Bytecode::VecPushBack(ty_ptr)
        }
        input::Bytecode::VecPopBack(si) => {
            let ty_ptr = context.get_vec_type(&si)?;
            Bytecode::VecPopBack(ty_ptr)
        }
        input::Bytecode::VecUnpack(si, size) => {
            let ty_ptr = context.get_vec_type(&si)?;
            Bytecode::VecUnpack(ty_ptr, size)
        }
        input::Bytecode::VecSwap(si) => {
            let ty_ptr = context.get_vec_type(&si)?;
            Bytecode::VecSwap(ty_ptr)
        }

        // Enums and Variants
        input::Bytecode::PackVariant(ndx) => {
            let variant_ptr = &context.definitions.variants.safe_get(ndx.0 as usize)?;
            Bytecode::PackVariant(variant_ptr.ptr_clone())
        }
        input::Bytecode::UnpackVariant(ndx) => {
            let variant_ptr = &context.definitions.variants.safe_get(ndx.0 as usize)?;
            Bytecode::UnpackVariant(variant_ptr.ptr_clone())
        }

        input::Bytecode::PackVariantGeneric(ndx) => {
            let variant_inst_ptr = &context
                .definitions
                .variant_instantiations
                .safe_get(ndx.0 as usize)?;
            Bytecode::PackVariantGeneric(variant_inst_ptr.ptr_clone())
        }
        input::Bytecode::UnpackVariantGeneric(ndx) => {
            let variant_inst_ptr = &context
                .definitions
                .variant_instantiations
                .safe_get(ndx.0 as usize)?;
            Bytecode::UnpackVariantGeneric(variant_inst_ptr.ptr_clone())
        }

        input::Bytecode::UnpackVariantImmRef(ndx) => {
            let variant_ptr = &context.definitions.variants.safe_get(ndx.0 as usize)?;
            Bytecode::UnpackVariantImmRef(variant_ptr.ptr_clone())
        }
        input::Bytecode::UnpackVariantMutRef(ndx) => {
            let variant_ptr = &context.definitions.variants.safe_get(ndx.0 as usize)?;
            Bytecode::UnpackVariantMutRef(variant_ptr.ptr_clone())
        }

        input::Bytecode::UnpackVariantGenericImmRef(ndx) => {
            let variant_inst_ptr = &context
                .definitions
                .variant_instantiations
                .safe_get(ndx.0 as usize)?;
            Bytecode::UnpackVariantGenericImmRef(variant_inst_ptr.ptr_clone())
        }
        input::Bytecode::UnpackVariantGenericMutRef(ndx) => {
            let variant_inst_ptr = &context
                .definitions
                .variant_instantiations
                .safe_get(ndx.0 as usize)?;
            Bytecode::UnpackVariantGenericMutRef(variant_inst_ptr.ptr_clone())
        }
        input::Bytecode::VariantSwitch(ndx) => {
            let jump_table = &jump_tables.safe_get(ndx.0 as usize)?;
            Bytecode::VariantSwitch(jump_table.ptr_clone())
        }
    };
    Ok(bytecode)
}

fn call(
    context: &PackageContext,
    module: &CompiledModule,
    function_handle_index: FunctionHandleIndex,
) -> PartialVMResult<CallType> {
    let func_handle = module.function_handle_at(function_handle_index);
    let member_name = context
        .interner
        .intern_ident_str(module.identifier_at(func_handle.name));
    let module_handle = module.module_handle_at(func_handle.module);
    let original_id = module.module_id_for_handle(module_handle);
    let module_name = context.interner.intern_ident_str(original_id.name());
    let vtable_key = VirtualTableKey::from_parts(*original_id.address(), module_name, member_name);

    dbg_println!(flag: function_resolution, "Resolving function: {:?}", vtable_key);
    Ok(
        match context.try_resolve_direct_function_call(&vtable_key)? {
            Some(func) => CallType::Direct(func),
            None => CallType::Virtual(vtable_key),
        },
    )
}

// -------------------------------------------------------------------------------------------------
// Type Translation
// -------------------------------------------------------------------------------------------------

/// Convert a signature token type into its execution counterpart, including converting datatypes
/// into their VTable entry keys.
// [ALLOC] Resultant type is allocated in the arena
fn make_arena_type(
    context: &PackageContext,
    module: &CompiledModule,
    tok: &SignatureToken,
) -> PartialVMResult<ArenaType> {
    let res = match tok {
        SignatureToken::Bool => ArenaType::Bool,
        SignatureToken::U8 => ArenaType::U8,
        SignatureToken::U16 => ArenaType::U16,
        SignatureToken::U32 => ArenaType::U32,
        SignatureToken::U64 => ArenaType::U64,
        SignatureToken::U128 => ArenaType::U128,
        SignatureToken::U256 => ArenaType::U256,
        SignatureToken::Address => ArenaType::Address,
        SignatureToken::Signer => ArenaType::Signer,
        SignatureToken::TypeParameter(idx) => ArenaType::TyParam(*idx),
        SignatureToken::Vector(inner_tok) => {
            ArenaType::Vector(context.arena_box(make_arena_type(context, module, inner_tok)?)?)
        }
        SignatureToken::Reference(inner_tok) => {
            ArenaType::Reference(context.arena_box(make_arena_type(context, module, inner_tok)?)?)
        }
        SignatureToken::MutableReference(inner_tok) => ArenaType::MutableReference(
            context.arena_box(make_arena_type(context, module, inner_tok)?)?,
        ),
        SignatureToken::Datatype(sh_idx) => {
            let datatype_handle = module.datatype_handle_at(*sh_idx);
            let datatype_name = context
                .interner
                .intern_ident_str(module.identifier_at(datatype_handle.name));
            let module_handle = module.module_handle_at(datatype_handle.module);
            let original_address = module.address_identifier_at(module_handle.address);
            let module_name = context
                .interner
                .intern_ident_str(module.identifier_at(module_handle.name));
            let cache_idx =
                VirtualTableKey::from_parts(*original_address, module_name, datatype_name);
            ArenaType::Datatype(cache_idx)
        }
        SignatureToken::DatatypeInstantiation(inst) => {
            let (sh_idx, tys) = &**inst;
            let type_parameters: Vec<_> = tys
                .iter()
                .map(|tok| make_arena_type(context, module, tok))
                .collect::<PartialVMResult<_>>()?;
            let type_parameters = context.arena_vec(type_parameters.into_iter())?;
            let datatype_handle = module.datatype_handle_at(*sh_idx);
            let datatype_name = context
                .interner
                .intern_ident_str(module.identifier_at(datatype_handle.name));
            let module_handle = module.module_handle_at(datatype_handle.module);
            let original_address = module.address_identifier_at(module_handle.address);
            let module_name = context
                .interner
                .intern_ident_str(module.identifier_at(module_handle.name));
            let cache_idx =
                VirtualTableKey::from_parts(*original_address, module_name, datatype_name);
            ArenaType::DatatypeInstantiation(context.arena_box((cache_idx, type_parameters))?)
        }
    };
    Ok(res)
}
