// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::{
        arena::{Arena, ArenaBox, ArenaVec},
        identifier_interner::IdentifierInterner,
    },
    dbg_println,
    execution::{
        dispatch_tables::{IntraPackageKey, PackageVirtualTable, VirtualTableKey},
        values::Value,
    },
    jit::{execution::ast::*, optimization::ast as input},
    natives::functions::NativeFunctions,
    shared::{
        linkage_context::LinkageContext,
        types::{PackageStorageId, RuntimePackageId},
        unique_map,
        vm_pointer::VMPointer,
    },
    string_interner,
};
use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        self as FF, CompiledModule, FunctionDefinition, FunctionDefinitionIndex,
        FunctionHandleIndex, SignatureIndex, SignatureToken, StructFieldInformation, TableIndex,
    },
};
use move_core_types::{identifier::Identifier, language_storage::ModuleId, vm_status::StatusCode};
use std::collections::{BTreeMap, BTreeSet, HashMap};

// -------------------------------------------------------------------------------------------------
// Translation Context and Definitions
// -------------------------------------------------------------------------------------------------

struct PackageContext<'natives> {
    pub natives: &'natives NativeFunctions,
    pub type_origin_table: HashMap<IntraPackageKey, PackageStorageId>,

    pub storage_id: PackageStorageId,
    pub runtime_id: RuntimePackageId,
    // NB: this is under the package's context so we don't need to further resolve by
    // address in this table.
    pub loaded_modules: BTreeMap<Identifier, Module>,

    // NB: All things except for types are allocated into this arena.
    pub package_arena: Arena,
    pub vtable: PackageVirtualTable,
}

struct FunctionContext<'pkg_ctxt, 'natives> {
    package_context: &'pkg_ctxt PackageContext<'natives>,
    module: &'pkg_ctxt CompiledModule,
    definitions: Definitions,
}

#[allow(dead_code)]
struct Definitions {
    structs: Vec<VMPointer<StructDef>>,
    struct_instantiations: Vec<VMPointer<StructInstantiation>>,
    enums: Vec<VMPointer<EnumDef>>,
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
        module_name: Identifier,
        vtable: impl IntoIterator<Item = (Identifier, VMPointer<Function>)>,
    ) -> PartialVMResult<()> {
        let string_interner = string_interner();
        let module_name = string_interner.get_or_intern_identifier(&module_name)?;
        for (name, func) in vtable {
            let member_name = string_interner.get_or_intern_identifier(&name)?;
            let key = IntraPackageKey {
                module_name,
                member_name,
            };
            if self.vtable.functions.insert(key, func).is_some() {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!(
                            "Duplicate key {}::{}",
                            self.storage_id,
                            key.to_string()?
                        )),
                );
            }
        }
        Ok(())
    }

    fn insert_vtable_datatypes(
        &mut self,
        datatype_descriptors: Vec<VMPointer<DatatypeDescriptor>>,
    ) -> PartialVMResult<()> {
        for ptr in datatype_descriptors.into_iter() {
            self.vtable.defining_ids.insert(*ptr.defining_id.address());
            let name = ptr.intra_package_name();
            if self.vtable.types.insert(name, ptr).is_some() {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!(
                            "Duplicate key {}::{}",
                            self.storage_id,
                            name.to_string()?
                        )),
                );
            }
        }
        Ok(())
    }

    fn try_resolve_function(&self, vtable_entry: &VirtualTableKey) -> Option<VMPointer<Function>> {
        self.vtable
            .functions
            .get(&vtable_entry.inner_pkg_key)
            .map(|f| f.ptr_clone())
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
            return Err(
                PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION).with_message(
                    "could not find the signature for a vector-related bytecode \
                        in the signature table"
                        .to_owned(),
                ),
            );
        };
        if !tys.to_ref().len() == 1 {
            return Err(
                PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION).with_message(
                    "the type argument for vector-related bytecode \
                        expects one and only one signature token"
                        .to_owned(),
                ),
            );
        };
        let ty = VMPointer::from_ref(&tys.to_ref()[0]);
        Ok(ty)
    }
}

// -------------------------------------------------------------------------------------------------
// Package Translation
// -------------------------------------------------------------------------------------------------

pub fn package(
    natives: &NativeFunctions,
    link_context: &LinkageContext,
    verified_package: input::Package,
) -> PartialVMResult<Package> {
    let storage_id = verified_package.storage_id;
    let runtime_id = verified_package.runtime_id;
    let (module_ids_in_pkg, mut package_modules): (BTreeSet<_>, Vec<_>) =
        verified_package.modules.into_iter().unzip();

    let type_origin_table = verified_package
        .type_origin_table
        .into_iter()
        .map(|type_origin| {
            Ok((
                IntraPackageKey {
                    module_name: string_interner()
                        .get_or_intern_identifier(&type_origin.module_name)?,
                    member_name: string_interner()
                        .get_or_intern_identifier(&type_origin.type_name)?,
                },
                type_origin.origin_id,
            ))
        })
        .collect::<PartialVMResult<_>>()?;

    let mut package_context = PackageContext {
        natives,
        storage_id,
        runtime_id,
        loaded_modules: BTreeMap::new(),
        package_arena: Arena::new(),
        vtable: PackageVirtualTable::new(),
        type_origin_table,
    };

    // Load modules in dependency order within the package. Needed for both static call
    // resolution and type caching.
    while let Some(mut input_module) = package_modules.pop() {
        let mut immediate_dependencies = input_module
            .compiled_module
            .immediate_dependencies()
            .into_iter()
            .filter(|dep| {
                module_ids_in_pkg.contains(dep) && dep != &input_module.compiled_module.self_id()
            });

        // If we haven't processed the immediate dependencies yet, push the module back onto
        // the front and process other modules first.
        if !immediate_dependencies.all(|dep| {
            package_context
                .loaded_modules
                .contains_key(&dep.name().to_owned())
        }) {
            package_modules.insert(0, input_module);
            continue;
        }

        let loaded_module = module(
            &mut package_context,
            link_context,
            storage_id,
            &mut input_module,
        )?;

        assert!(package_context
            .loaded_modules
            .insert(loaded_module.id.name().to_owned(), loaded_module)
            .is_none());
    }

    let PackageContext {
        storage_id,
        natives: _,
        runtime_id,
        loaded_modules,
        package_arena,
        vtable,
        type_origin_table: _,
    } = package_context;

    Ok(Package {
        storage_id,
        runtime_id,
        loaded_modules,
        package_arena,
        vtable,
    })
}

// -------------------------------------------------------------------------------------------------
// Module Translation

fn module(
    context: &mut PackageContext<'_>,
    _link_context: &LinkageContext,
    package_id: PackageStorageId,
    module: &mut input::Module,
) -> PartialVMResult<Module> {
    let self_id = module.compiled_module.self_id();
    dbg_println!("Loading module: {}", self_id);

    let cmodule = &module.compiled_module;

    // Initialize module data
    let type_refs = initialize_type_refs(context, cmodule)?;

    let (structs, enums, datatype_descriptors) = datatypes(context, &package_id, cmodule)?;
    let instantiation_signatures = cache_signatures(context, cmodule)?;
    dbg_println!("Module types loaded");

    let sig_pointers = instantiation_signatures
        .iter()
        .map(VMPointer::from_ref)
        .collect::<Vec<_>>();

    context.insert_vtable_datatypes(datatype_descriptors.to_ptrs())?;

    let struct_instantiations = struct_instantiations(context, cmodule, &structs, &sig_pointers)?;
    let enum_instantiations = enum_instantiations(context, cmodule, &enums, &sig_pointers)?;

    // Process function instantiations
    let function_instantiations = function_instantiations(context, cmodule, &sig_pointers)?;

    // Process field handles and instantiations
    let field_handles = field_handles(context, cmodule, &structs)?;
    let field_instantiations = field_instantiations(context, cmodule, &field_handles)?;

    let constants = constants(context, cmodule)?;

    let variant_handles = variant_handles(cmodule, &enums);
    let variant_instantiations = variant_instantiations(context, cmodule, &enum_instantiations)?;

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

    // Function loading is effectful; they all go into the arena. This happens last because it
    // relies on the definitions above to rewrite the bytecode appropriately.
    let functions = functions(context, module, definitions)?;

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
            let struct_name = string_interner()
                .get_or_intern_ident_str(module.identifier_at(datatype_handle.name))?;
            let module_handle = module.module_handle_at(datatype_handle.module);
            let runtime_id = module.module_id_for_handle(module_handle);
            let module_name = string_interner().get_or_intern_ident_str(runtime_id.name())?;
            Ok(IntraPackageKey {
                module_name,
                member_name: struct_name.to_owned(),
            })
        })
        .collect::<PartialVMResult<Vec<_>>>()?;
    context.arena_vec(type_refs.into_iter())
}

// -------------------------------------------------------------------------------------------------
// Datatype Translation
// -------------------------------------------------------------------------------------------------

/// Loads strucks and enums, returning them and their datatype descriptors (for vtable entry).
fn datatypes(
    context: &mut PackageContext,
    storage_id: &PackageStorageId,
    module: &CompiledModule,
) -> PartialVMResult<(
    ArenaVec<StructDef>,
    ArenaVec<EnumDef>,
    ArenaVec<DatatypeDescriptor>,
)> {
    fn resolve_member_name(
        ident_interner: &IdentifierInterner,
        name: &VirtualTableKey,
    ) -> PartialVMResult<Identifier> {
        ident_interner.resolve_ident(&name.inner_pkg_key.member_name, "datatype name")
    }

    // NB: It is the responsibility of the adapter to determine the correct type origin table,
    // and pass a full and complete representation of it in with the package.
    fn defining_id(
        context: &PackageContext,
        ident_interner: &IdentifierInterner,
        storage_id: &PackageStorageId,
        name: &VirtualTableKey,
    ) -> PartialVMResult<ModuleId> {
        let defining_address = context
            .type_origin_table
            .get(&name.inner_pkg_key)
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::LOOKUP_FAILED).with_message(format!(
                    "Type origin not found for type {}",
                    name.to_string().expect("No name")
                ))
            })?;
        dbg_println!("Package ID: {:?}", storage_id);
        dbg_println!("Defining Address: {:?}", defining_address);
        let module_id =
            ident_interner.resolve_ident(&name.inner_pkg_key.module_name, "module name")?;
        Ok(ModuleId::new(*defining_address, module_id))
    }

    let runtime_id = context.runtime_id;

    let structs: ArenaVec<StructDef> = structs(context, &runtime_id, module)?;
    let enums: ArenaVec<EnumDef> = enums(context, &runtime_id, module)?;

    let runtime_id = module.self_id();
    let interner = string_interner();

    let struct_descriptors = structs
        .iter()
        .map(|struct_| {
            let name = resolve_member_name(&interner, &struct_.def_vtable_key)?;
            let defining_id = defining_id(context, &interner, storage_id, &struct_.def_vtable_key)?;
            let runtime_id = runtime_id.clone();
            let datatype_info =
                context.arena_box(Datatype::Struct(VMPointer::from_ref(struct_)))?;
            let descriptor = DatatypeDescriptor::new(name, defining_id, runtime_id, datatype_info);
            Ok(descriptor)
        })
        .collect::<PartialVMResult<Vec<_>>>()?;

    let enum_descriptors = enums
        .iter()
        .map(|enum_| {
            let name = resolve_member_name(&interner, &enum_.def_vtable_key)?;
            let defining_id = defining_id(context, &interner, storage_id, &enum_.def_vtable_key)?;
            let runtime_id = runtime_id.clone();
            let datatype_info = context.arena_box(Datatype::Enum(VMPointer::from_ref(enum_)))?;
            let descriptor = DatatypeDescriptor::new(name, defining_id, runtime_id, datatype_info);
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
    original_id: &RuntimePackageId,
    module: &CompiledModule,
) -> PartialVMResult<ArenaVec<StructDef>> {
    let ident_interner = string_interner();
    let module_name = ident_interner.get_or_intern_ident_str(module.self_id().name())?;

    let struct_defs = module
        .struct_defs()
        .iter()
        .map(|struct_def| {
            let struct_handle = module.datatype_handle_at(struct_def.struct_handle);

            let name = module.identifier_at(struct_handle.name);
            let member_name = ident_interner.get_or_intern_ident_str(name)?;
            let def_vtable_key =
                VirtualTableKey::from_parts(*original_id, module_name, member_name);

            let abilities = struct_handle.abilities;

            let struct_module_handle = module.module_handle_at(struct_handle.module);
            dbg_println!("Indexing type {:?} at {:?}", name, struct_module_handle);

            let StructFieldInformation::Declared(fields) = &struct_def.field_information else {
                unreachable!("native structs have been removed");
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
    original_id: &RuntimePackageId,
    module: &CompiledModule,
) -> PartialVMResult<ArenaVec<EnumDef>> {
    // We do this in two passes:
    // 1. We make each outer EnumDef and place it in an ArenaVec so its location is fixed.
    // 2. We generate the variants with backpointers to the enum def.

    let ident_interner = string_interner();
    let module_name = ident_interner.get_or_intern_ident_str(module.self_id().name())?;

    let enum_defs = module
        .enum_defs()
        .iter()
        .map(|enum_def| {
            let enum_handle = module.datatype_handle_at(enum_def.enum_handle);

            let name = module.identifier_at(enum_handle.name);
            let member_name = ident_interner.get_or_intern_ident_str(name)?;
            let def_vtable_key =
                VirtualTableKey::from_parts(*original_id, module_name, member_name);

            let enum_module_handle = module.module_handle_at(enum_handle.module);
            dbg_println!("Indexing type {:?} at {:?}", name, enum_module_handle);

            let abilities = enum_handle.abilities;

            let type_parameters = context.arena_vec(enum_handle.type_parameters.iter().cloned())?;

            let variant_count = enum_def.variants.len() as u16;

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
                let variant_tag = variant_tag as u16;
                let variant_name = module.identifier_at(variant_def.variant_name).into();

                let fields = variant_def
                    .fields
                    .iter()
                    .map(|f| make_arena_type(context, module, &f.signature.0))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                let fields = context.arena_vec(fields.into_iter())?;

                let field_names = variant_def
                    .fields
                    .iter()
                    .map(|f| module.identifier_at(f.name).to_owned());
                let field_names = context.arena_vec(field_names)?;

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
) -> PartialVMResult<
    ArenaVec<ArenaVec<ArenaType>>,
> {
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
    Ok(signatures)
}

// -------------------------------------------------------------------------------------------------
// Handle Translation
// -------------------------------------------------------------------------------------------------

fn field_handles(
    context: &mut PackageContext<'_>,
    module: &CompiledModule,
    structs: &[StructDef],
) -> PartialVMResult<ArenaVec<FieldHandle>> {
    let field_handles = module.field_handles().iter().map(|f_handle| {
        let def_idx = f_handle.owner;
        let owner = structs[def_idx.0 as usize].def_vtable_key.clone();
        let offset = f_handle.field as usize;
        FieldHandle { offset, owner }
    });
    context.arena_vec(field_handles.into_iter())
}

/// [SAFETY] This assumes the elements in `enums` are stable and will not move.
/// NB: This returns a vector of pointers, as we do not need to store these -- they are already
/// fixed in the arena under the EnumDefs provided.
fn variant_handles(module: &CompiledModule, enums: &[EnumDef]) -> Vec<VMPointer<VariantDef>> {
    module
        .variant_handles()
        .iter()
        .map(|variant_handle| {
            let FF::VariantHandle { enum_def, variant } = variant_handle;
            let enum_ = &enums[enum_def.0 as usize];
            let variant_ = &enum_.variants[*variant as usize];
            VMPointer::from_ref(variant_)
        })
        .collect::<Vec<_>>()
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
            let struct_def = &structs[def];
            let field_count = struct_def.fields.len() as u16;
            let instantiation_idx = struct_inst.type_parameters;
            let type_params = signatures[instantiation_idx.0 as usize].ptr_clone();
            let instantiation = signatures[struct_inst.type_parameters.0 as usize].ptr_clone();

            Ok(StructInstantiation {
                field_count,
                def_vtable_key: struct_def.def_vtable_key.clone(),
                type_params,
                instantiation,
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
            let enum_def = &enums[def];
            let variant_count_map =
                context.arena_vec(enum_def.variants.iter().map(|v| v.fields.len() as u16))?;
            let instantiation_idx = enum_inst.type_parameters;
            let type_params = signatures[instantiation_idx.0 as usize].ptr_clone();
            let instantiation = signatures[enum_inst.type_parameters.0 as usize].ptr_clone();

            let def_vtable_key = enum_def.def_vtable_key.clone();
            let enum_def = VMPointer::from_ref(enum_def);

            Ok(EnumInstantiation {
                variant_count_map,
                enum_def,
                def_vtable_key,
                type_params,
                instantiation,
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
            let instantiation = signatures[fun_inst.type_parameters.0 as usize].ptr_clone();

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
    let field_instantiations = module.field_instantiations().iter().map(|f_inst| {
        let fh_idx = f_inst.handle;
        let owner = field_handles[fh_idx.0 as usize].owner.clone();
        let offset = field_handles[fh_idx.0 as usize].offset;

        FieldInstantiation { offset, owner }
    });
    context.arena_vec(field_instantiations.into_iter())
}

/// [SAFETY] This assumes the elements in `enum_instantiations` are stable and will not move.
fn variant_instantiations(
    context: &mut PackageContext<'_>,
    module: &CompiledModule,
    enum_instantiations: &[EnumInstantiation],
) -> PartialVMResult<ArenaVec<VariantInstantiation>> {
    let variant_insts = module.variant_instantiation_handles().iter().map(|v_inst| {
        let FF::VariantInstantiationHandle { enum_def, variant } = v_inst;
        let enum_inst = VMPointer::from_ref(&enum_instantiations[enum_def.0 as usize]);
        let variant = VMPointer::from_ref(&enum_inst.enum_def.variants[*variant as usize]);
        VariantInstantiation { enum_inst, variant }
    });
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
                    PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION).with_message(
                        "Verifier failed to verify the deserialization of constants".to_owned(),
                    )
                })?
                .to_constant_value(&context.package_arena)?;
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

fn functions(
    package_context: &mut PackageContext,
    module: &input::Module,
    definitions: Definitions,
) -> PartialVMResult<ArenaVec<Function>> {
    let input::Module {
        compiled_module: module,
        functions: optimized_fns,
    } = module;
    let self_id = module.self_id().name().to_owned();

    dbg_println!(flag: function_list_sizes, "pushing {} functions", module.function_defs().len());

    let prealloc_functions: Vec<Function> = module
        .function_defs()
        .iter()
        .enumerate()
        .map(|(ndx, fun)| {
            let findex = FunctionDefinitionIndex(ndx as TableIndex);
            alloc_function(package_context, module, findex, fun)
        })
        .collect::<PartialVMResult<Vec<_>>>()?;

    let mut loaded_functions = package_context.arena_vec(prealloc_functions.into_iter())?;

    let fun_map = unique_map(
        loaded_functions
            .iter()
            .map(|fun| (fun.name.clone(), VMPointer::from_ref(fun))),
    )
    .map_err(|key| {
        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(format!(
            "Duplicate function key {}::{}",
            package_context.storage_id, key,
        ))
    })?;

    package_context.insert_vtable_functions(self_id, fun_map)?;

    let mut module_context = FunctionContext {
        package_context,
        module,
        definitions,
    };

    let mut optimized_fns = optimized_fns.clone();

    for fun in loaded_functions.iter_mut() {
        let Some(opt_fun) = optimized_fns.remove(&fun.index) else {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                    format!(
                        "failed to find function {}::{} in optimized function list",
                        package_context.storage_id, fun.name
                    ),
                ),
            );
        };
        let input::Function {
            ndx: _,
            code: opt_code,
        } = opt_fun;
        if let Some(opt_code) = opt_code {
            let jump_table_ptrs = fun.jump_tables.to_ptrs();
            fun.code = code(&mut module_context, &jump_table_ptrs, opt_code.code)?;
        }
    }

    let FunctionContext { .. } = module_context;

    Ok(loaded_functions)
}

fn alloc_function(
    context: &PackageContext,
    module: &CompiledModule,
    index: FunctionDefinitionIndex,
    def: &FunctionDefinition,
) -> PartialVMResult<Function> {
    fn jump_table(
        context: &PackageContext,
        table: &FF::VariantJumpTable,
    ) -> PartialVMResult<VariantJumpTable> {
        match &table.jump_table {
            FF::JumpTableInner::Full(items) => {
                let jump_table = context.arena_vec(items.clone().into_iter())?;
                Ok(jump_table)
            }
        }
    }

    let handle = module.function_handle_at(def.function);
    let name = module.identifier_at(handle.name).to_owned();
    let module_id = module.self_id();
    let is_entry = def.is_entry;
    let (native, def_is_native) = if def.is_native() {
        (
            context.natives.resolve(
                module_id.address(),
                module_id.name().as_str(),
                name.as_str(),
            ),
            true,
        )
    } else {
        (None, false)
    };
    let parameters = module
        .signature_at(handle.parameters)
        .0
        .iter()
        .map(|tok| make_arena_type(context, module, tok))
        .collect::<PartialVMResult<Vec<_>>>()?;
    let parameters = context.arena_vec(parameters.into_iter())?;
    // Native functions do not have a code unit
    let (locals_len, locals, jump_tables) = match &def.code {
        Some(code) => {
            let locals_len = parameters.len() + module.signature_at(code.locals).0.len();
            let locals = context.arena_vec(
                module
                    .signature_at(code.locals)
                    .0
                    .iter()
                    .map(|tok| make_arena_type(context, module, tok))
                    .collect::<PartialVMResult<Vec<_>>>()?
                    .into_iter(),
            )?;
            let jump_tables = code
                .jump_tables
                .iter()
                .map(|table| jump_table(context, table))
                .collect::<PartialVMResult<Vec<_>>>()?;
            let jump_tables = context.package_arena.alloc_vec(jump_tables.into_iter())?;
            (locals_len, locals, jump_tables)
        }
        None => (0, ArenaVec::empty(), ArenaVec::empty()),
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
        // replaced in the next step of compilation
        code: ArenaVec::empty(),
        parameters,
        locals,
        return_,
        type_parameters,
        native,
        def_is_native,
        module: module_id,
        name,
        locals_len,
        jump_tables,
    };
    Ok(fun)
}

// [ALLOC] Bytecode result is allocated in the arena
fn code(
    context: &mut FunctionContext,
    jump_tables: &[VMPointer<VariantJumpTable>],
    blocks: BTreeMap<u16, Vec<input::Bytecode>>,
) -> PartialVMResult<ArenaVec<Bytecode>> {
    let function_bytecode = flatten_and_renumber_blocks(blocks);
    let result = context.package_context.package_arena.alloc_vec(
        function_bytecode
            .into_iter()
            .map(|bc| bytecode(context, jump_tables, bc))
            .collect::<PartialVMResult<Vec<Bytecode>>>()?
            .into_iter(),
    )?;
    Ok(result)
}

fn flatten_and_renumber_blocks(
    blocks: BTreeMap<u16, Vec<input::Bytecode>>,
) -> Vec<input::Bytecode> {
    dbg_println!("Input: {:#?}", blocks);
    let mut offset_map = BTreeMap::new(); // Map line name (u16) -> new bytecode offset
    let mut concatenated = Vec::new();

    // Calculate new offsets and build concatenated bytecode
    let mut current_offset = 0;
    for (line_name, bytecodes) in &blocks {
        offset_map.insert(*line_name, current_offset);
        current_offset += bytecodes.len() as u16;
        concatenated.extend_from_slice(bytecodes);
    }
    dbg_println!("Concatenated: {:#?}", concatenated);

    // Rewrite branch instructions with new offsets
    concatenated
        .into_iter()
        .map(|bytecode| match bytecode {
            input::Bytecode::BrFalse(target) => {
                input::Bytecode::BrFalse(*offset_map.get(&target).expect("Invalid branch target"))
            }
            input::Bytecode::BrTrue(target) => {
                input::Bytecode::BrTrue(*offset_map.get(&target).expect("Invalid branch target"))
            }
            input::Bytecode::Branch(target) => {
                input::Bytecode::Branch(*offset_map.get(&target).expect("Invalid branch target"))
            }
            other => other,
        })
        .collect()
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
            let generic_ptr = &context.definitions.function_instantiations[ndx.0 as usize];
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
            let const_ptr = &context.definitions.constants[ndx.0 as usize];
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
            let struct_ptr = &context.definitions.structs[ndx.0 as usize];
            Bytecode::Pack(struct_ptr.ptr_clone())
        }
        input::Bytecode::Unpack(ndx) => {
            let struct_ptr = &context.definitions.structs[ndx.0 as usize];
            Bytecode::Unpack(struct_ptr.ptr_clone())
        }

        input::Bytecode::PackGeneric(ndx) => {
            let struct_inst_ptr = &context.definitions.struct_instantiations[ndx.0 as usize];
            Bytecode::PackGeneric(struct_inst_ptr.ptr_clone())
        }
        input::Bytecode::UnpackGeneric(ndx) => {
            let struct_inst_ptr = &context.definitions.struct_instantiations[ndx.0 as usize];
            Bytecode::UnpackGeneric(struct_inst_ptr.ptr_clone())
        }

        input::Bytecode::MutBorrowField(ndx) => {
            let field_ptr = &context.definitions.field_handles[ndx.0 as usize];
            Bytecode::MutBorrowField(field_ptr.ptr_clone())
        }

        input::Bytecode::ImmBorrowField(ndx) => {
            let field_ptr = &context.definitions.field_handles[ndx.0 as usize];
            Bytecode::ImmBorrowField(field_ptr.ptr_clone())
        }

        input::Bytecode::MutBorrowFieldGeneric(ndx) => {
            let field_inst_ptr = &context.definitions.field_instantiations[ndx.0 as usize];
            Bytecode::MutBorrowFieldGeneric(field_inst_ptr.ptr_clone())
        }
        input::Bytecode::ImmBorrowFieldGeneric(ndx) => {
            let field_inst_ptr = &context.definitions.field_instantiations[ndx.0 as usize];
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
            let variant_ptr = &context.definitions.variants[ndx.0 as usize];
            Bytecode::PackVariant(variant_ptr.ptr_clone())
        }
        input::Bytecode::UnpackVariant(ndx) => {
            let variant_ptr = &context.definitions.variants[ndx.0 as usize];
            Bytecode::UnpackVariant(variant_ptr.ptr_clone())
        }

        input::Bytecode::PackVariantGeneric(ndx) => {
            let variant_inst_ptr = &context.definitions.variant_instantiations[ndx.0 as usize];
            Bytecode::PackVariantGeneric(variant_inst_ptr.ptr_clone())
        }
        input::Bytecode::UnpackVariantGeneric(ndx) => {
            let variant_inst_ptr = &context.definitions.variant_instantiations[ndx.0 as usize];
            Bytecode::UnpackVariantGeneric(variant_inst_ptr.ptr_clone())
        }

        input::Bytecode::UnpackVariantImmRef(ndx) => {
            let variant_ptr = &context.definitions.variants[ndx.0 as usize];
            Bytecode::UnpackVariantImmRef(variant_ptr.ptr_clone())
        }
        input::Bytecode::UnpackVariantMutRef(ndx) => {
            let variant_ptr = &context.definitions.variants[ndx.0 as usize];
            Bytecode::UnpackVariantMutRef(variant_ptr.ptr_clone())
        }

        input::Bytecode::UnpackVariantGenericImmRef(ndx) => {
            let variant_inst_ptr = &context.definitions.variant_instantiations[ndx.0 as usize];
            Bytecode::UnpackVariantGenericImmRef(variant_inst_ptr.ptr_clone())
        }
        input::Bytecode::UnpackVariantGenericMutRef(ndx) => {
            let variant_inst_ptr = &context.definitions.variant_instantiations[ndx.0 as usize];
            Bytecode::UnpackVariantGenericMutRef(variant_inst_ptr.ptr_clone())
        }
        input::Bytecode::VariantSwitch(ndx) => {
            let jump_table = &jump_tables[ndx.0 as usize];
            Bytecode::VariantSwitch(jump_table.ptr_clone())
        }
    };
    Ok(bytecode)
}

fn call(
    package_context: &PackageContext,
    module: &CompiledModule,
    function_handle_index: FunctionHandleIndex,
) -> PartialVMResult<CallType> {
    let string_interner = string_interner();

    let func_handle = module.function_handle_at(function_handle_index);
    let member_name =
        string_interner.get_or_intern_ident_str(module.identifier_at(func_handle.name))?;
    let module_handle = module.module_handle_at(func_handle.module);
    let runtime_id = module.module_id_for_handle(module_handle);
    let module_name = string_interner.get_or_intern_ident_str(runtime_id.name())?;
    let vtable_key = VirtualTableKey {
        package_key: *runtime_id.address(),
        inner_pkg_key: IntraPackageKey {
            module_name,
            member_name,
        },
    };
    dbg_println!(flag: function_resolution, "Resolving function: {:?}", vtable_key);
    Ok(match package_context.try_resolve_function(&vtable_key) {
        Some(func) => CallType::Direct(func),
        None => CallType::Virtual(vtable_key),
    })
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
            let datatype_name = string_interner()
                .get_or_intern_ident_str(module.identifier_at(datatype_handle.name))?;
            let module_handle = module.module_handle_at(datatype_handle.module);
            let runtime_address = module.address_identifier_at(module_handle.address);
            let module_name = string_interner()
                .get_or_intern_ident_str(module.identifier_at(module_handle.name))?;
            let cache_idx = VirtualTableKey {
                package_key: *runtime_address,
                inner_pkg_key: IntraPackageKey {
                    module_name,
                    member_name: datatype_name.to_owned(),
                },
            };
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
            let datatype_name = string_interner()
                .get_or_intern_ident_str(module.identifier_at(datatype_handle.name))?;
            let module_handle = module.module_handle_at(datatype_handle.module);
            let runtime_address = module.address_identifier_at(module_handle.address);
            let module_name = string_interner()
                .get_or_intern_ident_str(module.identifier_at(module_handle.name))?;
            let cache_idx = VirtualTableKey {
                package_key: *runtime_address,
                inner_pkg_key: IntraPackageKey {
                    module_name,
                    member_name: datatype_name.to_owned(),
                },
            };
            ArenaType::DatatypeInstantiation(context.arena_box((cache_idx, type_parameters))?)
        }
    };
    Ok(res)
}
