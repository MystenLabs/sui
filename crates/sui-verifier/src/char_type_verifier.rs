// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A module can define a "characteristic type", that is a type that is instantiated only once, and
//! this property is enforced by the system. We define a characteristic type as a struct type that
//! has the same name as the module that defines it but with all the letter capitalized, and
//! possessing certain special properties specified below (please note that by convention, "regular"
//! struct type names are expressed in camel case).  In other words, if a module defines a struct
//! type whose name is the same as the module name, this type MUST possess these special properties,
//! otherwise the module definition will be considered invalid and will be rejected by the
//! validator:
//!
//! - it has only one ability: drop
//! - it has only one arbitrarily named field of type boolean (since Move structs cannot be empty)
//! - its definition does not involve type parameters
//! - its only instance in existence is passed as an argument to the module initializer
//! - it is never instantiated anywhere in its defining module
//!
//! A characteristic type is one way of implementing a one-time witness pattern, where we want to
//! restrict the number of times a given type (instance) is used. Another way could be to have a set
//! data structure that can store types and use it to guarantee uniqueness.

use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    file_format::{
        Ability, AbilitySet, Bytecode, CompiledModule, FunctionDefinition, FunctionHandle,
        SignatureToken, StructDefinition, StructHandle,
    },
};
use move_core_types::{ident_str, language_storage::ModuleId};
use sui_types::{
    base_types::{TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME},
    error::ExecutionError,
    SUI_FRAMEWORK_ADDRESS,
};

use crate::{verification_failure, INIT_FN_NAME};

pub fn verify_module(module: &CompiledModule) -> Result<(), ExecutionError> {
    // In Sui's framework code there is an exception to the characteristic type rule - we have a SUI
    // type in the sui module but it is instantiated and the module has no initializer (the reason
    // for it is that the SUI coin is only instantiated during genesis). It is easiest to simply
    // special-case this module particularly that this is framework code and thus deemed correct.
    if ModuleId::new(SUI_FRAMEWORK_ADDRESS, ident_str!("sui").to_owned()) == module.self_id() {
        return Ok(());
    }

    let view = BinaryIndexedView::Module(module);
    let mod_handle = view.module_handle_at(module.self_module_handle_idx);
    let mod_name = view.identifier_at(mod_handle.name).as_str();
    let struct_defs = &module.struct_defs;
    let mut char_type_candidate = None;
    // find structs that can potentially represent a characteristic type
    for def in struct_defs {
        let struct_handle = module.struct_handle_at(def.struct_handle);
        let struct_name = view.identifier_at(struct_handle.name).as_str();
        if mod_name.to_ascii_uppercase() == struct_name {
            verify_char_type(module, struct_name, struct_handle, def)
                .map_err(verification_failure)?;
            // if we reached this point, it means we have a legitimate characteristic type candidate
            // and we have to make sure that both the init function's signature reflects this and
            // that this type is not instantiated in any function of the module
            char_type_candidate = Some((struct_name, struct_handle, def));
            break; // no reason to look any further
        }
    }
    for fn_def in &module.function_defs {
        let fn_handle = module.function_handle_at(fn_def.function);
        let fn_name = module.identifier_at(fn_handle.name);
        if fn_name == INIT_FN_NAME {
            if let Some((candidate_name, candidate_handle, _)) = char_type_candidate {
                // only verify if init function conforms to characteristic types requirements if we
                // have a characteristic type candidate
                verify_init_function_char_type(module, fn_handle, candidate_name, candidate_handle)
                    .map_err(verification_failure)?;
            } else {
                // if there is no characteristic type candidate than the init function should have
                // only one parameter of TxContext type
                verify_init_function_single_param(module, fn_handle)
                    .map_err(verification_failure)?;
            }
        }
        if let Some((candidate_name, _, def)) = char_type_candidate {
            // only verify lack of characteristic types instantiations if we have a
            // characteristic type candidate
            verify_no_instantiations(module, fn_def, candidate_name, def)
                .map_err(verification_failure)?;
        }
    }

    Ok(())
}

// Verifies all required properties of a characteristic type candidate (that is a type whose name is
// the same as the name of a
fn verify_char_type(
    module: &CompiledModule,
    candidate_name: &str,
    candidate_handle: &StructHandle,
    candidate_def: &StructDefinition,
) -> Result<(), String> {
    // must have only one ability: drop
    let drop_set = AbilitySet::EMPTY | Ability::Drop;
    let abilities = candidate_handle.abilities;
    if abilities != drop_set {
        return Err(format!(
            "characteristic type candidate {}::{} must have a single ability: drop",
            module.self_id(),
            candidate_name,
        ));
    }
    let field_count = candidate_def.declared_field_count().map_err(|_| {
        format!(
            "characteristic type candidate {}::{} cannot be a native structure",
            module.self_id(),
            candidate_name
        )
    })?;

    // unwrap below is safe as it will always be successful if declared_field_count call above is
    // successful
    if field_count != 1 || candidate_def.field(0).unwrap().signature.0 != SignatureToken::Bool {
        return Err(format!(
            "characteristic type candidate {}::{} must have a single bool field only (or no fields)",
            module.self_id(),
            candidate_name,
        ));
    }

    if !candidate_handle.type_parameters.is_empty() {
        return Err(format!(
            "characteristic type candidate {}::{} cannot have type parameters",
            module.self_id(),
            candidate_name,
        ));
    }
    Ok(())
}

/// Checks if this module's `init` function conformant with the characteristic type
fn verify_init_function_char_type(
    module: &CompiledModule,
    fn_handle: &FunctionHandle,
    candidate_name: &str,
    candidate_handle: &StructHandle,
) -> Result<(), String> {
    let view = &BinaryIndexedView::Module(module);
    let fn_sig = view.signature_at(fn_handle.parameters);
    if fn_sig.len() != 2 || !is_char_type(view, &fn_sig.0[0], candidate_handle) {
        // check only the first parameter - the other one is checked in entry_points verification
        // pass
        return Err(format!(
            "init function of a module containing characteristic type candidate must have {}::{} as the first parameter",
            module.self_id(),
            candidate_name,
        ));
    }

    Ok(())
}

// Checks if a given SignatureToken represents a characteristic type struct
fn is_char_type(
    view: &BinaryIndexedView,
    tok: &SignatureToken,
    candidate_handle: &StructHandle,
) -> bool {
    matches!(tok, SignatureToken::Struct(idx) if view.struct_handle_at(*idx) == candidate_handle)
}

/// Checks if this module's `init` function has a single parameter of TxContext type only
fn verify_init_function_single_param(
    module: &CompiledModule,
    fn_handle: &FunctionHandle,
) -> Result<(), String> {
    let view = &BinaryIndexedView::Module(module);
    let fn_sig = view.signature_at(fn_handle.parameters);
    if fn_sig.len() != 1 {
        return Err(format!(
            "Expected exactly one parameter for {}::{}  of type &mut {}::{}::{}",
            module.self_id(),
            INIT_FN_NAME,
            SUI_FRAMEWORK_ADDRESS,
            TX_CONTEXT_MODULE_NAME,
            TX_CONTEXT_STRUCT_NAME,
        ));
    }

    Ok(())
}

/// Checks if this module function does not contain instantiation of the characteristic type
fn verify_no_instantiations(
    module: &CompiledModule,
    fn_def: &FunctionDefinition,
    struct_name: &str,
    struct_def: &StructDefinition,
) -> Result<(), String> {
    let view = &BinaryIndexedView::Module(module);
    if fn_def.code.is_none() {
        return Ok(());
    }
    for bcode in &fn_def.code.as_ref().unwrap().code {
        let struct_def_idx = match bcode {
            Bytecode::Pack(idx) => idx,
            _ => continue,
        };
        // unwrap is safe below since we know we are getting a struct out of a module (see
        // definition of struct_def_at)
        if view.struct_def_at(*struct_def_idx).unwrap() == struct_def {
            let fn_handle = module.function_handle_at(fn_def.function);
            let fn_name = module.identifier_at(fn_handle.name);
            return Err(format!(
                "characteristic type {}::{} is instantiated \
                         in the {}::{} function and must never be",
                module.self_id(),
                struct_name,
                module.self_id(),
                fn_name,
            ));
        }
    }

    Ok(())
}
