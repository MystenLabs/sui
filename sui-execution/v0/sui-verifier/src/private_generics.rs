// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    file_format::{
        Bytecode, FunctionDefinition, FunctionHandle, FunctionInstantiation, ModuleHandle,
        SignatureToken,
    },
    CompiledModule,
};
use move_bytecode_utils::format_signature_token;
use move_core_types::{account_address::AccountAddress, ident_str, identifier::IdentStr};
use sui_types::{error::ExecutionError, SUI_FRAMEWORK_ADDRESS};

use crate::{verification_failure, TEST_SCENARIO_MODULE_NAME};

pub const TRANSFER_MODULE: &IdentStr = ident_str!("transfer");
pub const EVENT_MODULE: &IdentStr = ident_str!("event");
pub const EVENT_FUNCTION: &IdentStr = ident_str!("emit");
pub const PUBLIC_TRANSFER_FUNCTIONS: &[&IdentStr] = &[
    ident_str!("public_transfer"),
    ident_str!("public_freeze_object"),
    ident_str!("public_share_object"),
];
pub const PRIVATE_TRANSFER_FUNCTIONS: &[&IdentStr] = &[
    ident_str!("transfer"),
    ident_str!("freeze_object"),
    ident_str!("share_object"),
];
pub const TRANSFER_IMPL_FUNCTIONS: &[&IdentStr] = &[
    ident_str!("transfer_impl"),
    ident_str!("freeze_object_impl"),
    ident_str!("share_object_impl"),
];

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
    for instr in &code.code {
        if let Bytecode::CallGeneric(finst_idx) = instr {
            let FunctionInstantiation {
                handle,
                type_parameters,
            } = view.function_instantiation_at(*finst_idx);

            let fhandle = view.function_handle_at(*handle);
            let mhandle = view.module_handle_at(fhandle.module);

            let type_arguments = &view.signature_at(*type_parameters).0;
            let ident = addr_module(view, mhandle);
            if ident == (SUI_FRAMEWORK_ADDRESS, TRANSFER_MODULE) {
                verify_private_transfer(view, fhandle, type_arguments)?
            } else if ident == (SUI_FRAMEWORK_ADDRESS, EVENT_MODULE) {
                verify_private_event_emit(view, fhandle, type_arguments)?
            }
        }
    }
    Ok(())
}

fn verify_private_transfer(
    view: &BinaryIndexedView,
    fhandle: &FunctionHandle,
    type_arguments: &[SignatureToken],
) -> Result<(), String> {
    let self_handle = view.module_handle_at(view.self_handle_idx().unwrap());
    if addr_module(view, self_handle) == (SUI_FRAMEWORK_ADDRESS, TRANSFER_MODULE) {
        return Ok(());
    }
    let fident = view.identifier_at(fhandle.name);
    // public transfer functions require `store` and have no additional rules
    if PUBLIC_TRANSFER_FUNCTIONS.contains(&fident) {
        return Ok(());
    }
    if !PRIVATE_TRANSFER_FUNCTIONS.contains(&fident) {
        // unknown function, so a bug in the implementation here
        debug_assert!(false, "unknown transfer function {}", fident);
        return Err(format!("Calling unknown transfer function, {}", fident));
    };

    if type_arguments.len() != 1 {
        debug_assert!(false, "Expected 1 type argument for {}", fident);
        return Err(format!("Expected 1 type argument for {}", fident));
    }

    let type_arg = &type_arguments[0];
    if !is_defined_in_current_module(view, type_arg) {
        return Err(format!(
            "Invalid call to '{sui}::transfer::{f}' on an object of type '{t}'. \
            The transferred object's type must be defined in the current module. \
            If the object has the 'store' type ability, you can use the non-internal variant \
            instead, i.e. '{sui}::transfer::public_{f}'",
            sui = SUI_FRAMEWORK_ADDRESS,
            f = fident,
            t = format_signature_token(view, type_arg),
        ));
    }

    Ok(())
}

fn verify_private_event_emit(
    view: &BinaryIndexedView,
    fhandle: &FunctionHandle,
    type_arguments: &[SignatureToken],
) -> Result<(), String> {
    let fident = view.identifier_at(fhandle.name);
    if fident != EVENT_FUNCTION {
        debug_assert!(false, "unknown transfer function {}", fident);
        return Err(format!("Calling unknown event function, {}", fident));
    };

    if type_arguments.len() != 1 {
        debug_assert!(false, "Expected 1 type argument for {}", fident);
        return Err(format!("Expected 1 type argument for {}", fident));
    }

    let type_arg = &type_arguments[0];
    if !is_defined_in_current_module(view, type_arg) {
        return Err(format!(
            "Invalid call to '{}::event::{}' with an event type '{}'. \
                The event's type must be defined in the current module",
            SUI_FRAMEWORK_ADDRESS,
            fident,
            format_signature_token(view, type_arg),
        ));
    }

    Ok(())
}

fn is_defined_in_current_module(view: &BinaryIndexedView, type_arg: &SignatureToken) -> bool {
    match type_arg {
        SignatureToken::Datatype(idx) | SignatureToken::DatatypeInstantiation(idx, _) => {
            let shandle = view.datatype_handle_at(*idx);
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
) -> (AccountAddress, &'a IdentStr) {
    let maddr = view.address_identifier_at(mhandle.address);
    let mident = view.identifier_at(mhandle.name);
    (*maddr, mident)
}
