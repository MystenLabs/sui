// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;

use move_binary_format::{
    CompiledModule,
    file_format::{Bytecode, FunctionDefinition, FunctionHandle, SignatureToken, Visibility},
};
use move_bytecode_utils::format_signature_token;
use move_core_types::{account_address::AccountAddress, ident_str, identifier::IdentStr};
use move_vm_config::verifier::VerifierConfig;
use sui_types::{
    MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS, error::ExecutionError, make_invariant_violation,
};

use crate::{FunctionIdent, TEST_SCENARIO_MODULE_NAME, verification_failure};

pub const TRANSFER_MODULE: &IdentStr = ident_str!("transfer");
pub const EVENT_MODULE: &IdentStr = ident_str!("event");
pub const COIN_REGISTRY_MODULE: &IdentStr = ident_str!("coin_registry");

// Event function
pub const SUI_EVENT_EMIT_EVENT: FunctionIdent =
    (SUI_FRAMEWORK_ADDRESS, EVENT_MODULE, ident_str!("emit"));
pub const SUI_EVENT_EMIT_AUTHENTICATED: FunctionIdent = (
    SUI_FRAMEWORK_ADDRESS,
    EVENT_MODULE,
    ident_str!("emit_authenticated"),
);
pub const SUI_EVENT_NUM_EVENTS: FunctionIdent = (
    SUI_FRAMEWORK_ADDRESS,
    EVENT_MODULE,
    ident_str!("num_events"),
);
pub const SUI_EVENT_EVENTS_BY_TYPE: FunctionIdent = (
    SUI_FRAMEWORK_ADDRESS,
    EVENT_MODULE,
    ident_str!("events_by_type"),
);

// Public transfer functions
pub const SUI_TRANSFER_PUBLIC_TRANSFER: FunctionIdent = (
    SUI_FRAMEWORK_ADDRESS,
    TRANSFER_MODULE,
    ident_str!("public_transfer"),
);
pub const SUI_TRANSFER_PUBLIC_FREEZE_OBJECT: FunctionIdent = (
    SUI_FRAMEWORK_ADDRESS,
    TRANSFER_MODULE,
    ident_str!("public_freeze_object"),
);
pub const SUI_TRANSFER_PUBLIC_SHARE_OBJECT: FunctionIdent = (
    SUI_FRAMEWORK_ADDRESS,
    TRANSFER_MODULE,
    ident_str!("public_share_object"),
);
pub const SUI_TRANSFER_PUBLIC_RECEIVE: FunctionIdent = (
    SUI_FRAMEWORK_ADDRESS,
    TRANSFER_MODULE,
    ident_str!("public_receive"),
);
pub const SUI_TRANSFER_RECEIVING_OBJECT_ID: FunctionIdent = (
    SUI_FRAMEWORK_ADDRESS,
    TRANSFER_MODULE,
    ident_str!("receiving_object_id"),
);
pub const SUI_TRANSFER_PUBLIC_PARTY_TRANSFER: FunctionIdent = (
    SUI_FRAMEWORK_ADDRESS,
    TRANSFER_MODULE,
    ident_str!("public_party_transfer"),
);

// Private transfer functions
pub const SUI_TRANSFER_TRANSFER: FunctionIdent = (
    SUI_FRAMEWORK_ADDRESS,
    TRANSFER_MODULE,
    ident_str!("transfer"),
);
pub const SUI_TRANSFER_FREEZE_OBJECT: FunctionIdent = (
    SUI_FRAMEWORK_ADDRESS,
    TRANSFER_MODULE,
    ident_str!("freeze_object"),
);
pub const SUI_TRANSFER_SHARE_OBJECT: FunctionIdent = (
    SUI_FRAMEWORK_ADDRESS,
    TRANSFER_MODULE,
    ident_str!("share_object"),
);
pub const SUI_TRANSFER_RECEIVE: FunctionIdent = (
    SUI_FRAMEWORK_ADDRESS,
    TRANSFER_MODULE,
    ident_str!("receive"),
);
pub const SUI_TRANSFER_PARTY_TRANSFER: FunctionIdent = (
    SUI_FRAMEWORK_ADDRESS,
    TRANSFER_MODULE,
    ident_str!("party_transfer"),
);

// Coin registry functions
pub const SUI_COIN_REGISTRY_NEW_CURRENCY: FunctionIdent = (
    SUI_FRAMEWORK_ADDRESS,
    COIN_REGISTRY_MODULE,
    ident_str!("new_currency"),
);

// Modules that must have all public functions listed in `FUNCTIONS_TO_CHECK`
pub const EXHAUSTIVE_MODULES: &[(AccountAddress, &IdentStr)] = &[
    (SUI_FRAMEWORK_ADDRESS, EVENT_MODULE),
    (SUI_FRAMEWORK_ADDRESS, TRANSFER_MODULE),
];

// A list of all functions to check for internal rules. A boolean for each type parameter indicates
// if the type parameter is `internal`
pub const FUNCTIONS_TO_CHECK: &[(FunctionIdent, &[/* is internal */ bool])] = &[
    // event functions
    (SUI_EVENT_EMIT_EVENT, &[true]),
    (SUI_EVENT_EMIT_AUTHENTICATED, &[true]),
    (SUI_EVENT_NUM_EVENTS, &[]),
    (SUI_EVENT_EVENTS_BY_TYPE, &[false]),
    // public transfer functions
    (SUI_TRANSFER_PUBLIC_TRANSFER, &[false]),
    (SUI_TRANSFER_PUBLIC_FREEZE_OBJECT, &[false]),
    (SUI_TRANSFER_PUBLIC_SHARE_OBJECT, &[false]),
    (SUI_TRANSFER_PUBLIC_RECEIVE, &[false]),
    (SUI_TRANSFER_RECEIVING_OBJECT_ID, &[false]),
    (SUI_TRANSFER_PUBLIC_PARTY_TRANSFER, &[false]),
    // private transfer functions
    (SUI_TRANSFER_TRANSFER, &[true]),
    (SUI_TRANSFER_FREEZE_OBJECT, &[true]),
    (SUI_TRANSFER_SHARE_OBJECT, &[true]),
    (SUI_TRANSFER_RECEIVE, &[true]),
    (SUI_TRANSFER_PARTY_TRANSFER, &[true]),
    // coin registry functions
    (SUI_COIN_REGISTRY_NEW_CURRENCY, &[true]),
];

enum Error {
    User(String),
    InvariantViolation(String),
}

/// Several functions in the Sui Framework have `internal` type parameters, whose arguments must be
/// instantiated with types defined in the caller's module.
/// For example, with `transfer::transfer<T>(...)` `T` must be a type declared in the current
/// module. Otherwise, `transfer::public_transfer<T>(...)` can be used without restriction, as long
/// as `T` has `store`. Note thought that the ability constraint is not checked in this verifier,
/// but rather in the normal bytecode verifier type checking.
/// To avoid, issues, all `su::transfer` and `sui::event` functions must be configured in `INTERNAL_FUNCTIONS`.
pub fn verify_module(
    module: &CompiledModule,
    _verifier_config: &VerifierConfig,
) -> Result<(), ExecutionError> {
    let module_id = module.self_id();
    let module_address = *module_id.address();
    let module_name = module_id.name();

    // Skip sui::test_scenario
    if module_address == SUI_FRAMEWORK_ADDRESS && module_name.as_str() == TEST_SCENARIO_MODULE_NAME
    {
        // exclude test_module which is a test-only module in the Sui framework which "emulates"
        // transactional execution and needs to allow test code to bypass private generics
        return Ok(());
    };

    // Check exhaustiveness for sensitive modules
    if EXHAUSTIVE_MODULES.contains(&(module_address, module_name)) {
        for fdef in module
            .function_defs
            .iter()
            .filter(|fdef| fdef.visibility == Visibility::Public)
        {
            let function_name = module.identifier_at(module.function_handle_at(fdef.function).name);
            let resolved = &(module_address, module_name, function_name);
            let rules_opt = FUNCTIONS_TO_CHECK.iter().find(|(f, _)| f == resolved);
            if rules_opt.is_none() {
                // The function needs to be added to the FUNCTIONS_TO_CHECK list
                return Err(make_invariant_violation!(
                    "Unknown function '{module_id}::{function_name}'. \
                    All functions in '{module_id}' must be listed in FUNCTIONS_TO_CHECK",
                ));
            }
        }
    }

    // Check calls
    for func_def in &module.function_defs {
        verify_function(module, func_def).map_err(|error| match error {
            Error::User(error) => verification_failure(format!(
                "{}::{}. {}",
                module.self_id(),
                module.identifier_at(module.function_handle_at(func_def.function).name),
                error
            )),
            Error::InvariantViolation(error) => {
                make_invariant_violation!(
                    "{}::{}. {}",
                    module.self_id(),
                    module.identifier_at(module.function_handle_at(func_def.function).name),
                    error
                )
            }
        })?;
    }
    Ok(())
}

fn verify_function(module: &CompiledModule, fdef: &FunctionDefinition) -> Result<(), Error> {
    let code = match &fdef.code {
        None => return Ok(()),
        Some(code) => code,
    };
    for instr in &code.code {
        let (callee, ty_args): (FunctionIdent<'_>, &[SignatureToken]) = match instr {
            Bytecode::Call(fhandle_idx) => {
                let fhandle = module.function_handle_at(*fhandle_idx);
                (resolve_function(module, fhandle), &[])
            }
            Bytecode::CallGeneric(finst_idx) => {
                let finst = module.function_instantiation_at(*finst_idx);
                let fhandle = module.function_handle_at(finst.handle);
                let type_arguments = &module.signature_at(finst.type_parameters).0;
                (resolve_function(module, fhandle), type_arguments)
            }
            _ => continue,
        };
        verify_call(module, callee, ty_args)?;
    }
    Ok(())
}

fn verify_call(
    module: &CompiledModule,
    callee @ (callee_addr, callee_module, callee_function): FunctionIdent<'_>,
    ty_args: &[SignatureToken],
) -> Result<(), Error> {
    let Some((_, internal_flags)) = FUNCTIONS_TO_CHECK.iter().find(|(f, _)| &callee == f) else {
        return Ok(());
    };
    let internal_flags = *internal_flags;
    if ty_args.len() != internal_flags.len() {
        // This should have been caught by the bytecode verifier
        return Err(Error::InvariantViolation(format!(
            "'{callee_addr}::{callee_module}::{callee_function}' \
            expects {} type arguments found {}",
            internal_flags.len(),
            ty_args.len()
        )));
    }
    for (idx, (ty_arg, &is_internal)) in ty_args.iter().zip(internal_flags).enumerate() {
        if !is_internal {
            continue;
        }
        if !is_defined_in_current_module(module, ty_arg) {
            let callee_package_name = callee_package_name(&callee_addr);
            let help = help_message(&callee_addr, callee_module, callee_function);
            return Err(Error::User(format!(
                "Invalid call to '{callee_package_name}::{callee_module}::{callee_function}'. \
                Type argument #{idx} must be a type defined in the current module, found '{}'.\
                {help}",
                format_signature_token(module, ty_arg),
            )));
        }
    }

    Ok(())
}

fn resolve_function<'a>(
    module: &'a CompiledModule,
    callee_handle: &FunctionHandle,
) -> FunctionIdent<'a> {
    let mh = module.module_handle_at(callee_handle.module);
    let a = *module.address_identifier_at(mh.address);
    let m = module.identifier_at(mh.name);
    let f = module.identifier_at(callee_handle.name);
    (a, m, f)
}

fn is_defined_in_current_module(module: &CompiledModule, type_arg: &SignatureToken) -> bool {
    match type_arg {
        SignatureToken::Datatype(_) | SignatureToken::DatatypeInstantiation(_) => {
            let idx = match type_arg {
                SignatureToken::Datatype(idx) => *idx,
                SignatureToken::DatatypeInstantiation(s) => s.0,
                _ => unreachable!(),
            };
            let shandle = module.datatype_handle_at(idx);
            module.self_handle_idx() == shandle.module
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

pub fn callee_package_name(callee_addr: &AccountAddress) -> Cow<'static, str> {
    match *callee_addr {
        SUI_FRAMEWORK_ADDRESS => Cow::Borrowed("sui"),
        MOVE_STDLIB_ADDRESS => Cow::Borrowed("std"),
        a => {
            debug_assert!(
                false,
                "unknown package in private generics verifier. \
                Please improve this error message"
            );
            Cow::Owned(format!("{a}"))
        }
    }
}

pub fn help_message(
    callee_addr: &AccountAddress,
    callee_module: &IdentStr,
    callee_function: &IdentStr,
) -> String {
    if *callee_addr == SUI_FRAMEWORK_ADDRESS && callee_module == TRANSFER_MODULE {
        format!(
            " If the type has the 'store' ability, use the public variant instead: 'sui::transfer::public_{}'.",
            callee_function
        )
    } else {
        String::new()
    }
}
