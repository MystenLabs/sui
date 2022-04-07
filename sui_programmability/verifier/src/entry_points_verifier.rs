// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    file_format::{AbilitySet, FunctionDefinition, SignatureToken, Visibility},
    CompiledModule,
};
use move_core_types::{ident_str, identifier::IdentStr, language_storage::TypeTag};
use sui_types::{
    base_types::{TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME},
    error::{SuiError, SuiResult},
    SUI_FRAMEWORK_ADDRESS,
};

pub const INIT_FN_NAME: &IdentStr = ident_str!("init");

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
pub fn verify_module(module: &CompiledModule) -> SuiResult {
    for func_def in &module.function_defs {
        // find candidate entry functions and checke their parameters
        // (ignore other functions)
        if func_def.visibility != Visibility::Script {
            // it's not an entry function as a non-script function
            // cannot be called from Sui
            continue;
        }
        let is_entrypoint_execution = false;
        verify_entry_function_impl(module, func_def, None, is_entrypoint_execution)
            .map_err(|error| SuiError::ModuleVerificationFailure { error })?;
    }
    Ok(())
}

/// Checks if this module has a conformant `init`
// TODO make this static
pub fn module_has_init(module: &CompiledModule) -> bool {
    let view = BinaryIndexedView::Module(module);
    let fdef_opt = module.function_defs.iter().find(|fdef| {
        let handle = view.function_handle_at(fdef.function);
        let name = view.identifier_at(handle.name);
        name == INIT_FN_NAME
    });
    let fdef = match fdef_opt {
        None => return false,
        Some(fdef) => fdef,
    };
    if fdef.visibility != Visibility::Private {
        return false;
    }

    let fhandle = module.function_handle_at(fdef.function);
    if !fhandle.type_parameters.is_empty() {
        return false;
    }

    if !view.signature_at(fhandle.return_).0.is_empty() {
        return false;
    }

    let parameters = &view.signature_at(fhandle.parameters).0;
    if parameters.len() != 1 {
        return false;
    }

    is_tx_context(&view, &parameters[0])
}

pub fn verify_entry_function(
    module: &CompiledModule,
    func_def: &FunctionDefinition,
    function_type_args: &[TypeTag],
) -> SuiResult {
    let is_entrypoint_execution = true;
    verify_entry_function_impl(
        module,
        func_def,
        Some(function_type_args),
        is_entrypoint_execution,
    )
    .map_err(|error| SuiError::InvalidFunctionSignature { error })
}

fn verify_entry_function_impl(
    module: &CompiledModule,
    func_def: &FunctionDefinition,
    function_type_args: Option<&[TypeTag]>,
    _is_entrypoint_execution: bool,
) -> Result<(), String> {
    let view = BinaryIndexedView::Module(module);
    let handle = view.function_handle_at(func_def.function);
    let params = view.signature_at(handle.parameters);
    let pessimistic_type_args = &handle.type_parameters;

    // must have at least on &mut TxContext param
    if params.is_empty() {
        debug_assert!(!_is_entrypoint_execution);
        return Err(format!(
            "No parameters in entry function {}",
            view.identifier_at(handle.name)
        ));
    }
    let last_param = params.0.last().unwrap();
    if !is_tx_context(&view, last_param) {
        debug_assert!(!_is_entrypoint_execution);
        return Err(format!(
            "Expected last parameter of function signature to be &mut {}::{}::{}, but found {:?}",
            SUI_FRAMEWORK_ADDRESS, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME, last_param
        ));
    }

    let last_idx = params.0.len() - 1;
    let all_but_last = &params.0[..last_idx];
    let mut first_prim_idx_opt = None;
    for (idx, param_sig_token) in all_but_last.iter().enumerate() {
        if !is_object(&view, pessimistic_type_args, param_sig_token)? {
            first_prim_idx_opt = Some(idx);
            break;
        }
    }
    // if none, using last_idx will result in prim_suffix being empty
    let first_prim_idx = first_prim_idx_opt.unwrap_or(last_idx);
    let prim_suffix = &all_but_last[first_prim_idx..];
    for (pos, t) in prim_suffix
        .iter()
        .enumerate()
        .map(|(idx, s)| (idx + first_prim_idx, s))
    {
        if !is_primitive(function_type_args, pessimistic_type_args, t) {
            return Err(format!(
                "Expected primitive parameter after object parameters for function {} at \
                        position {}",
                view.identifier_at(handle.name),
                pos,
            ));
        }
    }

    for (pos, ret_t) in view.signature_at(handle.return_).0.iter().enumerate() {
        if !is_entry_ret_type(function_type_args, pessimistic_type_args, ret_t) {
            return Err(format!(
                "Expected primitive return type for function {} at position {}",
                view.identifier_at(handle.name),
                pos,
            ));
        }
    }

    Ok(())
}

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

pub fn is_object(
    view: &BinaryIndexedView,
    function_type_args: &[AbilitySet],
    t: &SignatureToken,
) -> Result<bool, String> {
    use SignatureToken as S;
    match t {
        S::Reference(inner) | S::MutableReference(inner) | S::Vector(inner) => {
            is_object(view, function_type_args, inner)
        }
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
        | S::U64
        | S::U128
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

/// Checks if a given parameter is of a primitive type. It's a mirror
/// of the is_primitive function in the adapter module that operates
/// on Type-s.
fn is_primitive(
    type_args: Option<&[TypeTag]>,
    function_type_constraints: &[AbilitySet],
    t: &SignatureToken,
) -> bool {
    // nested vectors of primitive types are OK to arbitrary nesting
    // level
    is_primitive_with_depth(type_args, function_type_constraints, t, 0, u32::MAX)
}

// Checks if a given type is the correct entry function return type. It's a mirror
/// of the is_entry_ret_type function in the adapter module that
/// operates on Type-s.
fn is_entry_ret_type(
    type_args: Option<&[TypeTag]>,
    function_type_constraints: &[AbilitySet],
    t: &SignatureToken,
) -> bool {
    // allow vectors of vectors but no deeper nesting
    is_primitive_with_depth(type_args, function_type_constraints, t, 0, 2)
}

fn is_primitive_with_depth(
    type_args: Option<&[TypeTag]>,
    function_type_constraints: &[AbilitySet],
    t: &SignatureToken,
    depth: u32,
    max_depth: u32,
) -> bool {
    use SignatureToken as S;
    match t {
        S::Bool | S::U8 | S::U64 | S::U128 | S::Address => true,

        // type parameters are optimistically considered primitives if no type args are given
        // if the type argument is out of bounds, that will be an error elsewhere
        S::TypeParameter(idx) => {
            let idx = *idx as usize;
            let constrained_by_key = function_type_constraints
                .get(idx)
                .map(|abs| abs.has_key())
                .unwrap_or(false);
            constrained_by_key
                || type_args
                    .and_then(|type_args| {
                        type_args
                            .get(idx)
                            .map(|tt| is_primitive_vm_type_with_depth(tt, depth, max_depth))
                    })
                    .unwrap_or(true)
        }

        S::Vector(inner) if depth < max_depth => is_primitive_with_depth(
            type_args,
            function_type_constraints,
            inner,
            depth + 1,
            max_depth,
        ),

        S::Vector(_)
        | S::Signer
        | S::Struct(_)
        | S::StructInstantiation(..)
        | S::Reference(_)
        | S::MutableReference(_) => false,
    }
}

fn is_primitive_vm_type_with_depth(tt: &TypeTag, depth: u32, max_depth: u32) -> bool {
    use TypeTag as T;
    match tt {
        T::Bool | T::U8 | T::U64 | T::U128 | T::Address => true,
        T::Vector(inner) if depth < max_depth => {
            is_primitive_vm_type_with_depth(inner, depth + 1, max_depth)
        }
        T::Vector(_) | T::Struct(_) | T::Signer => false,
    }
}
