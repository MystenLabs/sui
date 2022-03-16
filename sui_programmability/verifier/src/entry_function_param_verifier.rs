// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    binary_views::BinaryIndexedView,
    file_format::{Ability, FunctionHandle, Signature, SignatureToken, Visibility},
    CompiledModule,
};
use sui_types::{
    base_types::{TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME},
    error::{SuiError, SuiResult},
    SUI_FRAMEWORK_ADDRESS,
};

pub fn verify_module(module: &CompiledModule) -> SuiResult {
    check_params(module)
}
/// Checks if parameters of functions that can become entry
/// functions (functions called directly from Sui) have correct types.
///
/// We first identify functions that can be entry functions by looking
/// for functions with the following properties:
/// 1. Public
/// 2. Primitive return types (or vector of such up to 2 levels of nesting)
/// 3. Parameter order: objects, primitives, &mut TxContext
///
/// Note that this can be ambiguous in presence of the following
/// templated parameters:
/// - param: T
/// - param: vector<T> // and nested vectors
///
/// A function is considered an entry function if such templated
/// arguments are part of "object parameters" only.
///
/// In order for the parameter types of an entry function to be
/// correct, all generic types used in templated arguments mentioned
/// above must have the `key` ability.
pub fn check_params(module: &CompiledModule) -> SuiResult {
    let view = BinaryIndexedView::Module(module);
    for func_def in module.function_defs.iter() {
        // find candidate entry functions and checke their parameters
        // (ignore other functions)
        if func_def.visibility != Visibility::Public {
            // it's not an entry function as a non-public function
            // cannot be called from Sui
            continue;
        }
        let handle = view.function_handle_at(func_def.function);

        if view
            .signature_at(handle.return_)
            .0
            .iter()
            .any(|ret_type| !is_entry_ret_type(ret_type))
        {
            // it's not an entry function as it returns a value of
            // types unsupported in entry functions
            continue;
        }

        let params = view.signature_at(handle.parameters);
        let param_num = match is_entry_candidate(&view, params) {
            Some(v) => v,
            None => continue,
        };
        // iterate over all object params and make sure that each
        // template-typed (either by itself or in a vector) argument
        // has the Key ability
        for (pos, p) in params.0[0..param_num].iter().enumerate() {
            if let SignatureToken::TypeParameter(_) = p {
                if !is_template_param_ok(handle, p) {
                    return Err(SuiError::ModuleVerificationFailure {
                        error: format!(
                            "No `key` ability for the template parameter at position {} in entry function {}",
                            pos,
                            view.identifier_at(handle.name)
                        ),
                    });
                }
            }
            if let SignatureToken::Vector(_) = p {
                if !is_template_vector_param_ok(handle, p) {
                    return Err(SuiError::ModuleVerificationFailure {
                        error: format!(
                            "No `key` ability for the template vector parameter at position {} in entry function {}",
                            pos,
                            view.identifier_at(handle.name)
                        ),
                    });
                }
            }
        }
    }
    Ok(())
}

/// Checks if a function can possibly be an entry function (without
/// checking correctness of the function's parameters).
fn is_entry_candidate(view: &BinaryIndexedView, params: &Signature) -> Option<usize> {
    let mut obj_params_num = 0;
    if params.is_empty() {
        // must have at least on &mut TxContext param
        return None;
    }
    let last_param = params.0.get(params.len() - 1).unwrap();
    if !is_tx_context(view, last_param) {
        return None;
    }
    if params.len() == 1 {
        // only one &mut TxContext param
        return Some(obj_params_num);
    }
    // currently, an entry function has object parameters followed by
    // primitive type parameters (followed by the &mut TxContext
    // param, but we already checked this one)
    let mut primitive_params_phase = false; // becomes true once we start seeing primitive type params
    for p in &params.0[0..params.len() - 1] {
        if is_primitive(p) {
            primitive_params_phase = true;
        } else {
            obj_params_num += 1;
            if primitive_params_phase {
                // We encounter a non primitive type parameter after
                // the first one was encountered. This cannot be an
                // entry function as it would get rejected by the
                // resolve_and_type_check function in the adapter upon
                // its call attempt from Sui
                return None;
            }
            if !is_object(view, p)
                && !is_template(p)
                && !is_object_vector(view, p)
                && !is_template_vector(p)
            {
                // A non-primitive type for entry functions must be an
                // object, or a generic type, or a vector (possibly
                // nested) of objects, or a templeted vector (possibly
                // nested). Otherwise it is not an entry function as
                // we cannot pass non-object types from Sui).
                return None;
            }
        }
    }
    Some(obj_params_num)
}

/// Checks if a given parameter is of a primitive type. It's a mirror
/// of the is_primitive function in the adapter module that operates
/// on Type-s.
fn is_primitive(p: &SignatureToken) -> bool {
    // nested vectors of primitive types are OK to arbitrary nesting
    // level
    is_primitive_internal(p, 0, u32::MAX)
}

// Checks if a given type is the correct entry function return type. It's a mirror
/// of the is_entry_ret_type function in the adapter module that
/// operates on Type-s.
pub fn is_entry_ret_type(t: &SignatureToken) -> bool {
    // allow vectors of vectors but no deeper nesting
    is_primitive_internal(t, 0, 2)
}

fn is_primitive_internal(p: &SignatureToken, depth: u32, max_depth: u32) -> bool {
    use SignatureToken::*;
    match p {
        Bool | U8 | U64 | U128 | Address => true,
        Vector(t) => {
            if depth < max_depth {
                is_primitive_internal(t, depth + 1, max_depth)
            } else {
                false
            }
        }
        Signer
        | Struct(_)
        | StructInstantiation(..)
        | Reference(_)
        | MutableReference(_)
        | TypeParameter(_) => false,
    }
}

fn is_object(view: &BinaryIndexedView, p: &SignatureToken) -> bool {
    use SignatureToken::*;
    match p {
        Struct(idx) => view
            .struct_handle_at(*idx)
            .abilities
            .has_ability(Ability::Key),
        StructInstantiation(idx, _) => view
            .struct_handle_at(*idx)
            .abilities
            .has_ability(Ability::Key),
        Reference(t) => is_object(view, t),
        MutableReference(t) => is_object(view, t),
        _ => false,
    }
}

fn is_template(p: &SignatureToken) -> bool {
    matches!(p, SignatureToken::TypeParameter(_))
}

fn is_object_vector(view: &BinaryIndexedView, p: &SignatureToken) -> bool {
    if let SignatureToken::Vector(t) = p {
        match &**t {
            SignatureToken::Vector(inner_t) => return is_object_vector(view, inner_t),
            other => return is_object(view, other),
        }
    }
    false
}

fn is_template_vector(p: &SignatureToken) -> bool {
    match p {
        SignatureToken::Vector(t) => is_template_vector(t),
        other => matches!(other, SignatureToken::TypeParameter(_)),
    }
}

fn is_template_param_ok(handle: &FunctionHandle, p: &SignatureToken) -> bool {
    if let SignatureToken::TypeParameter(idx) = p {
        if !handle
            .type_parameters
            .get(*idx as usize)
            .unwrap()
            .has_ability(Ability::Key)
        {
            return false;
        }
    }
    true
}

fn is_template_vector_param_ok(handle: &FunctionHandle, p: &SignatureToken) -> bool {
    match p {
        SignatureToken::Vector(t) => is_template_vector_param_ok(handle, t),
        SignatureToken::TypeParameter(_) => is_template_param_ok(handle, p),
        _ => true,
    }
}

/// It's a mirror of the is_param_tx_context function in the adapter
/// module that operates on Type-s.
fn is_tx_context(view: &BinaryIndexedView, p: &SignatureToken) -> bool {
    match p {
        SignatureToken::MutableReference(m) => match &**m {
            SignatureToken::Struct(idx) => {
                let struct_handle = view.struct_handle_at(*idx);
                let struct_name = view.identifier_at(struct_handle.name);
                let module = view.module_handle_at(struct_handle.module);
                let module_name = view.identifier_at(module.name);
                let module_addr = view.address_identifier_at(module.address);
                module_name == TX_CONTEXT_MODULE_NAME
                    && module_addr == &SUI_FRAMEWORK_ADDRESS
                    && struct_name == TX_CONTEXT_STRUCT_NAME
            }
            _ => false,
        },
        _ => false,
    }
}
