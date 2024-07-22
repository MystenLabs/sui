// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    loader::{ast::*, CacheCursor, ModuleCache},
    native_functions::NativeFunctions,
};
use move_binary_format::{
    errors::PartialVMError,
    file_format::{
        Bytecode, CompiledModule, FunctionDefinition, FunctionDefinitionIndex, SignatureIndex,
    },
};
use move_core_types::{
    account_address::AccountAddress, language_storage::ModuleId, vm_status::StatusCode,
};
use move_vm_types::loaded_data::runtime_types::Type;
use std::collections::{btree_map, BTreeMap, HashMap};

pub fn module(
    cursor: &CacheCursor,
    link_context: AccountAddress,
    storage_id: ModuleId,
    module: &CompiledModule,
    cache: &ModuleCache,
) -> Result<LoadedModule, PartialVMError> {
    let self_id = module.self_id();

    let mut instantiation_signatures: BTreeMap<SignatureIndex, Vec<Type>> = BTreeMap::new();
    // helper to build the sparse signature vector
    fn cache_signatures(
        instantiation_signatures: &mut BTreeMap<SignatureIndex, Vec<Type>>,
        module: &CompiledModule,
        instantiation_idx: SignatureIndex,
        cache: &ModuleCache,
    ) -> Result<(), PartialVMError> {
        if let btree_map::Entry::Vacant(e) = instantiation_signatures.entry(instantiation_idx) {
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

    let mut type_refs = vec![];
    let mut structs = vec![];
    let mut struct_instantiations = vec![];
    let mut enums = vec![];
    let mut enum_instantiations = vec![];
    let mut function_refs = vec![];
    let mut function_instantiations = vec![];
    let mut field_handles = vec![];
    let mut field_instantiations: Vec<FieldInstantiation> = vec![];
    let mut function_map = HashMap::new();
    let mut single_signature_token_map = BTreeMap::new();

    for datatype_handle in module.datatype_handles() {
        let struct_name = module.identifier_at(datatype_handle.name);
        let module_handle = module.module_handle_at(datatype_handle.module);
        let runtime_id = module.module_id_for_handle(module_handle);
        type_refs.push(cache.resolve_type_by_name(struct_name, &runtime_id)?.0);
    }

    for struct_def in module.struct_defs() {
        let idx = type_refs[struct_def.struct_handle.0 as usize];
        let field_count = cache.datatypes.binaries[idx.0].get_struct()?.fields.len() as u16;
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

    for enum_def in module.enum_defs() {
        let idx = type_refs[enum_def.enum_handle.0 as usize];
        let enum_type = cache.datatypes.binaries[idx.0].get_enum()?;
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
            idx,
        });
    }

    for enum_inst in module.enum_instantiations() {
        let def = enum_inst.def.0 as usize;
        let enum_def = &enums[def];
        let variant_count_map = enum_def.variants.iter().map(|v| v.field_count).collect();
        let instantiation_idx = enum_inst.type_parameters;
        cache_signatures(
            &mut instantiation_signatures,
            module,
            instantiation_idx,
            cache,
        )?;

        enum_instantiations.push(EnumInstantiation {
            variant_count_map,
            def: enum_def.idx,
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
                            single_signature_token_map.insert(*si, cache.make_type(module, ty)?);
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

    Ok(LoadedModule {
        id: storage_id,
        type_refs,
        structs,
        struct_instantiations,
        enums,
        enum_instantiations,
        function_refs,
        function_instantiations,
        field_handles,
        field_instantiations,
        function_map,
        single_signature_token_map,
        instantiation_signatures,
        variant_handles: module.variant_handles().to_vec(),
        variant_instantiation_handles: module.variant_instantiation_handles().to_vec(),
    })
}

pub fn function(
    natives: &NativeFunctions,
    index: FunctionDefinitionIndex,
    def: &FunctionDefinition,
    module: &CompiledModule,
) -> Function {
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
    let (code, locals_len, jump_tables) = match &def.code {
        Some(code) => (
            code.code.clone(),
            parameters_len + module.signature_at(code.locals).0.len(),
            code.jump_tables.clone(),
        ),
        None => (vec![], 0, vec![]),
    };
    let return_ = handle.return_;
    let return_len = module.signature_at(return_).0.len();
    let type_parameters = handle.type_parameters.clone();
    Function {
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
        jump_tables,
    }
}
