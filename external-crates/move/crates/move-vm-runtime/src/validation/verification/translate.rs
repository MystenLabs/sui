// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    natives::functions::NativeFunctions, validation::deserialization::ast as Input,
    validation::verification::ast,
};

use move_binary_format::{
    errors::{verification_error, Location, PartialVMResult, VMResult},
    file_format::{StructFieldInformation, TableIndex},
    CompiledModule, IndexKind,
};
use move_bytecode_verifier::script_signature;
use move_core_types::vm_status::StatusCode;
use move_vm_config::runtime::VMConfig;

use std::collections::BTreeMap;

struct Context<'natives, 'config> {
    natives: &'natives NativeFunctions,
    vm_config: &'config VMConfig,
}

pub(crate) fn package(
    natives: &NativeFunctions,
    vm_config: &VMConfig,
    pkg: Input::Package,
) -> VMResult<ast::Package> {
    let Input::Package {
        original_id,
        modules: in_modules,
        version_id,
        type_origin_table,
        linkage_table,
    } = pkg;
    let context = Context { natives, vm_config };
    let mut modules = BTreeMap::new();
    for (module_id, d_module) in in_modules {
        modules.insert(module_id, module(&context, d_module)?);
    }
    Ok(ast::Package {
        original_id,
        modules,
        version_id,
        type_origin_table,
        linkage_table,
    })
}

fn module(context: &Context, m: CompiledModule) -> VMResult<ast::Module> {
    // bytecode verifier checks that can be performed with the module itself
    // TODO: Charge gas?
    move_bytecode_verifier::verify_module_with_config_unmetered(&context.vm_config.verifier, &m)?;
    // We do this here to avoid needing to do it during VM runs.
    for function in m.function_defs() {
        let handle = m.function_handle_at(function.function);
        let name = m.identifier_at(handle.name);
        script_signature::verify_module_function_signature_by_name(
            &m,
            name,
            move_bytecode_verifier::no_additional_script_signature_checks,
        )?;
    }
    check_natives(context, &m)?;
    Ok(ast::Module { value: m })
}

// All native functions must be known to the loader at load time.
fn check_natives(context: &Context, in_module: &CompiledModule) -> VMResult<()> {
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
    check_natives_impl(context.natives, in_module)
        .map_err(|e| e.finish(Location::Module(in_module.self_id())))
}
