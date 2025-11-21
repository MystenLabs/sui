// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_mode::ExecutionMode;
use crate::sp;
use crate::static_programmable_transactions::{env::Env, loading::ast::Type, typing::ast as T};
use move_binary_format::file_format::Visibility;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::ModuleId;
use sui_types::error::{ExecutionError, ExecutionErrorKind};
use sui_verifier::private_generics_verifier_v2;

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
    check_private_generics_v2(&function.original_mid, function.name.as_ident_str())?;
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
        if let Type::Reference(_, _) = return_type
            && !Mode::allow_arbitrary_values()
        {
            return Err(ExecutionError::from_kind(
                ExecutionErrorKind::InvalidPublicFunctionReturnType { idx: idx as u16 },
            ));
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
    _env: &Env,
    function: &T::LoadedFunction,
) -> Result<(), ExecutionError> {
    let visibility = function.visibility;
    let is_entry = function.is_entry;
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

fn check_private_generics_v2(
    callee_package: &ModuleId,
    callee_function: &IdentStr,
) -> Result<(), ExecutionError> {
    let callee_address = *callee_package.address();
    let callee_module = callee_package.name();
    let callee = (callee_address, callee_module, callee_function);
    let Some((_f, internal_type_parameters)) = private_generics_verifier_v2::FUNCTIONS_TO_CHECK
        .iter()
        .find(|(f, _)| &callee == f)
    else {
        return Ok(());
    };
    // If we find an internal type parameter, the call is automatically invalid--since we
    // are not in a module and cannot define any types to satisfy the internal constraint.
    let Some((internal_idx, _)) = internal_type_parameters
        .iter()
        .enumerate()
        .find(|(_, is_internal)| **is_internal)
    else {
        // No `internal` type parameters, so it is ok to call
        return Ok(());
    };
    let callee_package_name = private_generics_verifier_v2::callee_package_name(&callee_address);
    let help =
        private_generics_verifier_v2::help_message(&callee_address, callee_module, callee_function);
    let msg = format!(
        "Cannot directly call function '{}::{}::{}' since type parameter #{} can \
                 only be instantiated with types defined within the caller's module.{}",
        callee_package_name, callee_module, callee_function, internal_idx, help,
    );
    Err(ExecutionError::new_with_source(
        ExecutionErrorKind::NonEntryFunctionInvoked,
        msg,
    ))
}
