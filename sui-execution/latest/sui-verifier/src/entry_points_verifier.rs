// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    CompiledModule,
    file_format::{Bytecode, FunctionDefinition, Visibility},
};
use move_bytecode_utils::format_signature_token;
use sui_types::{
    SUI_FRAMEWORK_ADDRESS,
    base_types::{TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME, TxContext, TxContextKind},
    error::ExecutionError,
    move_package::{FnInfoMap, is_test_fun},
};

use crate::{INIT_FN_NAME, verification_failure};

/// Checks valid rules for entry points, both for module initialization and transactions
///
/// For module initialization
/// - The existence of the function is optional
/// - The function must have the name specified by `INIT_FN_NAME`
/// - The function must have `Visibility::Private`
/// - The function can have at most two parameters:
///   - mandatory &mut TxContext or &TxContext (see `is_tx_context`) in the last position
///   - optional one-time witness type (see one_time_witness verifier pass) passed by value in the
///     first position
///
/// For transaction entry points
/// - The function must have `is_entry` true
/// - The function may have a &mut TxContext or &TxContext (see `is_tx_context`) parameter
///   - The transaction context parameter must be the last parameter
/// - The function cannot have any return values
pub fn verify_module(
    module: &CompiledModule,
    fn_info_map: &FnInfoMap,
) -> Result<(), ExecutionError> {
    // When verifying test functions, a check preventing explicit calls to init functions is
    // disabled.

    for func_def in &module.function_defs {
        let handle = module.function_handle_at(func_def.function);
        let name = module.identifier_at(handle.name);

        // allow calling init function in the test code
        if !is_test_fun(name, module, fn_info_map) {
            verify_init_not_called(module, func_def).map_err(verification_failure)?;
        }

        if name == INIT_FN_NAME {
            verify_init_function(module, func_def).map_err(verification_failure)?;
            continue;
        }
    }
    Ok(())
}

fn verify_init_not_called(
    module: &CompiledModule,
    fdef: &FunctionDefinition,
) -> Result<(), String> {
    let code = match &fdef.code {
        None => return Ok(()),
        Some(code) => code,
    };
    code.code
        .iter()
        .enumerate()
        .filter_map(|(idx, instr)| match instr {
            Bytecode::Call(fhandle_idx) => Some((idx, module.function_handle_at(*fhandle_idx))),
            Bytecode::CallGeneric(finst_idx) => {
                let finst = module.function_instantiation_at(*finst_idx);
                Some((idx, module.function_handle_at(finst.handle)))
            }
            _ => None,
        })
        .try_for_each(|(idx, fhandle)| {
            let name = module.identifier_at(fhandle.name);
            if name == INIT_FN_NAME {
                Err(format!(
                    "{}::{} at offset {}. Cannot call a module's '{}' function from another Move function",
                    module.self_id(),
                    name,
                    idx,
                    INIT_FN_NAME
                ))
            } else {
                Ok(())
            }
        })
}

/// Checks if this module has a conformant `init`
fn verify_init_function(module: &CompiledModule, fdef: &FunctionDefinition) -> Result<(), String> {
    if fdef.visibility != Visibility::Private {
        return Err(format!(
            "{}. '{}' function must be private",
            module.self_id(),
            INIT_FN_NAME
        ));
    }

    if fdef.is_entry {
        return Err(format!(
            "{}. '{}' cannot be 'entry'",
            module.self_id(),
            INIT_FN_NAME
        ));
    }

    let fhandle = module.function_handle_at(fdef.function);
    if !fhandle.type_parameters.is_empty() {
        return Err(format!(
            "{}. '{}' function cannot have type parameters",
            module.self_id(),
            INIT_FN_NAME
        ));
    }

    if !module.signature_at(fhandle.return_).is_empty() {
        return Err(format!(
            "{}, '{}' function cannot have return values",
            module.self_id(),
            INIT_FN_NAME
        ));
    }

    let parameters = &module.signature_at(fhandle.parameters).0;
    if parameters.is_empty() || parameters.len() > 2 {
        return Err(format!(
            "Expected at least one and at most two parameters for {}::{}",
            module.self_id(),
            INIT_FN_NAME,
        ));
    }

    // Checking only the last (and possibly the only) parameter here. If there are two parameters,
    // then the first parameter must be of a one-time witness type and must be passed by value. This
    // is checked by the verifier for pass one-time witness value (one_time_witness_verifier) -
    // please see the description of this pass for additional details.
    if TxContext::kind(module, &parameters[parameters.len() - 1]) != TxContextKind::None {
        Ok(())
    } else {
        Err(format!(
            "Expected last parameter for {0}::{1} to be &mut {2}::{3}::{4} or &{2}::{3}::{4}, \
            but found {5}",
            module.self_id(),
            INIT_FN_NAME,
            SUI_FRAMEWORK_ADDRESS,
            TX_CONTEXT_MODULE_NAME,
            TX_CONTEXT_STRUCT_NAME,
            format_signature_token(module, &parameters[0]),
        ))
    }
}
