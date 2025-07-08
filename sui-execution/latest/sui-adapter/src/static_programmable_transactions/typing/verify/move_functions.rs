// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_mode::ExecutionMode;
use crate::programmable_transactions::execution::check_private_generics;
use crate::static_programmable_transactions::typing::ast::InputArg;
use crate::static_programmable_transactions::{env::Env, loading::ast::Type, typing::ast as T};
use move_binary_format::{CompiledModule, file_format::Visibility};
use sui_types::{
    error::{ExecutionError, ExecutionErrorKind, command_argument_error},
    execution_status::CommandArgumentError,
};

struct Context {
    gas_coin: IsDirty,
    inputs: Vec<IsDirty>,
    results: Vec<Vec<IsDirty>>,
}

/// Is dirty for entry verifier rules
#[derive(Copy, Clone)]
enum IsDirty {
    /// BCS input is not yet fixed
    NotFixed,
    Fixed {
        is_dirty: bool,
    },
}

impl Context {
    fn new(inputs: &T::Inputs) -> Self {
        let inputs = inputs
            .iter()
            .map(|(arg, _)| match arg {
                InputArg::Pure(_) => IsDirty::NotFixed,
                InputArg::Receiving(_) | InputArg::Object(_) => IsDirty::Fixed { is_dirty: false },
            })
            .collect();
        Self {
            gas_coin: IsDirty::Fixed { is_dirty: false },
            inputs,
            results: vec![],
        }
    }

    // check if dirty, and mark it as fixed if mutably borrowing a pure input
    fn is_dirty(&mut self, arg: &T::Argument) -> bool {
        match &arg.value.0 {
            T::Argument__::Borrow(/* mut */ true, T::Location::Input(i)) => {
                match &mut self.inputs[*i as usize] {
                    input @ IsDirty::NotFixed => {
                        *input = IsDirty::Fixed { is_dirty: false };
                        false
                    }
                    IsDirty::Fixed { is_dirty } => *is_dirty,
                }
            }
            _ => self.is_loc_dirty(arg.value.0.location()),
        }
    }

    fn is_loc_dirty(&self, location: T::Location) -> bool {
        match location {
            T::Location::TxContext => false, // TxContext is never dirty
            T::Location::GasCoin => self.gas_coin.is_dirty(),
            T::Location::Input(i) => self.inputs[i as usize].is_dirty(),
            T::Location::Result(i, j) => self.results[i as usize][j as usize].is_dirty(),
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
            T::Location::GasCoin => self.gas_coin = IsDirty::Fixed { is_dirty: true },
            T::Location::Input(i) => match &mut self.inputs[i as usize] {
                // if it needs to be dirtied, it will first be marked as fixed
                IsDirty::NotFixed => (),
                IsDirty::Fixed { is_dirty } => *is_dirty = true,
            },
            T::Location::Result(i, j) => {
                self.results[i as usize][j as usize] = IsDirty::Fixed { is_dirty: true }
            }
        }
    }
}

impl IsDirty {
    fn is_dirty(self) -> bool {
        match self {
            IsDirty::NotFixed => false,
            IsDirty::Fixed { is_dirty } => is_dirty,
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
    let T::Transaction { inputs, commands } = txn;
    let mut context = Context::new(inputs);
    for (c, result) in commands {
        let result_dirties = command::<Mode>(env, &mut context, c, result)
            .map_err(|e| e.with_command_index(c.idx as usize))?;
        assert_invariant!(
            result_dirties.len() == result.len(),
            "result length mismatch"
        );
        context.results.push(result_dirties);
    }
    Ok(())
}

fn command<Mode: ExecutionMode>(
    env: &Env,
    context: &mut Context,
    command: &T::Command,
    result: &T::ResultType,
) -> Result<Vec<IsDirty>, ExecutionError> {
    Ok(match &command.value {
        T::Command_::MoveCall(call) => move_call::<Mode>(env, context, call, result)?,
        T::Command_::TransferObjects(objs, recipient) => {
            arguments(env, context, objs);
            argument(env, context, recipient);
            vec![]
        }
        T::Command_::SplitCoins(_, coin, amounts) => {
            let amounts_are_dirty = arguments(env, context, amounts);
            let coin_is_dirty = argument(env, context, coin);
            let is_dirty = amounts_are_dirty || coin_is_dirty;
            if is_dirty {
                context.mark_dirty(coin);
            }
            vec![IsDirty::Fixed { is_dirty }; result.len()]
        }
        T::Command_::MergeCoins(_, target, coins) => {
            let is_dirty = arguments(env, context, coins);
            argument(env, context, target);
            if is_dirty {
                context.mark_dirty(target);
            }
            vec![]
        }
        T::Command_::MakeMoveVec(_, args) => {
            let is_dirty = arguments(env, context, args);
            debug_assert_eq!(result.len(), 1);
            vec![IsDirty::Fixed { is_dirty }]
        }
        T::Command_::Publish(_, _, _) => {
            debug_assert_eq!(Mode::packages_are_predefined(), result.is_empty());
            debug_assert_eq!(!Mode::packages_are_predefined(), result.len() == 1);
            result
                .iter()
                .map(|_| IsDirty::Fixed { is_dirty: false })
                .collect::<Vec<_>>()
        }
        T::Command_::Upgrade(_, _, _, ticket, _) => {
            debug_assert_eq!(result.len(), 1);
            let result = vec![IsDirty::Fixed { is_dirty: false }];
            argument(env, context, ticket);
            result
        }
    })
}

fn arguments(env: &Env, context: &mut Context, args: &[T::Argument]) -> bool {
    args.iter().any(|arg| argument(env, context, arg))
}

fn argument(_env: &Env, context: &mut Context, arg: &T::Argument) -> bool {
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
    Ok(vec![IsDirty::Fixed { is_dirty: true }; result.len()])
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
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::NonEntryFunctionInvoked,
                    "Can only call `entry` or `public` functions",
                ));
            }
        }
    };
    Ok((visibility, is_entry))
}
