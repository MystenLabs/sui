// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::{
        arena::{self, Arena, ArenaPointer},
        type_cache::{self, CrossVersionPackageCache},
    },
    dbg_println,
    execution::values::Value,
    jit::execution::ast::*,
    natives::functions::NativeFunctions,
    shared::{
        binary_cache::BinaryCache,
        linkage_context::LinkageContext,
        types::{PackageStorageId, RuntimePackageId},
    },
    string_interner,
    validation::verification,
};
use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        self as FF, CompiledModule, EnumDefinitionIndex, FunctionDefinition,
        FunctionDefinitionIndex, FunctionHandleIndex, SignatureIndex, StructDefinitionIndex,
        StructFieldInformation, TableIndex,
    },
};
use move_core_types::{identifier::Identifier, language_storage::ModuleId, vm_status::StatusCode};
use parking_lot::RwLock;
use std::{
    collections::{btree_map, BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

struct ModuleContext<'a, 'natives> {
    package_context: &'a PackageContext<'natives>,
    module: &'a CompiledModule,
    single_signature_token_map: BTreeMap<SignatureIndex, Type>,
}

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
    pub vtable: PackageVTable,
}

impl PackageContext<'_> {
    fn insert_and_make_module_function_vtable(
        &mut self,
        module_name: Identifier,
        vtable: impl IntoIterator<Item = (Identifier, ArenaPointer<Function>)>,
    ) -> PartialVMResult<()> {
        let string_interner = string_interner();
        let module_name = string_interner.get_or_intern_identifier(&module_name)?;
        for (name, func) in vtable {
            let member_name = string_interner.get_or_intern_identifier(&name)?;
            self.vtable.functions.insert(
                IntraPackageKey {
                    module_name,
                    member_name,
                },
                func,
            )?;
        }
        Ok(())
    }

    fn try_resolve_function(&self, vtable_entry: &VTableKey) -> Option<ArenaPointer<Function>> {
        self.vtable
            .functions
            .get(&vtable_entry.inner_pkg_key)
            .map(|f| ArenaPointer::new(f.to_ref()))
    }
}

pub fn package(
    package_cache: Arc<RwLock<CrossVersionPackageCache>>,
    natives: &NativeFunctions,
    link_context: &LinkageContext,
    verified_package: verification::ast::Package,
) -> PartialVMResult<Package> {
    let storage_id = verified_package.storage_id;
    let runtime_id = verified_package.runtime_id;
    let (module_ids_in_pkg, mut package_modules): (BTreeSet<_>, Vec<_>) =
        verified_package.modules.into_iter().unzip();

    let mut package_context = PackageContext {
        natives,
        storage_id,
        runtime_id,
        loaded_modules: BinaryCache::new(),
        compiled_modules: BinaryCache::new(),
        package_arena: Arena::new(),
        vtable: PackageVTable::new(package_cache),
        type_origin_table: verified_package
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
            .collect::<PartialVMResult<_>>()?,
    };

    // Load modules in dependency order within the package. Needed for both static call
    // resolution and type caching.
    while let Some(input_module) = package_modules.pop() {
        let mut immediate_dependencies = input_module
            .value
            .immediate_dependencies()
            .into_iter()
            .filter(|dep| module_ids_in_pkg.contains(dep) && dep != &input_module.value.self_id());

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
            &input_module.value,
        )?;

        package_context
            .loaded_modules
            .insert(loaded_module.id.name().to_owned(), loaded_module)?;
        package_context.compiled_modules.insert(
            input_module.value.self_id().name().to_owned(),
            input_module.value,
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

fn module(
    package_context: &mut PackageContext<'_>,
    link_context: &LinkageContext,
    package_id: PackageStorageId,
    module: &CompiledModule,
) -> Result<Module, PartialVMError> {
    let self_id = module.self_id();
    dbg_println!("Loading module: {}", self_id);

    load_module_types(
        package_context,
        link_context,
        package_context.runtime_id,
        package_id,
        module,
    )?;

    dbg_println!("Module types loaded");

    let mut instantiation_signatures: BTreeMap<SignatureIndex, Vec<Type>> = BTreeMap::new();
    // helper to build the sparse signature vector
    fn cache_signatures(
        instantiation_signatures: &mut BTreeMap<SignatureIndex, Vec<Type>>,
        module: &CompiledModule,
        instantiation_idx: SignatureIndex,
    ) -> Result<(), PartialVMError> {
        if let btree_map::Entry::Vacant(e) = instantiation_signatures.entry(instantiation_idx) {
            let instantiation = module
                .signature_at(instantiation_idx)
                .0
                .iter()
                .map(|ty| type_cache::make_type(module, ty))
                .collect::<Result<Vec<_>, _>>()?;
            e.insert(instantiation);
        }
        Ok(())
    }

    let mut type_refs = vec![];
    let mut structs = vec![];
    let mut struct_instantiations = vec![];
    let mut enums = vec![];
    let mut enum_instantiations = vec![];
    let mut function_instantiations = vec![];
    let mut field_handles = vec![];
    let mut field_instantiations: Vec<FieldInstantiation> = vec![];
    let mut constants = vec![];

    for datatype_handle in module.datatype_handles() {
        let struct_name = string_interner()
            .get_or_intern_ident_str(module.identifier_at(datatype_handle.name))?;
        let module_handle = module.module_handle_at(datatype_handle.module);
        let runtime_id = module.module_id_for_handle(module_handle);
        let module_name = string_interner().get_or_intern_ident_str(runtime_id.name())?;
        type_refs.push(IntraPackageKey {
            module_name,
            member_name: struct_name.to_owned(),
        });
    }

    for struct_def in module.struct_defs() {
        let idx = type_refs[struct_def.struct_handle.0 as usize].clone();
        let field_count = package_context
            .vtable
            .types
            .read()
            .type_at(&idx)
            .get_struct()?
            .fields
            .len() as u16;
        structs.push(StructDef {
            field_count,
            idx: VTableKey {
                package_key: package_context.runtime_id,
                inner_pkg_key: IntraPackageKey {
                    module_name: idx.module_name,
                    member_name: idx.member_name,
                },
            },
        });
    }

    for struct_inst in module.struct_instantiations() {
        let def = struct_inst.def.0 as usize;
        let struct_def = &structs[def];
        let field_count = struct_def.field_count;

        let instantiation_idx = struct_inst.type_parameters;
        cache_signatures(&mut instantiation_signatures, module, instantiation_idx)?;
        struct_instantiations.push(StructInstantiation {
            field_count,
            def: struct_def.idx.clone(),
            instantiation_idx,
        });
    }

    for enum_def in module.enum_defs() {
        let idx = type_refs[enum_def.enum_handle.0 as usize].clone();
        let datatype = &package_context.vtable.types.read().type_at(&idx);
        let enum_type = datatype.get_enum()?;
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
        enums.push(EnumDef {
            variant_count,
            variants,
            idx: VTableKey {
                package_key: package_context.runtime_id,
                inner_pkg_key: IntraPackageKey {
                    module_name: idx.module_name,
                    member_name: idx.member_name,
                },
            },
        });
    }

    for enum_inst in module.enum_instantiations() {
        let def = enum_inst.def.0 as usize;
        let enum_def = &enums[def];
        let variant_count_map = enum_def.variants.iter().map(|v| v.field_count).collect();
        let instantiation_idx = enum_inst.type_parameters;
        cache_signatures(&mut instantiation_signatures, module, instantiation_idx)?;

        enum_instantiations.push(EnumInstantiation {
            variant_count_map,
            def: enum_def.idx.clone(),
            instantiation_idx,
        });
    }

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
        .alloc_slice(prealloc_functions.into_iter());

    package_context.insert_and_make_module_function_vtable(
        self_id.name().to_owned(),
        arena::mut_to_ref_slice(loaded_functions)
            .iter()
            .map(|function| {
                (
                    function.name.clone(),
                    ArenaPointer::new(function as *const Function),
                )
            }),
    )?;

    dbg_println!(flag: function_list_sizes, "handle size: {}", module.function_handles().len());

    let single_signature_token_map = BTreeMap::new();
    let mut context = ModuleContext {
        package_context,
        module,
        single_signature_token_map,
    };

    for (alloc, fun) in arena::to_mut_ref_slice(loaded_functions)
        .iter_mut()
        .zip(module.function_defs())
    {
        if let Some(code_unit) = &fun.code {
            alloc.code = code(&mut context, &code_unit.code)?;
        }
    }

    for func_inst in context.module.function_instantiations() {
        let handle = call(&mut context, func_inst.handle)?;

        let instantiation_idx = func_inst.type_parameters;
        cache_signatures(&mut instantiation_signatures, module, instantiation_idx)?;

        function_instantiations.push(FunctionInstantiation {
            handle,
            instantiation_idx,
        });
    }

    for f_handle in module.field_handles() {
        let def_idx = f_handle.owner;
        let owner = structs[def_idx.0 as usize].idx.clone();
        let offset = f_handle.field as usize;
        field_handles.push(FieldHandle { offset, owner });
    }

    for f_inst in module.field_instantiations() {
        let fh_idx = f_inst.handle;
        let owner = field_handles[fh_idx.0 as usize].owner.clone();
        let offset = field_handles[fh_idx.0 as usize].offset;

        field_instantiations.push(FieldInstantiation { offset, owner });
    }

    for constant in module.constant_pool() {
        let value = Value::deserialize_constant(constant)
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::VERIFIER_INVARIANT_VIOLATION).with_message(
                    "Verifier failed to verify the deserialization of constants".to_owned(),
                )
            })?
            .to_constant_value()?;
        let type_ = type_cache::make_type(context.module, &constant.type_)?;
        let size = constant.data.len() as u64;
        let const_ = Constant { value, type_, size };
        constants.push(const_);
    }

    let ModuleContext {
        package_context: _,
        module,
        single_signature_token_map,
    } = context;

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
        // TODO: Remove this field
        function_map: HashMap::new(),
        single_signature_token_map,
        instantiation_signatures,
        variant_handles: module.variant_handles().to_vec(),
        variant_instantiation_handles: module.variant_instantiation_handles().to_vec(),
        constants,
    })
}

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
            .read()
            .contains_cached_type(&struct_key)
        {
            continue;
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
            .map(|f| type_cache::make_type(module, &f.signature.0))
            .collect::<PartialVMResult<Vec<Type>>>()?;

        package_context.vtable.types.write().cache_datatype(
            struct_key.clone(),
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

        if package_context
            .vtable
            .types
            .read()
            .contains_cached_type(&enum_key)
        {
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
                        .map(|f| type_cache::make_type(module, &f.signature.0))
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

        package_context.vtable.types.write().cache_datatype(
            enum_key.clone(),
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
        .map(|tok| type_cache::make_type(&module, tok))
        .collect::<PartialVMResult<Vec<_>>>()?;
    // Native functions do not have a code unit
    let (locals_len, jump_tables) = match &def.code {
        Some(code) => (
            parameters.len() + module.signature_at(code.locals).0.len(),
            code.jump_tables.clone(),
        ),
        None => (0, vec![]),
    };
    let return_ = module
        .signature_at(handle.return_)
        .0
        .iter()
        .map(|tok| type_cache::make_type(&module, tok))
        .collect::<PartialVMResult<Vec<_>>>()?;
    let type_parameters = handle.type_parameters.clone();
    let fun = Function {
        file_format_version: module.version(),
        index,
        is_entry,
        code: arena::null_ptr(),
        parameters,
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

fn code(context: &mut ModuleContext, code: &[FF::Bytecode]) -> PartialVMResult<*const [Bytecode]> {
    let result: *mut [Bytecode] = context.package_context.package_arena.alloc_slice(
        code.iter()
            .map(|bc| bytecode(context, bc))
            .collect::<PartialVMResult<Vec<Bytecode>>>()?
            .into_iter(),
    );
    Ok(result as *const [Bytecode])
}

fn bytecode(context: &mut ModuleContext, bytecode: &FF::Bytecode) -> PartialVMResult<Bytecode> {
    let bytecode = match bytecode {
        // Calls -- these get compiled to something more-direct here
        FF::Bytecode::Call(ndx) => {
            let call_type = call(context, *ndx)?;
            match call_type {
                CallType::Direct(func) => Bytecode::DirectCall(func),
                CallType::Virtual(vtable) => Bytecode::VirtualCall(vtable),
            }
        }

        // For now, generic calls retain an index so we can look up their signature as well.
        FF::Bytecode::CallGeneric(ndx) => Bytecode::CallGeneric(*ndx),

        // Standard Codes
        FF::Bytecode::Pop => Bytecode::Pop,
        FF::Bytecode::Ret => Bytecode::Ret,
        FF::Bytecode::BrTrue(n) => Bytecode::BrTrue(*n),
        FF::Bytecode::BrFalse(n) => Bytecode::BrFalse(*n),
        FF::Bytecode::Branch(n) => Bytecode::Branch(*n),

        FF::Bytecode::LdU256(n) => Bytecode::LdU256(n.clone()),
        FF::Bytecode::LdU128(n) => Bytecode::LdU128(n.clone()),
        FF::Bytecode::LdU16(n) => Bytecode::LdU16(*n),
        FF::Bytecode::LdU32(n) => Bytecode::LdU32(*n),
        FF::Bytecode::LdU64(n) => Bytecode::LdU64(*n),
        FF::Bytecode::LdU8(n) => Bytecode::LdU8(*n),

        FF::Bytecode::LdConst(ndx) => Bytecode::LdConst(*ndx),
        FF::Bytecode::LdTrue => Bytecode::LdTrue,
        FF::Bytecode::LdFalse => Bytecode::LdFalse,

        FF::Bytecode::CopyLoc(ndx) => Bytecode::CopyLoc(*ndx),
        FF::Bytecode::MoveLoc(ndx) => Bytecode::MoveLoc(*ndx),
        FF::Bytecode::StLoc(ndx) => Bytecode::StLoc(*ndx),
        FF::Bytecode::Pack(ndx) => Bytecode::Pack(*ndx),
        FF::Bytecode::PackGeneric(ndx) => Bytecode::PackGeneric(*ndx),
        FF::Bytecode::Unpack(ndx) => Bytecode::Unpack(*ndx),
        FF::Bytecode::UnpackGeneric(ndx) => Bytecode::UnpackGeneric(*ndx),
        FF::Bytecode::ReadRef => Bytecode::ReadRef,
        FF::Bytecode::WriteRef => Bytecode::WriteRef,
        FF::Bytecode::FreezeRef => Bytecode::FreezeRef,
        FF::Bytecode::MutBorrowLoc(ndx) => Bytecode::MutBorrowLoc(*ndx),
        FF::Bytecode::ImmBorrowLoc(ndx) => Bytecode::ImmBorrowLoc(*ndx),
        FF::Bytecode::MutBorrowField(ndx) => Bytecode::MutBorrowField(*ndx),
        FF::Bytecode::MutBorrowFieldGeneric(ndx) => Bytecode::MutBorrowFieldGeneric(*ndx),
        FF::Bytecode::ImmBorrowField(ndx) => Bytecode::ImmBorrowField(*ndx),
        FF::Bytecode::ImmBorrowFieldGeneric(ndx) => Bytecode::ImmBorrowFieldGeneric(*ndx),

        FF::Bytecode::Add => Bytecode::Add,
        FF::Bytecode::Sub => Bytecode::Sub,
        FF::Bytecode::Mul => Bytecode::Mul,
        FF::Bytecode::Mod => Bytecode::Mod,
        FF::Bytecode::Div => Bytecode::Div,
        FF::Bytecode::BitOr => Bytecode::BitOr,
        FF::Bytecode::BitAnd => Bytecode::BitAnd,
        FF::Bytecode::Xor => Bytecode::Xor,
        FF::Bytecode::Or => Bytecode::Or,
        FF::Bytecode::And => Bytecode::And,
        FF::Bytecode::Not => Bytecode::Not,
        FF::Bytecode::Eq => Bytecode::Eq,
        FF::Bytecode::Neq => Bytecode::Neq,
        FF::Bytecode::Lt => Bytecode::Lt,
        FF::Bytecode::Gt => Bytecode::Gt,
        FF::Bytecode::Le => Bytecode::Le,
        FF::Bytecode::Ge => Bytecode::Ge,
        FF::Bytecode::Abort => Bytecode::Abort,
        FF::Bytecode::Nop => Bytecode::Nop,
        FF::Bytecode::Shl => Bytecode::Shl,
        FF::Bytecode::Shr => Bytecode::Shr,

        FF::Bytecode::CastU256 => Bytecode::CastU256,
        FF::Bytecode::CastU128 => Bytecode::CastU128,
        FF::Bytecode::CastU16 => Bytecode::CastU16,
        FF::Bytecode::CastU32 => Bytecode::CastU32,
        FF::Bytecode::CastU64 => Bytecode::CastU64,
        FF::Bytecode::CastU8 => Bytecode::CastU8,

        // Vectors
        FF::Bytecode::VecPack(si, size) => {
            check_vector_type(context, si)?;
            Bytecode::VecPack(*si, *size)
        }
        FF::Bytecode::VecLen(si) => {
            check_vector_type(context, si)?;
            Bytecode::VecLen(*si)
        }
        FF::Bytecode::VecImmBorrow(si) => {
            check_vector_type(context, si)?;
            Bytecode::VecImmBorrow(*si)
        }
        FF::Bytecode::VecMutBorrow(si) => {
            check_vector_type(context, si)?;
            Bytecode::VecMutBorrow(*si)
        }
        FF::Bytecode::VecPushBack(si) => {
            check_vector_type(context, si)?;
            Bytecode::VecPushBack(*si)
        }
        FF::Bytecode::VecPopBack(si) => {
            check_vector_type(context, si)?;
            Bytecode::VecPopBack(*si)
        }
        FF::Bytecode::VecUnpack(si, size) => {
            check_vector_type(context, si)?;
            Bytecode::VecUnpack(*si, *size)
        }
        FF::Bytecode::VecSwap(si) => {
            check_vector_type(context, si)?;
            Bytecode::VecSwap(*si)
        }
        // Structs and Fields

        // Enums and Variants
        FF::Bytecode::PackVariant(ndx) => Bytecode::PackVariant(*ndx),
        FF::Bytecode::PackVariantGeneric(ndx) => Bytecode::PackVariantGeneric(*ndx),
        FF::Bytecode::UnpackVariant(ndx) => Bytecode::UnpackVariant(*ndx),
        FF::Bytecode::UnpackVariantImmRef(ndx) => Bytecode::UnpackVariantImmRef(*ndx),
        FF::Bytecode::UnpackVariantMutRef(ndx) => Bytecode::UnpackVariantMutRef(*ndx),
        FF::Bytecode::UnpackVariantGeneric(ndx) => Bytecode::UnpackVariantGeneric(*ndx),
        FF::Bytecode::UnpackVariantGenericImmRef(ndx) => Bytecode::UnpackVariantGenericImmRef(*ndx),
        FF::Bytecode::UnpackVariantGenericMutRef(ndx) => Bytecode::UnpackVariantGenericMutRef(*ndx),
        FF::Bytecode::VariantSwitch(ndx) => Bytecode::VariantSwitch(*ndx),

        // Deprecated bytecodes -- bail
        FF::Bytecode::ExistsDeprecated(_)
        | FF::Bytecode::ExistsGenericDeprecated(_)
        | FF::Bytecode::MoveFromDeprecated(_)
        | FF::Bytecode::MoveFromGenericDeprecated(_)
        | FF::Bytecode::MoveToDeprecated(_)
        | FF::Bytecode::MoveToGenericDeprecated(_)
        | FF::Bytecode::MutBorrowGlobalDeprecated(_)
        | FF::Bytecode::MutBorrowGlobalGenericDeprecated(_)
        | FF::Bytecode::ImmBorrowGlobalDeprecated(_)
        | FF::Bytecode::ImmBorrowGlobalGenericDeprecated(_) => {
            unreachable!("Global bytecodes deprecated")
        }
    };
    Ok(bytecode)
}

fn call(
    context: &mut ModuleContext,
    function_handle_index: FunctionHandleIndex,
) -> PartialVMResult<CallType> {
    let string_interner = string_interner();

    let func_handle = context.module.function_handle_at(function_handle_index);
    let member_name =
        string_interner.get_or_intern_ident_str(context.module.identifier_at(func_handle.name))?;
    let module_handle = context.module.module_handle_at(func_handle.module);
    let runtime_id = context.module.module_id_for_handle(module_handle);
    let module_name = string_interner.get_or_intern_ident_str(runtime_id.name())?;
    let vtable_key = VTableKey {
        package_key: *runtime_id.address(),
        inner_pkg_key: IntraPackageKey {
            module_name,
            member_name,
        },
    };
    dbg_println!(flag: function_resolution, "Resolving function: {:?}", vtable_key);
    Ok(
        match context.package_context.try_resolve_function(&vtable_key) {
            Some(func) => CallType::Direct(func),
            None => CallType::Virtual(vtable_key),
        },
    )
}

fn check_vector_type(
    context: &mut ModuleContext,
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
            .insert(*signature_index, type_cache::make_type(context.module, ty)?);
    }
    Ok(())
}
