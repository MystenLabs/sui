// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_mode::ExecutionMode;
use crate::sp;
use crate::static_programmable_transactions::{env::Env, loading::ast::Type, typing::ast as T};
use move_binary_format::file_format::Visibility;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::ModuleId;
use sui_types::base_types::TxContextKind;
use sui_types::error::ExecutionErrorTrait;
use sui_types::execution_status::{CommandArgumentError, ExecutionErrorKind};
use sui_verifier::private_generics_verifier_v2;

/// Checks the following
/// - valid visibility for move function calls
///   - Can be disabled under certain execution modes
/// - private generics rules for move function calls
/// - no references returned from move calls
///    - Can be disabled under certain execution modes
///    - Can be disabled via a feature flag
/// - valid `TxContext` usage in the signature
///    - Gated by a feature flag
pub fn verify<Mode: ExecutionMode>(
    env: &Env<Mode>,
    txn: &T::Transaction,
) -> Result<(), Mode::Error> {
    for c in &txn.commands {
        command::<Mode>(env, c).map_err(|e| e.with_command_index(c.idx as usize))?;
    }
    Ok(())
}

fn command<Mode: ExecutionMode>(
    env: &Env<Mode>,
    sp!(_, c): &T::Command,
) -> Result<(), Mode::Error> {
    let T::Command_ {
        command,
        result_type: _,
        drop_values: _,
        incurs_post_execution_checks: _,
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
/// - valid `TxContext` usage in the signature
/// - valid visibility
/// - private generics rules
fn move_call<Mode: ExecutionMode>(env: &Env<Mode>, call: &T::MoveCall) -> Result<(), Mode::Error> {
    let T::MoveCall {
        function,
        arguments: _,
    } = call;
    check_signature::<Mode>(env, function)?;
    check_tx_context::<Mode>(env, function)?;
    check_private_generics_v2(&function.original_mid, function.name.as_ident_str())?;
    check_visibility::<Mode>(env, function)?;
    Ok(())
}

fn check_signature<Mode: ExecutionMode>(
    env: &Env<Mode>,
    function: &T::LoadedFunction,
) -> Result<(), Mode::Error> {
    fn check_return_type<Mode: ExecutionMode, E: ExecutionErrorTrait>(
        idx: usize,
        return_type: &T::Type,
    ) -> Result<(), E> {
        if let Type::Reference(_, _) = return_type
            && !Mode::allow_arbitrary_values()
        {
            return Err(E::from_kind(
                ExecutionErrorKind::InvalidPublicFunctionReturnType {
                    idx: checked_as!(idx, u16)?,
                },
            ));
        }
        Ok(())
    }

    if env.protocol_config.allow_references_in_ptbs() {
        return Ok(());
    }

    for (idx, ty) in function.signature.return_.iter().enumerate() {
        check_return_type::<Mode, Mode::Error>(idx, ty)?;
    }
    Ok(())
}

/// Checks `TxContext` usage in the function's signature:
/// - In the parameters, `TxContext` can appear at most once as `&mut TxContext`, or any number of
///   times as `&TxContext`. It can never be taken by value.
/// - It can never appear in return position, meaning it can never become a result of a command.
///
/// These rules apply to the instantiated signature, so they cover generic parameters and return
/// types instantiated with `TxContext`. Unlike the reference rules in `check_signature`, they are
/// enforced under all execution modes.
fn check_tx_context<Mode: ExecutionMode>(
    env: &Env<Mode>,
    function: &T::LoadedFunction,
) -> Result<(), Mode::Error> {
    if !env.protocol_config.ptb_tx_context_restrictions() {
        return Ok(());
    }
    check_no_tx_context_by_value::<Mode::Error>(&function.signature.parameters)?;
    check_tx_context_refs::<Mode::Error>(&function.signature.parameters)?;
    check_no_tx_context_return::<Mode::Error>(&function.signature.return_)?;
    Ok(())
}

/// `TxContext` can never be taken by value
fn check_no_tx_context_by_value<E: ExecutionErrorTrait>(parameters: &[Type]) -> Result<(), E> {
    let Some(idx) = parameters
        .iter()
        .position(|param| param.is_tx_context_by_value())
    else {
        return Ok(());
    };
    Err(E::new_with_source(
        ExecutionErrorKind::command_argument_error(
            CommandArgumentError::InvalidTxContext,
            checked_as!(idx, u16)?,
        ),
        "TxContext cannot be taken by value",
    ))
}

/// If `&mut TxContext` appears, it must be the only `TxContext` parameter: no other `TxContext`
/// reference, mutable or immutable, may appear alongside it
fn check_tx_context_refs<E: ExecutionErrorTrait>(parameters: &[Type]) -> Result<(), E> {
    let mut mut_idxs = parameters
        .iter()
        .enumerate()
        .filter(|(_, param)| param.is_tx_context() == TxContextKind::Mutable)
        .map(|(idx, _)| idx);
    let Some(first_mut_idx) = mut_idxs.next() else {
        return Ok(());
    };
    if let Some(second_mut_idx) = mut_idxs.next() {
        return Err(E::new_with_source(
            ExecutionErrorKind::command_argument_error(
                CommandArgumentError::InvalidTxContext,
                checked_as!(second_mut_idx, u16)?,
            ),
            "TxContext can be taken by mutable reference at most once",
        ));
    }
    if parameters
        .iter()
        .any(|param| param.is_tx_context() == TxContextKind::Immutable)
    {
        return Err(E::new_with_source(
            ExecutionErrorKind::command_argument_error(
                CommandArgumentError::InvalidTxContext,
                checked_as!(first_mut_idx, u16)?,
            ),
            "&mut TxContext cannot be used alongside other TxContext parameters",
        ));
    }
    Ok(())
}

/// `TxContext` can never appear in return position, by value or by reference
fn check_no_tx_context_return<E: ExecutionErrorTrait>(return_: &[Type]) -> Result<(), E> {
    let Some(idx) = return_.iter().position(|return_ty| {
        return_ty.is_tx_context() != TxContextKind::None || return_ty.is_tx_context_by_value()
    }) else {
        return Ok(());
    };
    Err(E::new_with_source(
        ExecutionErrorKind::command_argument_error(
            CommandArgumentError::InvalidTxContext,
            checked_as!(idx, u16)?,
        ),
        "TxContext cannot be returned from a Move call",
    ))
}

fn check_visibility<Mode: ExecutionMode>(
    _env: &Env<Mode>,
    function: &T::LoadedFunction,
) -> Result<(), Mode::Error> {
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
                return Err(Mode::Error::new_with_source(
                    ExecutionErrorKind::NonEntryFunctionInvoked,
                    "Can only call `entry` or `public` functions",
                ));
            }
        }
    };
    Ok(())
}

fn check_private_generics_v2<E: ExecutionErrorTrait>(
    callee_package: &ModuleId,
    callee_function: &IdentStr,
) -> Result<(), E> {
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
    Err(E::new_with_source(
        ExecutionErrorKind::NonEntryFunctionInvoked,
        msg,
    ))
}
