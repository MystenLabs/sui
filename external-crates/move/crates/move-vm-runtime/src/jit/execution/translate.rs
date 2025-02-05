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
        CompiledModule, EnumDefinitionIndex, FunctionDefinition, FunctionDefinitionIndex,
        FunctionHandleIndex, SignatureIndex, SignatureToken, StructDefinitionIndex,
        StructFieldInformation, TableIndex,
    },
};
use move_core_types::{identifier::Identifier, language_storage::ModuleId, vm_status::StatusCode};
use std::collections::{btree_map, BTreeMap, BTreeSet, HashMap};

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

    // NB: All things except for types are allocated into this arena.
    pub package_arena: Arena,
    pub vtable: PackageVirtualTable,
}

struct FunctionContext<'pkg_ctxt, 'natives> {
    package_context: &'pkg_ctxt PackageContext<'natives>,
    module: &'pkg_ctxt CompiledModule,
    single_signature_token_map: BTreeMap<SignatureIndex, Type>,
}

impl PackageContext<'_> {
    fn insert_and_make_module_function_vtable(
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

    fn try_resolve_function(&self, vtable_entry: &VirtualTableKey) -> Option<VMPointer<Function>> {
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

    // Initialize module data
    let type_refs = initialize_type_refs(&module.compiled_module)?;

    let mut instantiation_signatures: BTreeMap<SignatureIndex, Vec<Type>> = BTreeMap::new();

    let structs = structs(package_context, &module.compiled_module, &type_refs)?;
    let enums = enums(package_context, &module.compiled_module, &type_refs)?;

    let struct_instantiations = struct_instantiations(
        &mut instantiation_signatures,
        &module.compiled_module,
        &structs,
    )?;
    let enum_instantiations = enum_instantiations(
        &mut instantiation_signatures,
        &module.compiled_module,
        &enums,
    )?;

    // Process functions and function instantiations
    // Function loading is effectful; they all go into the arena.
    let single_signature_token_map = functions(package_context, module)?;
    let function_instantiations = function_instantiations(
        package_context,
        &mut instantiation_signatures,
        &module.compiled_module,
    )?;

    // Process field handles and instantiations
    let field_handles = field_handles(&module.compiled_module, &structs);
    let field_instantiations = field_instantiations(&module.compiled_module, &field_handles);

    let constants = constants(&module.compiled_module)?;

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
        variant_handles: module.compiled_module.variant_handles().to_vec(),
        variant_instantiation_handles: module
            .compiled_module
            .variant_instantiation_handles()
            .to_vec(),
        constants,
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

fn structs(
    package_context: &mut PackageContext<'_>,
    module: &CompiledModule,
    type_refs: &[IntraPackageKey],
) -> PartialVMResult<Vec<StructDef>> {
    module
        .struct_defs()
        .iter()
        .map(|struct_def| {
            let key = type_refs[struct_def.struct_handle.0 as usize];
            let type_ = package_context.vtable.types.type_at(&key);
            let struct_type = type_.get_struct()?;
            let field_count = struct_type.fields.len() as u16;
            Ok(StructDef {
                field_count,
                idx: VirtualTableKey {
                    package_key: package_context.runtime_id,
                    inner_pkg_key: key,
                },
            })
        })
        .collect()
}

fn struct_instantiations(
    instantiation_signatures: &mut BTreeMap<SignatureIndex, Vec<Type>>,
    module: &CompiledModule,
    structs: &[StructDef],
) -> PartialVMResult<Vec<StructInstantiation>> {
    module
        .struct_instantiations()
        .iter()
        .map(|struct_inst| {
            let def = struct_inst.def.0 as usize;
            let struct_def = &structs[def];
            let field_count = struct_def.field_count;

            let instantiation_idx = struct_inst.type_parameters;
            cache_signatures(instantiation_signatures, module, instantiation_idx)?;

            Ok(StructInstantiation {
                field_count,
                def: struct_def.idx.clone(),
                instantiation_idx,
            })
        })
        .collect()
}

fn enums(
    package_context: &mut PackageContext<'_>,
    module: &CompiledModule,
    type_refs: &[IntraPackageKey],
) -> PartialVMResult<Vec<EnumDef>> {
    module
        .enum_defs()
        .iter()
        .map(|enum_def| {
            let key = type_refs[enum_def.enum_handle.0 as usize];
            let type_ = package_context.vtable.types.type_at(&key);
            let enum_type = type_.get_enum()?;
            let variant_count = enum_type.variants.len() as u16;
            let variants = enum_type
                .variants
                .iter()
                .enumerate()
                .map(|(tag, variant_type)| VariantDef {
                    tag: tag as u16,
                    field_count: variant_type.fields.len() as u16,
                    field_types: variant_type.fields.clone(),
                })
                .collect();
            Ok(EnumDef {
                variant_count,
                variants,
                idx: VirtualTableKey {
                    package_key: package_context.runtime_id,
                    inner_pkg_key: key,
                },
            })
        })
        .collect()
}

fn enum_instantiations(
    instantiation_signatures: &mut BTreeMap<SignatureIndex, Vec<Type>>,
    module: &CompiledModule,
    enums: &[EnumDef],
) -> PartialVMResult<Vec<EnumInstantiation>> {
    module
        .enum_instantiations()
        .iter()
        .map(|enum_inst| {
            let def = enum_inst.def.0 as usize;
            let enum_def = &enums[def];
            let variant_count_map = enum_def.variants.iter().map(|v| v.field_count).collect();
            let instantiation_idx = enum_inst.type_parameters;
            cache_signatures(instantiation_signatures, module, instantiation_idx)?;

            Ok(EnumInstantiation {
                variant_count_map,
                def: enum_def.idx.clone(),
                instantiation_idx,
            })
        })
        .collect()
}

fn constants(module: &CompiledModule) -> PartialVMResult<Vec<Constant>> {
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
            Ok(const_)
        })
        .collect()
}

fn field_handles(module: &CompiledModule, structs: &[StructDef]) -> Vec<FieldHandle> {
    module
        .field_handles()
        .iter()
        .map(|f_handle| {
            let def_idx = f_handle.owner;
            let owner = structs[def_idx.0 as usize].idx.clone();
            let offset = f_handle.field as usize;
            FieldHandle { offset, owner }
        })
        .collect()
}

fn field_instantiations(
    module: &CompiledModule,
    field_handles: &[FieldHandle],
) -> Vec<FieldInstantiation> {
    module
        .field_instantiations()
        .iter()
        .map(|f_inst| {
            let fh_idx = f_inst.handle;
            let owner = field_handles[fh_idx.0 as usize].owner.clone();
            let offset = field_handles[fh_idx.0 as usize].offset;

            FieldInstantiation { offset, owner }
        })
        .collect()
}

fn cache_signatures(
    instantiation_signatures: &mut BTreeMap<SignatureIndex, Vec<Type>>,
    module: &CompiledModule,
    instantiation_idx: SignatureIndex,
) -> PartialVMResult<()> {
    if let btree_map::Entry::Vacant(e) = instantiation_signatures.entry(instantiation_idx) {
        let instantiation = module
            .signature_at(instantiation_idx)
            .0
            .iter()
            .map(|ty| make_type(module, ty))
            .collect::<Result<Vec<_>, _>>()?;
        e.insert(instantiation);
    }
    Ok(())
}

// -------------------------------------------------------------------------------------------------
// Function Translation

fn functions(
    package_context: &mut PackageContext,
    module: &mut input::Module,
) -> PartialVMResult<BTreeMap<SignatureIndex, Type>> {
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
            alloc_function(package_context, findex, fun, module)
        })
        .collect::<PartialVMResult<Vec<_>>>()?;
    let loaded_functions = package_context
        .package_arena
        .alloc_slice(prealloc_functions.into_iter())?;

    package_context.insert_and_make_module_function_vtable(
        self_id,
        vm_pointer::mut_to_ref_slice(loaded_functions)
            .iter()
            .map(|function| {
                (
                    function.name.clone(),
                    VMPointer::new(function as *const Function),
                )
            }),
    )?;

    let mut module_context = FunctionContext {
        package_context,
        module,
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
            alloc.code = code(&mut module_context, opt_code.code)?;
        }
    }

    let FunctionContext {
        single_signature_token_map,
        ..
    } = module_context;

    Ok(single_signature_token_map)
}

fn function_instantiations(
    package_context: &mut PackageContext,
    instantiation_signatures: &mut BTreeMap<SignatureIndex, Vec<Type>>,
    module: &CompiledModule,
) -> PartialVMResult<Vec<FunctionInstantiation>> {
    dbg_println!(flag: function_list_sizes, "handle size: {}", module.function_handles().len());

    module
        .function_instantiations()
        .iter()
        .map(|func_inst| {
            let handle = call(package_context, module, func_inst.handle)?;

            let instantiation_idx = func_inst.type_parameters;
            cache_signatures(instantiation_signatures, module, instantiation_idx)?;

            Ok(FunctionInstantiation {
                handle,
                instantiation_idx,
            })
        })
        .collect()
}

fn alloc_function(
    context: &PackageContext,
    index: FunctionDefinitionIndex,
    def: &FunctionDefinition,
    module: &CompiledModule,
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
    let parameters = module
        .signature_at(handle.parameters)
        .0
        .iter()
        .map(|tok| make_type(module, tok))
        .collect::<PartialVMResult<Vec<_>>>()?;
    // Native functions do not have a code unit
    let (locals_len, locals, jump_tables) = match &def.code {
        Some(code) => (
            parameters.len() + module.signature_at(code.locals).0.len(),
            module
                .signature_at(code.locals)
                .0
                .iter()
                .map(|tok| make_type(module, tok))
                .collect::<PartialVMResult<Vec<_>>>()?,
            code.jump_tables.clone(),
        ),
        None => (0, vec![], vec![]),
    };
    let return_ = module
        .signature_at(handle.return_)
        .0
        .iter()
        .map(|tok| make_type(module, tok))
        .collect::<PartialVMResult<Vec<_>>>()?;
    let type_parameters = handle.type_parameters.clone();
    let fun = Function {
        file_format_version: module.version(),
        index,
        is_entry,
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
    blocks: BTreeMap<u16, Vec<input::Bytecode>>,
) -> PartialVMResult<*const [Bytecode]> {
    let function_bytecode = flatten_and_renumber_blocks(blocks);
    let result: *mut [Bytecode] = context.package_context.package_arena.alloc_slice(
        function_bytecode
            .iter()
            .map(|bc| bytecode(context, bc))
            .collect::<PartialVMResult<Vec<Bytecode>>>()?
            .into_iter(),
    )?;
    Ok(result as *const [Bytecode])
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
        input::Bytecode::CallGeneric(ndx) => Bytecode::CallGeneric(*ndx),

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

        input::Bytecode::LdConst(ndx) => Bytecode::LdConst(*ndx),
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
        input::Bytecode::Pack(ndx) => Bytecode::Pack(*ndx),
        input::Bytecode::PackGeneric(ndx) => Bytecode::PackGeneric(*ndx),
        input::Bytecode::Unpack(ndx) => Bytecode::Unpack(*ndx),
        input::Bytecode::UnpackGeneric(ndx) => Bytecode::UnpackGeneric(*ndx),
        input::Bytecode::MutBorrowField(ndx) => Bytecode::MutBorrowField(*ndx),
        input::Bytecode::MutBorrowFieldGeneric(ndx) => Bytecode::MutBorrowFieldGeneric(*ndx),
        input::Bytecode::ImmBorrowField(ndx) => Bytecode::ImmBorrowField(*ndx),
        input::Bytecode::ImmBorrowFieldGeneric(ndx) => Bytecode::ImmBorrowFieldGeneric(*ndx),

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
            check_vector_type(context, si)?;
            Bytecode::VecPack(*si, *size)
        }
        input::Bytecode::VecLen(si) => {
            check_vector_type(context, si)?;
            Bytecode::VecLen(*si)
        }
        input::Bytecode::VecImmBorrow(si) => {
            check_vector_type(context, si)?;
            Bytecode::VecImmBorrow(*si)
        }
        input::Bytecode::VecMutBorrow(si) => {
            check_vector_type(context, si)?;
            Bytecode::VecMutBorrow(*si)
        }
        input::Bytecode::VecPushBack(si) => {
            check_vector_type(context, si)?;
            Bytecode::VecPushBack(*si)
        }
        input::Bytecode::VecPopBack(si) => {
            check_vector_type(context, si)?;
            Bytecode::VecPopBack(*si)
        }
        input::Bytecode::VecUnpack(si, size) => {
            check_vector_type(context, si)?;
            Bytecode::VecUnpack(*si, *size)
        }
        input::Bytecode::VecSwap(si) => {
            check_vector_type(context, si)?;
            Bytecode::VecSwap(*si)
        }

        // Enums and Variants
        input::Bytecode::PackVariant(ndx) => Bytecode::PackVariant(*ndx),
        input::Bytecode::PackVariantGeneric(ndx) => Bytecode::PackVariantGeneric(*ndx),
        input::Bytecode::UnpackVariant(ndx) => Bytecode::UnpackVariant(*ndx),
        input::Bytecode::UnpackVariantImmRef(ndx) => Bytecode::UnpackVariantImmRef(*ndx),
        input::Bytecode::UnpackVariantMutRef(ndx) => Bytecode::UnpackVariantMutRef(*ndx),
        input::Bytecode::UnpackVariantGeneric(ndx) => Bytecode::UnpackVariantGeneric(*ndx),
        input::Bytecode::UnpackVariantGenericImmRef(ndx) => {
            Bytecode::UnpackVariantGenericImmRef(*ndx)
        }
        input::Bytecode::UnpackVariantGenericMutRef(ndx) => {
            Bytecode::UnpackVariantGenericMutRef(*ndx)
        }
        input::Bytecode::VariantSwitch(ndx) => Bytecode::VariantSwitch(*ndx),
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

fn check_vector_type(
    context: &mut FunctionContext,
    signature_index: &SignatureIndex,
) -> PartialVMResult<()> {
    if !context
        .single_signature_token_map
        .contains_key(signature_index)
    {
        let ty = match context.module.signature_at(*signature_index).0.first() {
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
        context
            .single_signature_token_map
            .insert(*signature_index, make_type(context.module, ty)?);
    }
    Ok(())
}
