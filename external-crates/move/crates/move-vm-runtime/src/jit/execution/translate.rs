// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::arena::Arena,
    dbg_println,
    execution::{
        dispatch_tables::{CachedDatatype, IntraPackageKey, PackageVirtualTable, VirtualTableKey},
        values::Value,
    },
    jit::execution::ast::*,
    jit::optimization::ast as input,
    natives::functions::NativeFunctions,
    shared::{
        binary_cache::BinaryCache,
        linkage_context::LinkageContext,
        types::{PackageStorageId, RuntimePackageId},
        vm_pointer::{self, VMPointer},
    },
    string_interner,
};
use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        self as FF, CompiledModule, EnumDefinitionIndex, FunctionDefinition,
        FunctionDefinitionIndex, FunctionHandleIndex, SignatureIndex, SignatureToken,
        StructDefinitionIndex, StructFieldInformation, TableIndex,
    },
    internals::ModuleIndex,
};
use move_core_types::{identifier::Identifier, language_storage::ModuleId, vm_status::StatusCode};
use std::collections::{BTreeMap, BTreeSet, HashMap};

struct PackageContext<'natives> {
    pub natives: &'natives NativeFunctions,
    pub type_origin_table: HashMap<IntraPackageKey, PackageStorageId>,

    pub storage_id: PackageStorageId,
    pub runtime_id: RuntimePackageId,
    // NB: this is under the package's context so we don't need to further resolve by
    // address in this table.
    pub loaded_modules: BinaryCache<Identifier, Module>,

    // NB: this is needed for the bytecode verifier. If we update the bytecode verifier we should
    // be able to remove this.
    pub compiled_modules: BinaryCache<Identifier, CompiledModule>,

    // NB: All code and signatures are allocated in this arena.
    pub package_arena: Arena,
    pub vtable: PackageVirtualTable,
}

struct FunctionContext<'pkg_ctxt, 'natives, 'cache> {
    package_context: &'pkg_ctxt PackageContext<'natives>,
    module: &'pkg_ctxt CompiledModule,
    indicies: &'cache IndexMaps,
    single_signature_token_map: BTreeMap<SignatureIndex, VMPointer<Type>>,
}

struct IndexMaps {
    structs: Vec<VMPointer<StructDef>>,
    struct_instantiations: Vec<VMPointer<StructInstantiation>>,
    enums: Vec<VMPointer<EnumDef>>,
    enum_instantiations: Vec<VMPointer<EnumInstantiation>>,
    variants: Vec<VMPointer<VariantDef>>,
    variant_instantiations: Vec<VMPointer<VariantInstantiation>>,
    function_instantiations: Vec<VMPointer<FunctionInstantiation>>,
    field_handles: Vec<VMPointer<FieldHandle>>,
    field_instantiations: Vec<VMPointer<FieldInstantiation>>,
    instantiation_signatures: SignatureCache,
    constants: ConstantCache,
}

impl PackageContext<'_> {
    fn insert_and_make_module_function_vtable(
        &mut self,
        module_name: Identifier,
        vtable: impl IntoIterator<Item = (Identifier, VMPointer<Function>)>,
    ) -> PartialVMResult<Vec<VMPointer<Function>>> {
        let string_interner = string_interner();
        let module_name = string_interner.get_or_intern_identifier(&module_name)?;
        let mut output = vec![];
        for (name, func) in vtable {
            let member_name = string_interner.get_or_intern_identifier(&name)?;
            let key = IntraPackageKey {
                module_name,
                member_name,
            };
            output.push(func);
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
        Ok(output)
    }

    fn try_resolve_function(&self, vtable_entry: &VirtualTableKey) -> Option<VMPointer<Function>> {
        if vtable_entry.package_key != self.runtime_id {
            return None;
        }
        self.vtable
            .functions
            .get(&vtable_entry.inner_pkg_key)
            .map(|f| VMPointer::new(f.to_ref()))
    }
}

// -------------------------------------------------------------------------------------------------
// Package Translation

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
        loaded_modules: BinaryCache::new(),
        compiled_modules: BinaryCache::new(),
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
                .contains(&dep.name().to_owned())
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

        package_context
            .loaded_modules
            .insert(loaded_module.id.name().to_owned(), loaded_module)?;
        package_context.compiled_modules.insert(
            input_module.compiled_module.self_id().name().to_owned(),
            input_module.compiled_module,
        )?;
    }

    let PackageContext {
        storage_id,
        natives: _,
        runtime_id,
        loaded_modules,
        compiled_modules: _,
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
    package_context: &mut PackageContext<'_>,
    link_context: &LinkageContext,
    package_id: PackageStorageId,
    module: &mut input::Module,
) -> PartialVMResult<Module> {
    let self_id = module.compiled_module.self_id();
    dbg_println!("Loading module: {}", self_id);

    // Load module types
    load_module_types(
        package_context,
        link_context,
        package_context.runtime_id,
        package_id,
        &module.compiled_module,
    )?;
    dbg_println!("Module types loaded");

    let comp_module = &module.compiled_module;

    // Initialize module data
    let type_refs = initialize_type_refs(comp_module)?;

    let structs = structs(package_context, &type_refs, comp_module)?;
    let enums = enums(package_context, &type_refs, comp_module)?;

    let instantiation_signatures = instantiation_signatures(package_context, comp_module)?;

    let struct_instantiations = struct_instantiations(
        package_context,
        &instantiation_signatures,
        &structs,
        comp_module,
    )?;
    let enum_instantiations = enum_instantiations(
        package_context,
        &instantiation_signatures,
        &enums,
        comp_module,
    )?;

    let variants = variant_handles(package_context, &enums, comp_module);
    let variant_instantiations =
        variant_instantiations(package_context, &enum_instantiations, comp_module)?;

    // Process field handles and instantiations
    let field_handles = field_handles(package_context, &structs, comp_module)?;
    let field_instantiations = field_instantiations(package_context, &field_handles, comp_module)?;

    let constants = constants(package_context, comp_module)?;

    // Process functions and function instantiations
    // Function loading is effectful; they all go into the arena.
    let function_instantiations =
        function_instantiations(package_context, &instantiation_signatures, comp_module)?;

    let index_map = IndexMaps {
        structs,
        struct_instantiations,
        enums,
        enum_instantiations,
        variants,
        variant_instantiations,
        function_instantiations,
        field_handles,
        field_instantiations,
        instantiation_signatures,
        constants,
    };

    let (functions, single_signature_token_map) = functions(package_context, &index_map, module)?;
    let IndexMaps {
        structs,
        struct_instantiations,
        enums,
        enum_instantiations,
        variants,
        variant_instantiations,
        function_instantiations,
        field_handles,
        field_instantiations,
        instantiation_signatures,
        constants,
    } = index_map;

    // Build and return the module
    Ok(Module {
        id: self_id,
        type_refs,
        structs,
        struct_instantiations,
        enums,
        enum_instantiations,
        function_instantiations,
        field_handles,
        field_instantiations,
        single_signature_token_map,
        instantiation_signatures,
        variant_handles: variants,
        variant_instantiations,
        constants,
        functions,
    })
}

// -------------------------------------------------------------------------------------------------
// Type Definitions Translation

fn initialize_type_refs(module: &CompiledModule) -> PartialVMResult<Vec<IntraPackageKey>> {
    module
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
        .collect()
}

fn instantiation_signatures(
    package_context: &mut PackageContext,
    module: &CompiledModule,
) -> PartialVMResult<SignatureCache> {
    module
        .signatures()
        .iter()
        .map(|signature| {
            let instantiation = signature
                .0
                .iter()
                .map(|ty| make_type(module, ty))
                .collect::<PartialVMResult<Vec<_>>>()?;

            // Allocate the vector in the package arena and create a VMPointer
            let instantiation_ptr =
                VMPointer::new(package_context.package_arena.alloc_item(instantiation)?);
            Ok(instantiation_ptr)
        })
        .collect::<PartialVMResult<Vec<_>>>()
}

fn structs(
    package_context: &mut PackageContext<'_>,
    type_refs: &[IntraPackageKey],
    module: &CompiledModule,
) -> PartialVMResult<Vec<VMPointer<StructDef>>> {
    module
        .struct_defs()
        .iter()
        .map(|struct_def| {
            let key = type_refs[struct_def.struct_handle.into_index()];
            let type_ = package_context.vtable.types.type_at(&key);
            let struct_type = type_.get_struct()?;
            let field_count = struct_type.fields.len() as u16;

            let def = StructDef {
                field_count,
                def_vtable_key: VirtualTableKey {
                    package_key: package_context.runtime_id,
                    inner_pkg_key: key,
                },
            };
            let def_ptr = VMPointer::new(package_context.package_arena.alloc_item(def)?);
            Ok(def_ptr)
        })
        .collect::<PartialVMResult<Vec<_>>>()
}

fn struct_instantiations(
    package_context: &mut PackageContext<'_>,
    signature_cache: &SignatureCache,
    structs: &[VMPointer<StructDef>],
    module: &CompiledModule,
) -> PartialVMResult<Vec<VMPointer<StructInstantiation>>> {
    module
        .struct_instantiations()
        .iter()
        .map(|struct_inst| {
            let struct_def = structs[struct_inst.def.into_index()].to_ref();
            let field_count = struct_def.field_count;

            let instantiation_idx = struct_inst.type_parameters;
            let instantiation_signature =
                signature_cache[instantiation_idx.into_index()].ptr_clone();

            let instantiation = StructInstantiation {
                field_count,
                def_vtable_key: struct_def.def_vtable_key.clone(),
                type_params: instantiation_signature,
            };
            let inst_ptr = VMPointer::new(package_context.package_arena.alloc_item(instantiation)?);
            Ok(inst_ptr)
        })
        .collect::<PartialVMResult<Vec<_>>>()
}

fn enums(
    package_context: &mut PackageContext<'_>,
    type_refs: &[IntraPackageKey],
    module: &CompiledModule,
) -> PartialVMResult<Vec<VMPointer<EnumDef>>> {
    module
        .enum_defs()
        .iter()
        .map(|enum_def| {
            let key = type_refs[enum_def.enum_handle.into_index()];
            let type_ = package_context.vtable.types.type_at(&key);
            let enum_type = type_.get_enum()?;
            let variant_count = enum_type.variants.len() as u16;

            // NB: Note the knot-tying we do here, so variants point back to the definition that
            // holds them

            // Allocate the enum definition in the arena
            let def = EnumDef {
                variant_count,
                variants: vec![],
                def_vtable_key: VirtualTableKey {
                    package_key: package_context.runtime_id,
                    inner_pkg_key: key,
                },
            };
            let def_ptr = VMPointer::new(package_context.package_arena.alloc_item(def)?);

            // Generate the variant entries, pointing back to that allocation
            let variants = enum_type
                .variants
                .iter()
                .enumerate()
                .map(|(variant_tag, variant_type)| VariantDef {
                    enum_def: def_ptr.ptr_clone(),
                    variant_tag: variant_tag as u16,
                    field_count: variant_type.fields.len() as u16,
                    field_types: variant_type.fields.clone(),
                })
                .collect();

            // Tie the knot
            assert!(std::mem::replace(&mut def_ptr.to_mut_ref().variants, variants).is_empty());

            Ok(def_ptr)
        })
        .collect::<PartialVMResult<Vec<_>>>()
}

fn enum_instantiations(
    package_context: &mut PackageContext<'_>,
    signature_cache: &SignatureCache,
    enums: &[VMPointer<EnumDef>],
    module: &CompiledModule,
) -> PartialVMResult<Vec<VMPointer<EnumInstantiation>>> {
    module
        .enum_instantiations()
        .iter()
        .map(|enum_inst| {
            let enum_def = enums[enum_inst.def.into_index()].to_ref();
            let variant_count_map = enum_def.variants.iter().map(|v| v.field_count).collect();
            let instantiation_idx = enum_inst.type_parameters;
            let instantiation_signature =
                signature_cache[instantiation_idx.into_index()].ptr_clone();

            let instantiation = EnumInstantiation {
                variant_count_map,
                def_vtable_key: enum_def.def_vtable_key.clone(),
                type_params: instantiation_signature,
            };
            let inst_ptr = VMPointer::new(package_context.package_arena.alloc_item(instantiation)?);
            Ok(inst_ptr)
        })
        .collect::<PartialVMResult<Vec<_>>>()
}

/// NOTE: Must be called after enum creation. Relies on the fact that `enum` definition `variant`
/// vectors will not be resized or moved, as we grab direct pointers into them to avoid duplicate
/// allocations.
fn variant_handles(
    _package_context: &mut PackageContext<'_>,
    enums: &[VMPointer<EnumDef>],
    module: &CompiledModule,
) -> Vec<VMPointer<VariantDef>> {
    module
        .variant_handles()
        .iter()
        .map(|variant| {
            let FF::VariantHandle { enum_def, variant } = variant;
            let tag = variant;
            let enum_def = enums[enum_def.into_index()];
            // NB: These are stable so long as the enum definitions are already fixed.
            VMPointer::from_ref(&enum_def.to_ref().variants[*tag as usize])
        })
        .collect::<Vec<_>>()
}

/// NOTE: Must be called after enum creation. Relies on the fact that `enum` definition `variant`
/// vectors will not be resized or moved, as we grab direct pointers into them to avoid duplicate
/// allocations.
fn variant_instantiations(
    package_context: &mut PackageContext<'_>,
    enum_instantiations: &[VMPointer<EnumInstantiation>],
    module: &CompiledModule,
) -> PartialVMResult<Vec<VMPointer<VariantInstantiation>>> {
    module
        .variant_instantiation_handles()
        .iter()
        .map(|variant_inst| {
            let FF::VariantInstantiationHandle { enum_def, variant } = variant_inst;
            let variant = *variant;
            let enum_def = enum_instantiations[enum_def.into_index()];
            let inst = VariantInstantiation {
                enum_inst: enum_def,
                variant_tag: variant,
            };
            let inst_ptr = VMPointer::new(package_context.package_arena.alloc_item(inst)?);
            Ok(inst_ptr)
        })
        .collect::<PartialVMResult<Vec<_>>>()
}

fn constants(
    package_context: &mut PackageContext,
    module: &CompiledModule,
) -> PartialVMResult<ConstantCache> {
    module
        .constant_pool()
        .iter()
        .map(|constant| {
            let value = Value::deserialize_constant(constant)
                .ok_or_else(|| {
                    PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION).with_message(
                        "Verifier failed to verify the deserialization of constants".to_owned(),
                    )
                })?
                .to_constant_value()?;
            let type_ = make_type(module, &constant.type_)?;
            let size = constant.data.len() as u64;
            let const_ = Constant { value, type_, size };
            let const_ptr = VMPointer::new(package_context.package_arena.alloc_item(const_)?);
            Ok(const_ptr)
        })
        .collect::<PartialVMResult<ConstantCache>>()
}

fn field_handles(
    package_context: &mut PackageContext<'_>,
    structs: &[VMPointer<StructDef>],
    module: &CompiledModule,
) -> PartialVMResult<Vec<VMPointer<FieldHandle>>> {
    module
        .field_handles()
        .iter()
        .map(|f_handle| {
            let owner = structs[f_handle.owner.into_index()]
                .to_ref()
                .def_vtable_key
                .clone();
            let offset = f_handle.field as usize;
            let handle = FieldHandle { offset, owner };
            let handle_ptr = VMPointer::new(package_context.package_arena.alloc_item(handle)?);
            Ok(handle_ptr)
        })
        .collect::<PartialVMResult<Vec<_>>>()
}

fn field_instantiations(
    package_context: &mut PackageContext<'_>,
    field_handles: &[VMPointer<FieldHandle>],
    module: &CompiledModule,
) -> PartialVMResult<Vec<VMPointer<FieldInstantiation>>> {
    module
        .field_instantiations()
        .iter()
        .map(|f_inst| {
            let fh_idx = f_inst.handle;
            let (owner, offset) = {
                let handle = field_handles[fh_idx.into_index()].to_ref();
                (handle.owner.clone(), handle.offset)
            };

            let handle = FieldInstantiation { offset, owner };
            let handle_ptr = VMPointer::new(package_context.package_arena.alloc_item(handle)?);
            Ok(handle_ptr)
        })
        .collect::<PartialVMResult<Vec<_>>>()
}

// -------------------------------------------------------------------------------------------------
// Function Translation

fn functions(
    package_context: &mut PackageContext,
    indicies: &IndexMaps,
    module: &mut input::Module,
) -> PartialVMResult<(
    Vec<VMPointer<Function>>,
    BTreeMap<SignatureIndex, VMPointer<Type>>,
)> {
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
            alloc_function(
                package_context,
                &indicies.instantiation_signatures,
                module,
                findex,
                fun,
            )
        })
        .collect::<PartialVMResult<Vec<_>>>()?;
    let loaded_functions = package_context
        .package_arena
        .alloc_slice(prealloc_functions)?;

    let functions = package_context.insert_and_make_module_function_vtable(
        self_id,
        vm_pointer::mut_to_ref_slice(loaded_functions)
            .iter()
            .map(|function| (function.name.clone(), VMPointer::new(function))),
    )?;

    let mut module_context = FunctionContext {
        package_context,
        module,
        indicies,
        single_signature_token_map: BTreeMap::new(),
    };

    for (alloc, _) in vm_pointer::to_mut_ref_slice(loaded_functions)
        .iter_mut()
        .zip(module.function_defs())
    {
        let Some(opt_fun) = optimized_fns.remove(&alloc.index) else {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                    "failed to find module function in optimized function list".to_string(),
                ),
            );
        };
        let input::Function {
            ndx: _,
            code: opt_code,
        } = opt_fun;
        if let Some(opt_code) = opt_code {
            alloc.code = code(&mut module_context, &alloc.jump_tables, opt_code.code)?;
        }
    }

    let FunctionContext {
        single_signature_token_map,
        ..
    } = module_context;

    Ok((functions, single_signature_token_map))
}

fn function_instantiations(
    package_context: &mut PackageContext,
    signature_cache: &SignatureCache,
    module: &CompiledModule,
) -> PartialVMResult<Vec<VMPointer<FunctionInstantiation>>> {
    dbg_println!(flag: function_list_sizes, "handle size: {}", module.function_handles().len());
    module
        .function_instantiations()
        .iter()
        .map(|func_inst| {
            let handle = call(package_context, module, func_inst.handle)?;

            let instantiation_idx = func_inst.type_parameters;
            let instantiation_signature =
                signature_cache[instantiation_idx.into_index()].ptr_clone();
            let inst = FunctionInstantiation {
                fn_call: handle,
                instantiation_signature,
            };
            let inst_ptr = package_context.package_arena.alloc_item(inst)?;
            let inst_ptr = VMPointer::new(inst_ptr);
            Ok(inst_ptr)
        })
        .collect()
}

fn alloc_function(
    context: &mut PackageContext,
    signature_cache: &SignatureCache,
    module: &CompiledModule,
    index: FunctionDefinitionIndex,
    def: &FunctionDefinition,
) -> PartialVMResult<Function> {
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
    let parameters = signature_cache[handle.parameters.into_index()];
    // Native functions do not have a code unit
    let (locals_len, locals, jump_tables) = match &def.code {
        Some(code) => {
            let len = parameters.to_ref().len() + module.signature_at(code.locals).0.len();
            let locals = Some(signature_cache[code.locals.into_index()]);
            let slice_tables = context
                .package_arena
                .alloc_slice(code.jump_tables.clone())?;
            let jump_tables = vm_pointer::mut_to_ref_slice(slice_tables)
                .iter()
                .map(|table| VMPointer::new(table))
                .collect::<Vec<_>>();
            (len, locals, jump_tables)
        }
        None => (0, None, vec![]),
    };
    let return_ = signature_cache[handle.return_.into_index()];
    let type_parameters = handle.type_parameters.clone();
    let fun = Function {
        file_format_version: module.version(),
        index,
        is_entry,
        visibility: def.visibility,
        code: vm_pointer::null_ptr(),
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

fn code(
    context: &mut FunctionContext,
    jump_tables: &[VMPointer<FF::VariantJumpTable>],
    blocks: BTreeMap<u16, Vec<input::Bytecode>>,
) -> PartialVMResult<*const [Bytecode]> {
    let function_bytecode = flatten_and_renumber_blocks(blocks);
    let result: *mut [Bytecode] = context.package_context.package_arena.alloc_slice(
        function_bytecode
            .iter()
            .map(|bc| bytecode(context, jump_tables, bc))
            .collect::<PartialVMResult<Vec<Bytecode>>>()?,
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
    jump_tables: &[VMPointer<FF::VariantJumpTable>],
    bytecode: &input::Bytecode,
) -> PartialVMResult<Bytecode> {
    let bytecode = match bytecode {
        // Calls -- these get compiled to something more-direct here
        input::Bytecode::Call(ndx) => {
            let call_type = call(context.package_context, context.module, *ndx)?;
            match call_type {
                CallType::Direct(func) => Bytecode::DirectCall(func),
                CallType::Virtual(vtable) => Bytecode::VirtualCall(vtable),
            }
        }

        // For now, generic calls retain an index so we can look up their signature as well.
        input::Bytecode::CallGeneric(ndx) => {
            Bytecode::CallGeneric(context.indicies.function_instantiations[ndx.into_index()])
        }

        // Standard Codes
        input::Bytecode::Pop => Bytecode::Pop,
        input::Bytecode::Ret => Bytecode::Ret,
        input::Bytecode::BrTrue(n) => Bytecode::BrTrue(*n),
        input::Bytecode::BrFalse(n) => Bytecode::BrFalse(*n),
        input::Bytecode::Branch(n) => Bytecode::Branch(*n),

        input::Bytecode::LdU256(n) => Bytecode::LdU256(n.clone()),
        input::Bytecode::LdU128(n) => Bytecode::LdU128(n.clone()),
        input::Bytecode::LdU16(n) => Bytecode::LdU16(*n),
        input::Bytecode::LdU32(n) => Bytecode::LdU32(*n),
        input::Bytecode::LdU64(n) => Bytecode::LdU64(*n),
        input::Bytecode::LdU8(n) => Bytecode::LdU8(*n),

        input::Bytecode::LdConst(ndx) => {
            Bytecode::LdConst(context.indicies.constants[ndx.into_index()])
        }
        input::Bytecode::LdTrue => Bytecode::LdTrue,
        input::Bytecode::LdFalse => Bytecode::LdFalse,

        input::Bytecode::CopyLoc(ndx) => Bytecode::CopyLoc(*ndx),
        input::Bytecode::MoveLoc(ndx) => Bytecode::MoveLoc(*ndx),
        input::Bytecode::StLoc(ndx) => Bytecode::StLoc(*ndx),
        input::Bytecode::ReadRef => Bytecode::ReadRef,
        input::Bytecode::WriteRef => Bytecode::WriteRef,
        input::Bytecode::FreezeRef => Bytecode::FreezeRef,
        input::Bytecode::MutBorrowLoc(ndx) => Bytecode::MutBorrowLoc(*ndx),
        input::Bytecode::ImmBorrowLoc(ndx) => Bytecode::ImmBorrowLoc(*ndx),

        // Structs and Fields
        input::Bytecode::Pack(ndx) => Bytecode::Pack(context.indicies.structs[ndx.into_index()]),
        input::Bytecode::PackGeneric(ndx) => {
            Bytecode::PackGeneric(context.indicies.struct_instantiations[ndx.into_index()])
        }
        input::Bytecode::Unpack(ndx) => {
            Bytecode::Unpack(context.indicies.structs[ndx.into_index()])
        }
        input::Bytecode::UnpackGeneric(ndx) => {
            Bytecode::UnpackGeneric(context.indicies.struct_instantiations[ndx.into_index()])
        }
        input::Bytecode::MutBorrowField(ndx) => {
            Bytecode::MutBorrowField(context.indicies.field_handles[ndx.into_index()])
        }
        input::Bytecode::MutBorrowFieldGeneric(ndx) => {
            Bytecode::MutBorrowFieldGeneric(context.indicies.field_instantiations[ndx.into_index()])
        }
        input::Bytecode::ImmBorrowField(ndx) => {
            Bytecode::ImmBorrowField(context.indicies.field_handles[ndx.into_index()])
        }
        input::Bytecode::ImmBorrowFieldGeneric(ndx) => {
            Bytecode::ImmBorrowFieldGeneric(context.indicies.field_instantiations[ndx.into_index()])
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
            let vec_type = check_and_cache_vector_type(context, si)?;
            Bytecode::VecPack(vec_type, *size)
        }
        input::Bytecode::VecLen(si) => {
            let vec_type = check_and_cache_vector_type(context, si)?;
            Bytecode::VecLen(vec_type)
        }
        input::Bytecode::VecImmBorrow(si) => {
            let vec_type = check_and_cache_vector_type(context, si)?;
            Bytecode::VecImmBorrow(vec_type)
        }
        input::Bytecode::VecMutBorrow(si) => {
            let vec_type = check_and_cache_vector_type(context, si)?;
            Bytecode::VecMutBorrow(vec_type)
        }
        input::Bytecode::VecPushBack(si) => {
            let vec_type = check_and_cache_vector_type(context, si)?;
            Bytecode::VecPushBack(vec_type)
        }
        input::Bytecode::VecPopBack(si) => {
            let vec_type = check_and_cache_vector_type(context, si)?;
            Bytecode::VecPopBack(vec_type)
        }
        input::Bytecode::VecUnpack(si, size) => {
            let vec_type = check_and_cache_vector_type(context, si)?;
            Bytecode::VecUnpack(vec_type, *size)
        }
        input::Bytecode::VecSwap(si) => {
            let vec_type = check_and_cache_vector_type(context, si)?;
            Bytecode::VecSwap(vec_type)
        }

        // Enums and Variants
        input::Bytecode::PackVariant(ndx) => {
            Bytecode::PackVariant(context.indicies.variants[ndx.into_index()])
        }
        input::Bytecode::PackVariantGeneric(ndx) => {
            Bytecode::PackVariantGeneric(context.indicies.variant_instantiations[ndx.into_index()])
        }
        input::Bytecode::UnpackVariant(ndx) => {
            Bytecode::UnpackVariant(context.indicies.variants[ndx.into_index()])
        }
        input::Bytecode::UnpackVariantImmRef(ndx) => {
            Bytecode::UnpackVariantImmRef(context.indicies.variants[ndx.into_index()])
        }
        input::Bytecode::UnpackVariantMutRef(ndx) => {
            Bytecode::UnpackVariantMutRef(context.indicies.variants[ndx.into_index()])
        }
        input::Bytecode::UnpackVariantGeneric(ndx) => Bytecode::UnpackVariantGeneric(
            context.indicies.variant_instantiations[ndx.into_index()],
        ),
        input::Bytecode::UnpackVariantGenericImmRef(ndx) => Bytecode::UnpackVariantGenericImmRef(
            context.indicies.variant_instantiations[ndx.into_index()],
        ),
        input::Bytecode::UnpackVariantGenericMutRef(ndx) => Bytecode::UnpackVariantGenericMutRef(
            context.indicies.variant_instantiations[ndx.into_index()],
        ),
        input::Bytecode::VariantSwitch(ndx) => {
            Bytecode::VariantSwitch(jump_tables[ndx.into_index()])
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

fn load_module_types(
    package_context: &mut PackageContext<'_>,
    _link_context: &LinkageContext,
    _package_uid: RuntimePackageId,
    package_id: PackageStorageId,
    module: &CompiledModule,
) -> PartialVMResult<()> {
    let module_id = module.self_id();
    let module_name = string_interner().get_or_intern_ident_str(module_id.name())?;

    let mut cached_types = vec![];

    for (idx, struct_def) in module.struct_defs().iter().enumerate() {
        let struct_handle = module.datatype_handle_at(struct_def.struct_handle);
        let name = module.identifier_at(struct_handle.name);

        let member_name = string_interner().get_or_intern_ident_str(name)?;
        let struct_key = IntraPackageKey {
            module_name,
            member_name,
        };

        if package_context
            .vtable
            .types
            .contains_cached_type(&struct_key)
        {
            debug_assert!(false, "Double-loading types");
        }

        let struct_module_handle = module.module_handle_at(struct_handle.module);
        dbg_println!("Indexing type {:?} at {:?}", name, struct_module_handle);
        // NB: It is the responsibility of the adapter to determine the correct type origin table,
        // and pass a full and complete representation of it in with the package.
        let defining_address = package_context
            .type_origin_table
            .get(&struct_key)
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::LOOKUP_FAILED).with_message(format!(
                    "Type origin not found for type {:?} in module {:?}",
                    name, module_id
                ))
            })?;
        dbg_println!("Package ID: {:?}", package_id);
        dbg_println!("Defining Address: {:?}", defining_address);
        let defining_id = ModuleId::new(*defining_address, module_id.name().to_owned());

        let field_names = match &struct_def.field_information {
            StructFieldInformation::Native => vec![],
            StructFieldInformation::Declared(field_info) => field_info
                .iter()
                .map(|f| module.identifier_at(f.name).to_owned())
                .collect(),
        };

        let StructFieldInformation::Declared(fields) = &struct_def.field_information else {
            unreachable!("native structs have been removed");
        };

        let fields = fields
            .iter()
            .map(|f| make_type(module, &f.signature.0))
            .collect::<PartialVMResult<Vec<Type>>>()?;

        package_context.vtable.types.cache_datatype(
            struct_key,
            CachedDatatype {
                abilities: struct_handle.abilities,
                type_parameters: struct_handle.type_parameters.clone(),
                name: name.to_owned(),
                defining_id,
                runtime_id: module_id.clone(),
                depth: None,
                datatype_info: Datatype::Struct(StructType {
                    fields,
                    field_names,
                    struct_def: StructDefinitionIndex(idx as u16),
                }),
                module_key: module_name,
                member_key: member_name,
            },
        )?;

        cached_types.push(struct_key);
    }

    for (idx, enum_def) in module.enum_defs().iter().enumerate() {
        let enum_handle = module.datatype_handle_at(enum_def.enum_handle);
        let name = module.identifier_at(enum_handle.name);

        let member_name = string_interner().get_or_intern_ident_str(name)?;
        let enum_key = IntraPackageKey {
            module_name,
            member_name,
        };

        if package_context.vtable.types.contains_cached_type(&enum_key) {
            continue;
        }

        let enum_module_handle = module.module_handle_at(enum_handle.module);
        dbg_println!("Indexing type {:?} at {:?}", name, enum_module_handle);
        // NB: It is the responsibility of the adapter to determine the correct type origin table,
        // and pass a full and complete representation of it in with the package.
        let defining_address = package_context
            .type_origin_table
            .get(&enum_key)
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::LOOKUP_FAILED).with_message(format!(
                    "Type origin not found for type {:?} in module {:?}",
                    name, module_id
                ))
            })?;
        dbg_println!("Package ID: {:?}", package_id);
        dbg_println!("Enum Defining Address: {:?}", defining_address);
        let defining_id = ModuleId::new(*defining_address, module_id.name().to_owned());

        let variants: Vec<VariantType> = enum_def
            .variants
            .iter()
            .enumerate()
            .map(|(variant_tag, variant_def)| {
                Ok(VariantType {
                    variant_name: module.identifier_at(variant_def.variant_name).to_owned(),
                    fields: variant_def
                        .fields
                        .iter()
                        .map(|f| make_type(module, &f.signature.0))
                        .collect::<PartialVMResult<_>>()?,
                    field_names: variant_def
                        .fields
                        .iter()
                        .map(|f| module.identifier_at(f.name).to_owned())
                        .collect(),
                    enum_def: EnumDefinitionIndex(idx as u16),
                    variant_tag: variant_tag as u16,
                })
            })
            .collect::<PartialVMResult<_>>()?;

        package_context.vtable.types.cache_datatype(
            enum_key,
            CachedDatatype {
                abilities: enum_handle.abilities,
                type_parameters: enum_handle.type_parameters.clone(),
                name: name.to_owned(),
                defining_id,
                runtime_id: module_id.clone(),
                depth: None,
                datatype_info: Datatype::Enum(EnumType {
                    variants,
                    enum_def: EnumDefinitionIndex(idx as u16),
                }),
                module_key: module_name,
                member_key: member_name,
            },
        )?;
        cached_types.push(enum_key);
    }

    Ok(())
}

/// Convert a signature token type into its execution counterpart, including converting datatypes
/// into their VTable entry keys.
pub fn make_type(module: &CompiledModule, tok: &SignatureToken) -> PartialVMResult<Type> {
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
        SignatureToken::Vector(inner_tok) => Type::Vector(Box::new(make_type(module, inner_tok)?)),
        SignatureToken::Reference(inner_tok) => {
            Type::Reference(Box::new(make_type(module, inner_tok)?))
        }
        SignatureToken::MutableReference(inner_tok) => {
            Type::MutableReference(Box::new(make_type(module, inner_tok)?))
        }
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
            Type::Datatype(cache_idx)
        }
        SignatureToken::DatatypeInstantiation(inst) => {
            let (sh_idx, tys) = &**inst;
            let type_parameters: Vec<_> = tys
                .iter()
                .map(|tok| make_type(module, tok))
                .collect::<PartialVMResult<_>>()?;
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
            Type::DatatypeInstantiation(Box::new((cache_idx, type_parameters)))
        }
    };
    Ok(res)
}

fn check_and_cache_vector_type(
    context: &mut FunctionContext,
    signature_index: &SignatureIndex,
) -> PartialVMResult<VMPointer<Type>> {
    if let Some(type_) = context.single_signature_token_map.get(signature_index) {
        Ok(type_.ptr_clone())
    } else {
        let sig_token = match context.module.signature_at(*signature_index).0.first() {
            None => {
                return Err(
                    PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION).with_message(
                        "the type argument for vector-related bytecode \
                        expects one and only one signature token"
                            .to_owned(),
                    ),
                );
            }
            Some(sig_token) => sig_token,
        };
        let ty = make_type(context.module, sig_token)?;
        let ty_ptr = VMPointer::new(context.package_context.package_arena.alloc_item(ty)?);
        assert!(context
            .single_signature_token_map
            .insert(*signature_index, ty_ptr)
            .is_none());
        Ok(ty_ptr)
    }
}
