// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_mode::ExecutionMode;
use crate::programmable_transactions::execution::check_private_generics;
use crate::sp;
use crate::static_programmable_transactions::{env::Env, loading::ast::Type, typing::ast as T};
use move_binary_format::{CompiledModule, file_format::Visibility};
use sui_types::{
    balance::{
        BALANCE_MODULE_NAME, SEND_TO_ACCOUNT_FUNCTION_NAME, WITHDRAW_FROM_ACCOUNT_FUNCTION_NAME,
    },
    error::{ExecutionError, ExecutionErrorKind, command_argument_error},
    execution_status::CommandArgumentError,
};

struct Context {
    gas_coin: IsDirty,
    objects: Vec<IsDirty>,
    pure: Vec<IsDirty>,
    receiving: Vec<IsDirty>,
    results: Vec<Vec<IsDirty>>,
}

/// Is dirty for entry verifier rules
type IsDirty = bool;

impl Context {
    fn new(txn: &T::Transaction) -> Self {
        Self {
            gas_coin: false,
            objects: txn.objects.iter().map(|_| false).collect(),
            pure: txn.pure.iter().map(|_| false).collect(),
            receiving: txn.receiving.iter().map(|_| false).collect(),
            results: vec![],
        }
    }

    // check if dirty, and mark it as fixed if mutably borrowing a pure input
    fn is_dirty(&self, arg: &T::Argument) -> bool {
        self.is_loc_dirty(arg.value.0.location())
    }

    fn is_loc_dirty(&self, location: T::Location) -> bool {
        match location {
            T::Location::TxContext => false, // TxContext is never dirty
            T::Location::GasCoin => self.gas_coin,
            T::Location::ObjectInput(i) => self.objects[i as usize],
            T::Location::PureInput(i) => self.pure[i as usize],
            T::Location::ReceivingInput(i) => self.receiving[i as usize],
            T::Location::Result(i, j) => self.results[i as usize][j as usize],
        }
    }

    /// Marks mutable usages as dirty. We don't care about `Move` since the value will be moved
    /// and that location is no longer accessible.
    fn mark_dirty(&mut self, arg: &T::Argument) {
        match &arg.value.0 {
            T::Argument__::Borrow(/* mut */ true, loc) => self.mark_loc_dirty(*loc),
            T::Argument__::Borrow(/* mut */ false, _)
            | T::Argument__::Use(_)
            | T::Argument__::Read(_)
            | &T::Argument__::Freeze(_) => (),
        }
    }

    fn mark_loc_dirty(&mut self, location: T::Location) {
        match location {
            T::Location::TxContext => (), // TxContext is never dirty, so nothing to do
            T::Location::GasCoin => self.gas_coin = true,
            T::Location::ObjectInput(i) => self.objects[i as usize] = true,
            T::Location::PureInput(i) => self.pure[i as usize] = true,
            T::Location::ReceivingInput(i) => self.receiving[i as usize] = true,
            T::Location::Result(i, j) => self.results[i as usize][j as usize] = true,
        }
    }
}

/// Checks the following
/// - entry function taint rules
/// - valid visibility for move function calls
///   - Can be disabled under certain execution modes
/// - private generics rules for move function calls
/// - no references returned from move calls
///    - Can be disabled under certain execution modes
pub fn verify<Mode: ExecutionMode>(env: &Env, txn: &T::Transaction) -> Result<(), ExecutionError> {
    let mut context = Context::new(txn);
    for c in &txn.commands {
        let result_dirties = command::<Mode>(env, &mut context, c)
            .map_err(|e| e.with_command_index(c.idx as usize))?;
        assert_invariant!(
            result_dirties.len() == c.value.result_type.len(),
            "result length mismatch"
        );
        context.results.push(result_dirties);
    }
    Ok(())
}

fn command<Mode: ExecutionMode>(
    env: &Env,
    context: &mut Context,
    sp!(_, c): &T::Command,
) -> Result<Vec<IsDirty>, ExecutionError> {
    let result = &c.result_type;
    Ok(match &c.command {
        T::Command__::MoveCall(call) => move_call::<Mode>(env, context, call, result)?,
        T::Command__::TransferObjects(objs, recipient) => {
            arguments(env, context, objs);
            argument(env, context, recipient);
            vec![]
        }
        T::Command__::SplitCoins(_, coin, amounts) => {
            let amounts_are_dirty = arguments(env, context, amounts);
            let coin_is_dirty = argument(env, context, coin);
            let is_dirty = amounts_are_dirty || coin_is_dirty;
            if is_dirty {
                context.mark_dirty(coin);
            }
            vec![is_dirty; result.len()]
        }
        T::Command__::MergeCoins(_, target, coins) => {
            let is_dirty = arguments(env, context, coins);
            argument(env, context, target);
            if is_dirty {
                context.mark_dirty(target);
            }
            vec![]
        }
        T::Command__::MakeMoveVec(_, args) => {
            let is_dirty = arguments(env, context, args);
            debug_assert_eq!(result.len(), 1);
            vec![is_dirty]
        }
        T::Command__::Publish(_, _, _) => {
            debug_assert_eq!(Mode::packages_are_predefined(), result.is_empty());
            debug_assert_eq!(!Mode::packages_are_predefined(), result.len() == 1);
            result.iter().map(|_| false).collect::<Vec<_>>()
        }
        T::Command__::Upgrade(_, _, _, ticket, _) => {
            debug_assert_eq!(result.len(), 1);
            let result = vec![false];
            argument(env, context, ticket);
            result
        }
    })
}

fn arguments(env: &Env, context: &Context, args: &[T::Argument]) -> bool {
    args.iter().any(|arg| argument(env, context, arg))
}

fn argument(_env: &Env, context: &Context, arg: &T::Argument) -> bool {
    context.is_dirty(arg)
}

fn move_call<Mode: ExecutionMode>(
    env: &Env,
    context: &mut Context,
    call: &T::MoveCall,
    result: &T::ResultType,
) -> Result<Vec<IsDirty>, ExecutionError> {
    let T::MoveCall {
        function,
        arguments: args,
    } = call;
    check_signature::<Mode>(function)?;
    check_private_generics(&function.runtime_id, function.name.as_ident_str())?;
    let (vis, is_entry) = check_visibility::<Mode>(env, function)?;
    let arg_dirties = args
        .iter()
        .map(|arg| argument(env, context, arg))
        .collect::<Vec<_>>();
    if is_entry && matches!(vis, Visibility::Private) {
        for (idx, &arg_is_dirty) in arg_dirties.iter().enumerate() {
            if arg_is_dirty && !Mode::allow_arbitrary_values() {
                return Err(command_argument_error(
                    CommandArgumentError::InvalidArgumentToPrivateEntryFunction,
                    idx,
                ));
            }
        }
    } else if !is_entry {
        // mark args dirty if not entry
        for arg in args {
            context.mark_dirty(arg);
        }
    }
    Ok(vec![true; result.len()])
}

fn check_signature<Mode: ExecutionMode>(
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
    for (idx, ty) in function.signature.return_.iter().enumerate() {
        check_return_type::<Mode>(idx, ty)?;
    }
    Ok(())
}

fn check_visibility<Mode: ExecutionMode>(
    env: &Env,
    function: &T::LoadedFunction,
) -> Result<(Visibility, /* is_entry */ bool), ExecutionError> {
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
                // Special case: allow private accumulator entrypoints in test/simtest environments
                // TODO: delete this as soon as the accumulator Move API is available
                if env.protocol_config.allow_private_accumulator_entrypoints()
                    && module.self_id().name() == BALANCE_MODULE_NAME
                    && (function.name.as_ident_str() == SEND_TO_ACCOUNT_FUNCTION_NAME
                        || function.name.as_ident_str() == WITHDRAW_FROM_ACCOUNT_FUNCTION_NAME)
                {
                    // Allow these specific functions
                } else {
                    return Err(ExecutionError::new_with_source(
                        ExecutionErrorKind::NonEntryFunctionInvoked,
                        "Can only call `entry` or `public` functions",
                    ));
                }
            }
        }
    };
    Ok((visibility, is_entry))
}
