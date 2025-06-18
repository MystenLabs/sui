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
            T::Location::GasCoin => self.gas_coin.is_dirty(),
            T::Location::Input(i) => self.inputs[i as usize].is_dirty(),
            T::Location::Result(i, j) => self.results[i as usize][j as usize].is_dirty(),
        }
    }

    fn mark_dirty(&mut self, arg: &T::Argument) {
        self.mark_loc_dirty(arg.value.0.location())
    }

    fn mark_loc_dirty(&mut self, location: T::Location) {
        match location {
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
        command::<Mode>(env, &mut context, c, result)?;
    }
    Ok(())
}

fn command<Mode: ExecutionMode>(
    env: &Env,
    context: &mut Context,
    command: &T::Command,
    result: &T::ResultType,
) -> Result<(), ExecutionError> {
    match &command.value {
        T::Command_::MoveCall(call) => {
            let result_dirties = move_call::<Mode>(env, context, call, result)?;
            debug_assert!(result_dirties.len() == result.len());
            context.results.push(result_dirties);
        }
        T::Command_::TransferObjects(objs, recipient) => {
            arguments(env, context, objs);
            argument(env, context, recipient);
            debug_assert!(result.is_empty());
            context.results.push(vec![]);
        }
        T::Command_::SplitCoins(_, coin, amounts) => {
            let amounts_are_dirty = arguments(env, context, amounts);
            let coin_is_dirty = argument(env, context, coin);
            debug_assert!(!amounts_are_dirty);
            let is_dirty = amounts_are_dirty || coin_is_dirty;
            debug_assert_eq!(result.len(), amounts.len());
            context
                .results
                .push(vec![IsDirty::Fixed { is_dirty }; result.len()]);
        }
        T::Command_::MergeCoins(_, target, coins) => {
            let is_dirty = arguments(env, context, coins);
            argument(env, context, target);
            if is_dirty {
                context.mark_dirty(target);
            }
            debug_assert!(result.is_empty());
            context.results.push(vec![]);
        }
        T::Command_::MakeMoveVec(_, args) => {
            let is_dirty = arguments(env, context, args);
            debug_assert_eq!(result.len(), 1);
            context.results.push(vec![IsDirty::Fixed { is_dirty }]);
        }
        T::Command_::Publish(_, _, _) => {
            debug_assert_eq!(Mode::packages_are_predefined(), result.is_empty());
            debug_assert_eq!(!Mode::packages_are_predefined(), result.len() == 1);
            let result = result
                .iter()
                .map(|_| IsDirty::Fixed { is_dirty: false })
                .collect::<Vec<_>>();
            context.results.push(result);
        }
        T::Command_::Upgrade(_, _, _, ticket, _) => {
            debug_assert_eq!(result.len(), 1);
            let result = vec![IsDirty::Fixed { is_dirty: false }];
            argument(env, context, ticket);
            context.results.push(result);
        }
    }
    Ok(())
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
    let (_vis, is_entry) = check_visibility::<Mode>(env, function)?;
    let arg_dirties = args
        .iter()
        .map(|arg| argument(env, context, arg))
        .collect::<Vec<_>>();
    if is_entry {
        for (idx, &arg_is_dirty) in arg_dirties.iter().enumerate() {
            if arg_is_dirty && !Mode::allow_arbitrary_values() {
                return Err(command_argument_error(
                    CommandArgumentError::InvalidArgumentToPrivateEntryFunction,
                    idx,
                ));
            }
        }
        // mark args dirty if is entry
        for arg in args {
            context.mark_dirty(arg);
        }
    }
    let is_dirty = is_entry || arg_dirties.iter().any(|&d| d);
    Ok(vec![IsDirty::Fixed { is_dirty }; result.len()])
}

fn check_signature<Mode: ExecutionMode>(
    function: &T::LoadedFunction,
) -> Result<(), ExecutionError> {
    fn check_return_type<Mode: ExecutionMode>(
        idx: usize,
        return_type: &T::Type,
    ) -> Result<(), ExecutionError> {
        match return_type {
            Type::Reference(_, _) => {
                if !Mode::allow_arbitrary_values() {
                    return Err(ExecutionError::from_kind(
                        ExecutionErrorKind::InvalidPublicFunctionReturnType { idx: idx as u16 },
                    ));
                }
                todo!("RUNTIME"); // can we support this?
            }
            t => t,
        };
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
