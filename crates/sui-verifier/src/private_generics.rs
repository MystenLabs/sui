// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    file_format::{
        AbilitySet, Bytecode, FunctionDefinition, FunctionHandle, FunctionInstantiation,
        ModuleHandle, SignatureToken,
    },
    CompiledModule,
};
use move_core_types::{account_address::AccountAddress, identifier::IdentStr};
use sui_types::{error::ExecutionError, SUI_FRAMEWORK_ADDRESS};

use crate::{format_signature_token, verification_failure};

const TEST_SCENARIO_MODULE_NAME: &str = "test_scenario";

/// All transfer functions (the functions in `sui::transfer`) are "private" in that they are
/// restricted to the module.
/// For example, with `transfer::transfer<T>(...)`, either:
/// - `T` must be a type declared in the current module or
/// - `T` must have `store`
///
/// Similarly, `event::emit` is also "private" to the module. Unlike the `transfer` functions, there
/// is no relaxation for `store`
/// Concretely, with `event::emit<T>(...)`:
/// - `T` must be a type declared in the current module
pub fn verify_module(module: &CompiledModule) -> Result<(), ExecutionError> {
    if *module.address() == SUI_FRAMEWORK_ADDRESS
        && module.name() == IdentStr::new(TEST_SCENARIO_MODULE_NAME).unwrap()
    {
        // exclude test_module which is a test-only module in the Sui framework which "emulates"
        // transactional execution and needs to allow test code to bypass private generics
        return Ok(());
    }
    let view = &BinaryIndexedView::Module(module);
    // do not need to check the sui::transfer module itself
    for func_def in &module.function_defs {
        verify_function(view, func_def).map_err(|error| {
            verification_failure(format!(
                "{}::{}. {}",
                module.self_id(),
                module.identifier_at(module.function_handle_at(func_def.function).name),
                error
            ))
        })?;
    }
    Ok(())
}

fn verify_function(view: &BinaryIndexedView, fdef: &FunctionDefinition) -> Result<(), String> {
    let code = match &fdef.code {
        None => return Ok(()),
        Some(code) => code,
    };
    let function_type_parameters = &view.function_handle_at(fdef.function).type_parameters;
    for instr in &code.code {
        if let Bytecode::CallGeneric(finst_idx) = instr {
            let FunctionInstantiation {
                handle,
                type_parameters,
            } = view.function_instantiation_at(*finst_idx);

            let fhandle = view.function_handle_at(*handle);
            let mhandle = view.module_handle_at(fhandle.module);

            let type_arguments = &view.signature_at(*type_parameters).0;
            match addr_module(view, mhandle) {
                (SUI_FRAMEWORK_ADDRESS, "transfer") => verify_private_transfer(
                    view,
                    function_type_parameters,
                    fhandle,
                    type_arguments,
                )?,
                (SUI_FRAMEWORK_ADDRESS, "event") => {
                    verify_private_event_emit(view, fhandle, type_arguments)?
                }
                _ => (),
            }
        }
    }
    Ok(())
}

fn verify_private_transfer(
    view: &BinaryIndexedView,
    function_type_parameters: &[AbilitySet],
    fhandle: &FunctionHandle,
    type_arguments: &[SignatureToken],
) -> Result<(), String> {
    let self_handle = view.module_handle_at(view.self_handle_idx().unwrap());
    if addr_module(view, self_handle) == (SUI_FRAMEWORK_ADDRESS, "transfer") {
        return Ok(());
    }
    let fident = view.identifier_at(fhandle.name);
    match fident.as_str() {
        // transfer functions
        "transfer" | "freeze_object" | "share_object" => (),
        // should be unreachable
        // these are private and the module itself is skipped
        "transfer_internal" => {
            debug_assert!(false, "internal error. Unexpected private function");
            return Ok(());
        }
        // unknown function, so a bug in the implementation here
        s => {
            debug_assert!(false, "unknown transfer function {}", s);
            return Ok(());
        }
    };
    for type_arg in type_arguments {
        let has_store = view
            .abilities(type_arg, function_type_parameters)
            .map_err(|vm_err| vm_err.to_string())?
            .has_store();
        if !has_store && !is_defined_in_current_module(view, type_arg) {
            return Err(format!(
                "Invalid call to '{}::transfer::{}' on an object of type '{}'. \
                The transferred object's type must be defined in the current module, \
                or must have the 'store' type ability",
                SUI_FRAMEWORK_ADDRESS,
                fident,
                format_signature_token(view, type_arg),
            ));
        }
    }
    Ok(())
}

fn verify_private_event_emit(
    view: &BinaryIndexedView,
    fhandle: &FunctionHandle,
    type_arguments: &[SignatureToken],
) -> Result<(), String> {
    let fident = view.identifier_at(fhandle.name);
    match fident.as_str() {
        // transfer functions
        "emit" => (),
        // unknown function, so a bug in the implementation here
        s => {
            debug_assert!(false, "unknown transfer function {}", s);
            return Ok(());
        }
    };
    for type_arg in type_arguments {
        if !is_defined_in_current_module(view, type_arg) {
            return Err(format!(
                "Invalid call to '{}::event::{}' with an event type '{}'. \
                The event's type must be defined in the current module",
                SUI_FRAMEWORK_ADDRESS,
                fident,
                format_signature_token(view, type_arg),
            ));
        }
    }
    Ok(())
}

fn is_defined_in_current_module(view: &BinaryIndexedView, type_arg: &SignatureToken) -> bool {
    match type_arg {
        SignatureToken::Struct(idx) | SignatureToken::StructInstantiation(idx, _) => {
            let shandle = view.struct_handle_at(*idx);
            view.self_handle_idx() == Some(shandle.module)
        }
        SignatureToken::TypeParameter(_)
        | SignatureToken::Bool
        | SignatureToken::U8
        | SignatureToken::U16
        | SignatureToken::U32
        | SignatureToken::U64
        | SignatureToken::U128
        | SignatureToken::U256
        | SignatureToken::Address
        | SignatureToken::Vector(_)
        | SignatureToken::Signer
        | SignatureToken::Reference(_)
        | SignatureToken::MutableReference(_) => false,
    }
}

fn addr_module<'a>(
    view: &'a BinaryIndexedView,
    mhandle: &ModuleHandle,
) -> (AccountAddress, &'a str) {
    let maddr = view.address_identifier_at(mhandle.address);
    let mident = view.identifier_at(mhandle.name);
    (*maddr, mident.as_str())
}
