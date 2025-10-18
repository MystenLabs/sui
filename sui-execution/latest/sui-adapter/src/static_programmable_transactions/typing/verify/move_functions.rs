// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_mode::ExecutionMode;
use crate::programmable_transactions::execution::check_private_generics;
use crate::sp;
use crate::static_programmable_transactions::{env::Env, loading::ast::Type, typing::ast as T};
use move_binary_format::{CompiledModule, file_format::Visibility};
use sui_types::error::{ExecutionError, ExecutionErrorKind};

/// Checks the following
/// - valid visibility for move function calls
///   - Can be disabled under certain execution modes
/// - private generics rules for move function calls
/// - no references returned from move calls
///    - Can be disabled under certain execution modes
///    - Can be disabled via a feature flag
pub fn verify<Mode: ExecutionMode>(env: &Env, txn: &T::Transaction) -> Result<(), ExecutionError> {
    for c in &txn.commands {
        command::<Mode>(env, c).map_err(|e| e.with_command_index(c.idx as usize))?;
    }
    Ok(())
}

fn command<Mode: ExecutionMode>(env: &Env, sp!(_, c): &T::Command) -> Result<(), ExecutionError> {
    let T::Command_ {
        command,
        result_type: _,
        drop_values: _,
        consumed_shared_objects: _,
    } = c;
    match command {
        T::Command__::MoveCall(call) => move_call::<Mode>(env, call)?,
        T::Command__::TransferObjects(_, _)
        | T::Command__::SplitCoins(_, _, _)
        | T::Command__::MergeCoins(_, _, _)
        | T::Command__::MakeMoveVec(_, _)
        | T::Command__::Publish(_, _, _)
        | T::Command__::Upgrade(_, _, _, _, _) => (),
    }
    Ok(())
}

/// Checks a move call for
/// - valid signature (no references in return type)
/// - valid visibility
/// - private generics rules
fn move_call<Mode: ExecutionMode>(env: &Env, call: &T::MoveCall) -> Result<(), ExecutionError> {
    let T::MoveCall {
        function,
        arguments: _,
    } = call;
    check_signature::<Mode>(env, function)?;
    check_private_generics(&function.runtime_id, function.name.as_ident_str())?;
    check_visibility::<Mode>(env, function)?;
    Ok(())
}

fn check_signature<Mode: ExecutionMode>(
    env: &Env,
    function: &T::LoadedFunction,
) -> Result<(), ExecutionError> {
    fn check_return_type<Mode: ExecutionMode>(
        idx: usize,
        return_type: &T::Type,
    ) -> Result<(), ExecutionError> {
        if let Type::Reference(_, _) = return_type {
            if !Mode::allow_arbitrary_values() {
                return Err(ExecutionError::from_kind(
                    ExecutionErrorKind::InvalidPublicFunctionReturnType { idx: idx as u16 },
                ));
            }
        }
        Ok(())
    }

    if env.protocol_config.allow_references_in_ptbs() {
        return Ok(());
    }

    for (idx, ty) in function.signature.return_.iter().enumerate() {
        check_return_type::<Mode>(idx, ty)?;
    }
    Ok(())
}

fn check_visibility<Mode: ExecutionMode>(
    env: &Env,
    function: &T::LoadedFunction,
) -> Result<(), ExecutionError> {
    let module = env.module_definition(&function.runtime_id, &function.linkage)?;
    let module: &CompiledModule = module.as_ref();
    let Some((_index, fdef)) = module.find_function_def_by_name(function.name.as_str()) else {
        invariant_violation!(
            "Could not resolve function '{}' in module {}. \
            This should have been checked when linking",
            &function.name,
            module.self_id(),
        );
    };
    let visibility = fdef.visibility;
    let is_entry = fdef.is_entry;
    match (visibility, is_entry) {
        // can call entry
        (Visibility::Private | Visibility::Friend, true) => (),
        // can call public entry
        (Visibility::Public, true) => (),
        // can call public
        (Visibility::Public, false) => (),
        // cannot call private or friend if not entry
        (Visibility::Private | Visibility::Friend, false) => {
            if !Mode::allow_arbitrary_function_calls() {
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::NonEntryFunctionInvoked,
                    "Can only call `entry` or `public` functions",
                ));
            }
        }
    };
    Ok(())
}
