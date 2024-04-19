// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A module can define a one-time witness type, that is a type that is instantiated only once, and
//! this property is enforced by the system. We define a one-time witness type as a struct type that
//! has the same name as the module that defines it but with all the letters capitalized, and
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
use move_binary_format::file_format::{
    Ability, AbilitySet, Bytecode, CompiledModule, DatatypeHandle, FunctionDefinition,
    FunctionHandle, SignatureToken, StructDefinition,
};
use move_core_types::{ident_str, language_storage::ModuleId};
use sui_types::{
    base_types::{TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME},
    error::ExecutionError,
    move_package::{is_test_fun, FnInfoMap},
    SUI_FRAMEWORK_ADDRESS,
};

use crate::{verification_failure, INIT_FN_NAME};

pub fn verify_module(
    module: &CompiledModule,
    fn_info_map: &FnInfoMap,
) -> Result<(), ExecutionError> {
    // When verifying test functions, a check preventing by-hand instantiation of one-time withess
    // is disabled

    // In Sui's framework code there is an exception to the one-time witness type rule - we have a
    // SUI type in the sui module but it is instantiated outside of the module initializer (in fact,
    // the module has no initializer). The reason for it is that the SUI coin is only instantiated
    // during genesis. It is easiest to simply special-case this module particularly that this is
    // framework code and thus deemed correct.
    if ModuleId::new(SUI_FRAMEWORK_ADDRESS, ident_str!("sui").to_owned()) == module.self_id() {
        return Ok(());
    }

    let mod_handle = module.module_handle_at(module.self_module_handle_idx);
    let mod_name = module.identifier_at(mod_handle.name).as_str();
    let struct_defs = &module.struct_defs;
    let mut one_time_witness_candidate = None;
    // find structs that can potentially represent a one-time witness type
    for def in struct_defs {
        let struct_handle = module.datatype_handle_at(def.struct_handle);
        let struct_name = module.identifier_at(struct_handle.name).as_str();
        if mod_name.to_ascii_uppercase() == struct_name {
            // one-time witness candidate's type name must be the same as capitalized module name
            if let Ok(field_count) = def.declared_field_count() {
                // checks if the struct is non-native (and if it isn't then that's why unwrap below
                // is safe)
                if field_count == 1 && def.field(0).unwrap().signature.0 == SignatureToken::Bool {
                    // a single boolean field means that we found a one-time witness candidate -
                    // make sure that the remaining properties hold
                    verify_one_time_witness(module, struct_name, struct_handle)
                        .map_err(verification_failure)?;
                    // if we reached this point, it means we have a legitimate one-time witness type
                    // candidate and we have to make sure that both the init function's signature
                    // reflects this and that this type is not instantiated in any function of the
                    // module
                    one_time_witness_candidate = Some((struct_name, struct_handle, def));
                    break; // no reason to look any further
                }
            }
        }
    }
    for fn_def in &module.function_defs {
        let fn_handle = module.function_handle_at(fn_def.function);
        let fn_name = module.identifier_at(fn_handle.name);
        if fn_name == INIT_FN_NAME {
            if let Some((candidate_name, candidate_handle, _)) = one_time_witness_candidate {
                // only verify if init function conforms to one-time witness type requirements if we
                // have a one-time witness type candidate
                verify_init_one_time_witness(module, fn_handle, candidate_name, candidate_handle)
                    .map_err(verification_failure)?;
            } else {
                // if there is no one-time witness type candidate than the init function should have
                // only one parameter of TxContext type
                verify_init_single_param(module, fn_handle).map_err(verification_failure)?;
            }
        }
        if let Some((candidate_name, _, def)) = one_time_witness_candidate {
            // only verify lack of one-time witness type instantiations if we have a one-time
            // witness type candidate and if instantiation does not happen in test code

            if !is_test_fun(fn_name, module, fn_info_map) {
                verify_no_instantiations(module, fn_def, candidate_name, def)
                    .map_err(verification_failure)?;
            }
        }
    }

    Ok(())
}

// Verifies all required properties of a one-time witness type candidate (that is a type whose name
// is the same as the name of a module but capitalized)
fn verify_one_time_witness(
    module: &CompiledModule,
    candidate_name: &str,
    candidate_handle: &DatatypeHandle,
) -> Result<(), String> {
    // must have only one ability: drop
    let drop_set = AbilitySet::EMPTY | Ability::Drop;
    let abilities = candidate_handle.abilities;
    if abilities != drop_set {
        return Err(format!(
            "one-time witness type candidate {}::{} must have a single ability: drop",
            module.self_id(),
            candidate_name,
        ));
    }

    if !candidate_handle.type_parameters.is_empty() {
        return Err(format!(
            "one-time witness type candidate {}::{} cannot have type parameters",
            module.self_id(),
            candidate_name,
        ));
    }
    Ok(())
}

/// Checks if this module's `init` function conformant with the one-time witness type
fn verify_init_one_time_witness(
    module: &CompiledModule,
    fn_handle: &FunctionHandle,
    candidate_name: &str,
    candidate_handle: &DatatypeHandle,
) -> Result<(), String> {
    let fn_sig = module.signature_at(fn_handle.parameters);
    if fn_sig.len() != 2 || !is_one_time_witness(module, &fn_sig.0[0], candidate_handle) {
        // check only the first parameter - the other one is checked in entry_points verification
        // pass
        return Err(format!(
            "init function of a module containing one-time witness type candidate must have \
             {}::{} as the first parameter (a struct which has no fields or a single field of type \
             bool)",
            module.self_id(),
            candidate_name,
        ));
    }

    Ok(())
}

// Checks if a given SignatureToken represents a one-time witness type struct
fn is_one_time_witness(
    view: &CompiledModule,
    tok: &SignatureToken,
    candidate_handle: &DatatypeHandle,
) -> bool {
    matches!(tok, SignatureToken::Datatype(idx) if view.datatype_handle_at(*idx) == candidate_handle)
}

/// Checks if this module's `init` function has a single parameter of TxContext type only
fn verify_init_single_param(
    module: &CompiledModule,
    fn_handle: &FunctionHandle,
) -> Result<(), String> {
    let fn_sig = module.signature_at(fn_handle.parameters);
    if fn_sig.len() != 1 {
        return Err(format!(
            "Expected last (and at most second) parameter for {0}::{1} to be &mut {2}::{3}::{4} or \
             &{2}::{3}::{4}; optional first parameter must be of one-time witness type whose name \
             is the same as the capitalized module name ({5}::{6}) and which has no fields or a \
             single field of type bool",
            module.self_id(),
            INIT_FN_NAME,
            SUI_FRAMEWORK_ADDRESS,
            TX_CONTEXT_MODULE_NAME,
            TX_CONTEXT_STRUCT_NAME,
            module.self_id(),
            module.self_id().name().as_str().to_uppercase(),
        ));
    }

    Ok(())
}

/// Checks if this module function does not contain instantiation of the one-time witness type
fn verify_no_instantiations(
    module: &CompiledModule,
    fn_def: &FunctionDefinition,
    struct_name: &str,
    struct_def: &StructDefinition,
) -> Result<(), String> {
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
        if module.struct_def_at(*struct_def_idx) == struct_def {
            let fn_handle = module.function_handle_at(fn_def.function);
            let fn_name = module.identifier_at(fn_handle.name);
            return Err(format!(
                "one-time witness type {}::{} is instantiated \
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
