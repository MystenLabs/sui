// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    file_format::{AbilitySet, Bytecode, FunctionDefinition, SignatureToken, Visibility},
    CompiledModule,
};
use move_core_types::{account_address::AccountAddress, identifier::IdentStr};
use sui_types::{
    base_types::{
        STD_ASCII_MODULE_NAME, STD_ASCII_STRUCT_NAME, STD_OPTION_MODULE_NAME,
        STD_OPTION_STRUCT_NAME, STD_UTF8_MODULE_NAME, STD_UTF8_STRUCT_NAME, TX_CONTEXT_MODULE_NAME,
        TX_CONTEXT_STRUCT_NAME,
    },
    error::ExecutionError,
    id::{ID_STRUCT_NAME, OBJECT_MODULE_NAME},
    move_package::FnInfoMap,
    MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS,
};

use crate::{
    format_signature_token, is_test_fun, resolve_struct, verification_failure, INIT_FN_NAME,
};

/// Checks valid rules rules for entry points, both for module initialization and transactions
///
/// For module initialization
/// - The existence of the function is optional
/// - The function must have the name specified by `INIT_FN_NAME`
/// - The function must have `Visibility::Private`
/// - The function can have at most two parameters:
///   - mandatory &mut TxContext or &TxContext (see `is_tx_context`) in the last position
///   - optional one-time witness type (see one_time_witness verifier pass) passed by value in the first
///   position
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

        // find candidate entry functions and check their parameters
        // (ignore other functions)
        if !func_def.is_entry {
            // it's not an entry function
            continue;
        }
        verify_entry_function_impl(module, func_def).map_err(verification_failure)?;
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
    let view = &BinaryIndexedView::Module(module);

    if fdef.visibility != Visibility::Private {
        return Err(format!(
            "{}. '{}' function must be private",
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

    if !view.signature_at(fhandle.return_).is_empty() {
        return Err(format!(
            "{}, '{}' function cannot have return values",
            module.self_id(),
            INIT_FN_NAME
        ));
    }

    let parameters = &view.signature_at(fhandle.parameters).0;
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
    if is_tx_context(view, &parameters[parameters.len() - 1]) != TxContextKind::None {
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
            format_signature_token(view, &parameters[0]),
        ))
    }
}

fn verify_entry_function_impl(
    module: &CompiledModule,
    func_def: &FunctionDefinition,
) -> Result<(), String> {
    let view = &BinaryIndexedView::Module(module);
    let handle = view.function_handle_at(func_def.function);
    let params = view.signature_at(handle.parameters);

    let all_non_ctx_params = match params.0.last() {
        Some(last_param) if is_tx_context(view, last_param) != TxContextKind::None => {
            &params.0[0..params.0.len() - 1]
        }
        _ => &params.0,
    };
    for param in all_non_ctx_params {
        verify_param_type(view, &handle.type_parameters, param)?;
    }

    let return_ = view.signature_at(handle.return_);
    if !return_.is_empty() {
        return Err(format!(
            "Entry function {} cannot have return values",
            view.identifier_at(handle.name)
        ));
    }

    Ok(())
}

fn verify_param_type(
    view: &BinaryIndexedView,
    function_type_args: &[AbilitySet],
    param: &SignatureToken,
) -> Result<(), String> {
    if is_primitive(view, function_type_args, param)
        || is_object(view, function_type_args, param)?
        || is_object_vector(view, function_type_args, param)?
    {
        Ok(())
    } else {
        Err(format!(
            "Invalid entry point parameter type. Expected primitive or object type. Got: {}",
            format_signature_token(view, param)
        ))
    }
}

pub const RESOLVED_SUI_ID: (&AccountAddress, &IdentStr, &IdentStr) =
    (&SUI_FRAMEWORK_ADDRESS, OBJECT_MODULE_NAME, ID_STRUCT_NAME);
pub const RESOLVED_STD_OPTION: (&AccountAddress, &IdentStr, &IdentStr) = (
    &MOVE_STDLIB_ADDRESS,
    STD_OPTION_MODULE_NAME,
    STD_OPTION_STRUCT_NAME,
);
pub const RESOLVED_ASCII_STR: (&AccountAddress, &IdentStr, &IdentStr) = (
    &MOVE_STDLIB_ADDRESS,
    STD_ASCII_MODULE_NAME,
    STD_ASCII_STRUCT_NAME,
);
pub const RESOLVED_UTF8_STR: (&AccountAddress, &IdentStr, &IdentStr) = (
    &MOVE_STDLIB_ADDRESS,
    STD_UTF8_MODULE_NAME,
    STD_UTF8_STRUCT_NAME,
);

fn is_primitive(
    view: &BinaryIndexedView,
    function_type_args: &[AbilitySet],
    s: &SignatureToken,
) -> bool {
    match s {
        SignatureToken::Bool
        | SignatureToken::U8
        | SignatureToken::U16
        | SignatureToken::U32
        | SignatureToken::U64
        | SignatureToken::U128
        | SignatureToken::U256
        | SignatureToken::Address => true,
        SignatureToken::Signer => false,
        // optimistic, but no primitive has key
        SignatureToken::TypeParameter(idx) => !function_type_args[*idx as usize].has_key(),

        SignatureToken::Struct(idx) => {
            let resolved_struct = resolve_struct(view, *idx);
            resolved_struct == RESOLVED_SUI_ID
                || resolved_struct == RESOLVED_ASCII_STR
                || resolved_struct == RESOLVED_UTF8_STR
        }

        SignatureToken::StructInstantiation(idx, targs) => {
            let resolved_struct = resolve_struct(view, *idx);
            // is option of a primitive
            resolved_struct == RESOLVED_STD_OPTION
                && targs.len() == 1
                && is_primitive(view, function_type_args, &targs[0])
        }

        SignatureToken::Vector(inner) => is_primitive(view, function_type_args, inner),
        SignatureToken::Reference(_) | SignatureToken::MutableReference(_) => false,
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TxContextKind {
    // No TxContext
    None,
    // &mut TxContext
    Mutable,
    // &TxContext
    Immutable,
}

// Returns Some(kind) if the type is a reference to the TxnContext. kind being Mutable with
// a MutableReference, and Immutable otherwise.
// Returns None for all other types
pub fn is_tx_context(view: &BinaryIndexedView, p: &SignatureToken) -> TxContextKind {
    match p {
        SignatureToken::MutableReference(m) | SignatureToken::Reference(m) => match &**m {
            SignatureToken::Struct(idx) => {
                let (module_addr, module_name, struct_name) = resolve_struct(view, *idx);
                let is_tx_context_type = module_name == TX_CONTEXT_MODULE_NAME
                    && module_addr == &SUI_FRAMEWORK_ADDRESS
                    && struct_name == TX_CONTEXT_STRUCT_NAME;
                if is_tx_context_type {
                    match p {
                        SignatureToken::MutableReference(_) => TxContextKind::Mutable,
                        SignatureToken::Reference(_) => TxContextKind::Immutable,
                        _ => unreachable!(),
                    }
                } else {
                    TxContextKind::None
                }
            }
            _ => TxContextKind::None,
        },
        _ => TxContextKind::None,
    }
}

pub fn is_object(
    view: &BinaryIndexedView,
    function_type_args: &[AbilitySet],
    t: &SignatureToken,
) -> Result<bool, String> {
    use SignatureToken as S;
    match t {
        S::Reference(inner) | S::MutableReference(inner) => {
            is_object(view, function_type_args, inner)
        }
        _ => is_object_struct(view, function_type_args, t),
    }
}

pub fn is_object_vector(
    view: &BinaryIndexedView,
    function_type_args: &[AbilitySet],
    t: &SignatureToken,
) -> Result<bool, String> {
    use SignatureToken as S;
    match t {
        S::Vector(inner) => is_object_struct(view, function_type_args, inner),
        _ => is_object_struct(view, function_type_args, t),
    }
}

fn is_object_struct(
    view: &BinaryIndexedView,
    function_type_args: &[AbilitySet],
    s: &SignatureToken,
) -> Result<bool, String> {
    use SignatureToken as S;
    match s {
        S::Bool
        | S::U8
        | S::U16
        | S::U32
        | S::U64
        | S::U128
        | S::U256
        | S::Address
        | S::Signer
        | S::Vector(_)
        | S::Reference(_)
        | S::MutableReference(_) => Ok(false),
        S::TypeParameter(idx) => Ok(function_type_args
            .get(*idx as usize)
            .map(|abs| abs.has_key())
            .unwrap_or(false)),
        S::Struct(_) | S::StructInstantiation(_, _) => {
            let abilities = view
                .abilities(s, function_type_args)
                .map_err(|vm_err| vm_err.to_string())?;
            Ok(abilities.has_key())
        }
    }
}
