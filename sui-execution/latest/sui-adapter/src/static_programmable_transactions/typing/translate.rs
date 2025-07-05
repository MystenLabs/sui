// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{ast as T, env::Env};
use crate::{
    execution_mode::ExecutionMode,
    programmable_transactions::context::EitherError,
    static_programmable_transactions::{
        loading::ast::{self as L, InputArg, Type},
        spanned::sp,
        typing::ast::{BytesConstraint, BytesUsage},
    },
};
use std::{collections::BTreeMap, rc::Rc};
use sui_types::{
    base_types::TxContextKind,
    coin::RESOLVED_COIN_STRUCT,
    error::{ExecutionError, ExecutionErrorKind, command_argument_error},
    execution_status::CommandArgumentError,
};

struct Context {
    current_command: u16,
    gathered_input_types: BTreeMap<u16, BTreeMap<Type, BytesConstraint>>,
    inputs: Vec<(InputArg, InputType)>,
    results: Vec<T::ResultType>,
}

enum InputType {
    Bytes,
    Fixed(Type),
}

enum LocationType<'context> {
    Bytes(
        &'context mut InputType,
        &'context mut BTreeMap<Type, BytesConstraint>,
    ),
    Fixed(Type),
}

impl Context {
    fn new(linputs: L::Inputs) -> Self {
        let mut context = Context {
            current_command: 0,
            gathered_input_types: BTreeMap::new(),
            inputs: vec![],
            results: vec![],
        };
        context.inputs = linputs
            .into_iter()
            .enumerate()
            .map(|(i, (arg, ty))| {
                let idx = i as u16;
                let ty = match ty {
                    L::InputType::Bytes => {
                        context.gathered_input_types.insert(idx, BTreeMap::new());
                        InputType::Bytes
                    }
                    L::InputType::Fixed(t) => InputType::Fixed(t),
                };
                (arg, ty)
            })
            .collect();
        context
    }

    fn finish(self) -> Vec<(InputArg, T::InputType)> {
        let Self {
            mut gathered_input_types,
            inputs,
            ..
        } = self;
        inputs
            .into_iter()
            .enumerate()
            .map(|(i, (arg, ty))| match (&arg, ty) {
                (InputArg::Pure(_) | InputArg::Receiving(_), _) => {
                    let tys = gathered_input_types.remove(&(i as u16)).unwrap_or_default();
                    (arg, T::InputType::Bytes(tys))
                }
                (_, InputType::Bytes) => {
                    unreachable!()
                }
                (_, InputType::Fixed(t)) => (arg, T::InputType::Fixed(t)),
            })
            .collect()
    }

    fn location_type<'context>(
        &'context mut self,
        env: &Env,
        location: T::Location,
    ) -> Result<LocationType<'context>, ExecutionError> {
        Ok(match location {
            T::Location::GasCoin => LocationType::Fixed(env.gas_coin_type()?),
            T::Location::Input(i) => match &mut self.inputs[i as usize].1 {
                t @ InputType::Bytes => {
                    LocationType::Bytes(t, self.gathered_input_types.get_mut(&i).unwrap())
                }
                InputType::Fixed(t) => LocationType::Fixed(t.clone()),
            },
            T::Location::Result(i, j) => {
                LocationType::Fixed(self.results[i as usize][j as usize].clone())
            }
            T::Location::TxContext => LocationType::Fixed(env.tx_context_type()?),
        })
    }
}

pub fn transaction<Mode: ExecutionMode>(
    env: &Env,
    lt: L::Transaction,
) -> Result<T::Transaction, ExecutionError> {
    let L::Transaction { inputs, commands } = lt;
    let mut context = Context::new(inputs);
    let commands = commands
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let idx = i as u16;
            context.current_command = idx;
            let (c, tys) =
                command::<Mode>(env, &mut context, c).map_err(|e| e.with_command_index(i))?;
            context.results.push(tys.clone());
            Ok((sp(idx, c), tys))
        })
        .collect::<Result<Vec<_>, ExecutionError>>()?;
    let inputs = context.finish();
    let mut ast = T::Transaction { inputs, commands };
    // mark the last usage of references as Move instead of Copy
    scope_references::transaction(&mut ast);
    Ok(ast)
}

fn command<Mode: ExecutionMode>(
    env: &Env,
    context: &mut Context,
    command: L::Command,
) -> Result<(T::Command_, T::ResultType), ExecutionError> {
    Ok(match command {
        L::Command::MoveCall(lmc) => {
            let L::MoveCall {
                function,
                arguments: largs,
            } = *lmc;
            let arg_locs = locations(context, 0, largs)?;
            let tx_context_kind = tx_context_kind(&function);
            let parameter_tys = match tx_context_kind {
                TxContextKind::None => &function.signature.parameters,
                TxContextKind::Mutable | TxContextKind::Immutable => {
                    let n = function.signature.parameters.len();
                    &function.signature.parameters[0..n - 1]
                }
            };
            let num_args = arg_locs.len();
            let num_parameters = parameter_tys.len();
            if num_args != num_parameters {
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::ArityMismatch,
                    format!(
                        "Expected {} argument{} calling function '{}::{}', but found {}",
                        num_parameters,
                        if num_parameters == 1 { "" } else { "s" },
                        function.storage_id,
                        function.name,
                        num_args,
                    ),
                ));
            }
            let mut args = arguments(env, context, 0, arg_locs, parameter_tys.iter().cloned())?;
            match tx_context_kind {
                TxContextKind::None => (),
                TxContextKind::Mutable | TxContextKind::Immutable => {
                    let is_mut = match tx_context_kind {
                        TxContextKind::Mutable => true,
                        TxContextKind::Immutable => false,
                        TxContextKind::None => unreachable!(),
                    };
                    // TODO this is out of bounds of the original PTB arguments... what do we
                    // do here?
                    let idx = args.len() as u16;
                    let arg__ = T::Argument__::Borrow(is_mut, T::Location::TxContext);
                    let ty = Type::Reference(is_mut, Rc::new(env.tx_context_type()?));
                    args.push(sp(idx, (arg__, ty)));
                }
            }
            let result = function.signature.return_.clone();
            (
                T::Command_::MoveCall(Box::new(T::MoveCall {
                    function,
                    arguments: args,
                })),
                result,
            )
        }
        L::Command::TransferObjects(lobjects, laddress) => {
            let object_locs = locations(context, 0, lobjects)?;
            let address_loc = one_location(context, object_locs.len(), laddress)?;
            let objects = constrained_arguments(
                env,
                context,
                0,
                object_locs,
                |ty| {
                    let abilities = ty.abilities();
                    Ok(abilities.has_store() && abilities.has_key())
                },
                CommandArgumentError::InvalidTransferObject,
            )?;
            let address = argument(env, context, objects.len(), address_loc, Type::Address)?;
            (T::Command_::TransferObjects(objects, address), vec![])
        }
        L::Command::SplitCoins(lcoin, lamounts) => {
            let coin_loc = one_location(context, 0, lcoin)?;
            let amount_locs = locations(context, 1, lamounts)?;
            let coin = coin_mut_ref_argument(env, context, 0, coin_loc)?;
            let coin_type = match &coin.value.1 {
                Type::Reference(true, ty) => (**ty).clone(),
                ty => invariant_violation!("coin must be a mutable reference. Found: {ty:?}"),
            };
            let amounts = arguments(
                env,
                context,
                1,
                amount_locs,
                std::iter::repeat_with(|| Type::U64),
            )?;
            let result = vec![coin_type.clone(); amounts.len()];
            (T::Command_::SplitCoins(coin_type, coin, amounts), result)
        }
        L::Command::MergeCoins(ltarget, lcoins) => {
            let target_loc = one_location(context, 0, ltarget)?;
            let coin_locs = locations(context, 1, lcoins)?;
            let target = coin_mut_ref_argument(env, context, 0, target_loc)?;
            let coin_type = match &target.value.1 {
                Type::Reference(true, ty) => (**ty).clone(),
                ty => invariant_violation!("target must be a mutable reference. Found: {ty:?}"),
            };
            let coins = arguments(
                env,
                context,
                1,
                coin_locs,
                std::iter::repeat_with(|| coin_type.clone()),
            )?;
            (T::Command_::MergeCoins(coin_type, target, coins), vec![])
        }
        L::Command::MakeMoveVec(Some(ty), lelems) => {
            let elem_locs = locations(context, 0, lelems)?;
            let elems = arguments(
                env,
                context,
                0,
                elem_locs,
                std::iter::repeat_with(|| ty.clone()),
            )?;
            (
                T::Command_::MakeMoveVec(ty.clone(), elems),
                vec![env.vector_type(ty)?],
            )
        }
        L::Command::MakeMoveVec(None, lelems) => {
            let mut lelems = lelems.into_iter();
            let Some(lfirst) = lelems.next() else {
                // TODO maybe this should be a different errors for CLI usage
                invariant_violation!(
                    "input checker ensures if args are empty, there is a type specified"
                );
            };
            let first_loc = one_location(context, 0, lfirst)?;
            let first_arg = constrained_argument(
                env,
                context,
                0,
                first_loc,
                |ty| Ok(ty.abilities().has_key()),
                CommandArgumentError::InvalidMakeMoveVecNonObjectArgument,
            )?;
            let first_ty = first_arg.value.1.clone();
            let elems_loc = locations(context, 1, lelems)?;
            let mut elems = arguments(
                env,
                context,
                1,
                elems_loc,
                std::iter::repeat_with(|| first_ty.clone()),
            )?;
            elems.insert(0, first_arg);
            (
                T::Command_::MakeMoveVec(first_ty.clone(), elems),
                vec![env.vector_type(first_ty)?],
            )
        }
        L::Command::Publish(items, object_ids, linkage) => {
            let result = if Mode::packages_are_predefined() {
                // If packages are predefined, no upgrade cap is made
                vec![]
            } else {
                vec![env.upgrade_cap_type()?.clone()]
            };
            (T::Command_::Publish(items, object_ids, linkage), result)
        }
        L::Command::Upgrade(items, object_ids, object_id, la, linkage) => {
            let location = one_location(context, 0, la)?;
            let expected_ty = env.upgrade_ticket_type()?;
            let a = argument(env, context, 0, location, expected_ty)?;
            let res = env.upgrade_receipt_type()?;
            (
                T::Command_::Upgrade(items, object_ids, object_id, a, linkage),
                vec![res.clone()],
            )
        }
    })
}

fn tx_context_kind(function: &L::LoadedFunction) -> TxContextKind {
    match function.signature.parameters.last() {
        Some(ty) => ty.is_tx_context(),
        None => TxContextKind::None,
    }
}

fn one_location(
    context: &mut Context,
    command_arg_idx: usize,
    arg: L::Argument,
) -> Result<T::Location, ExecutionError> {
    let locs = locations(context, command_arg_idx, vec![arg])?;
    let Ok([loc]): Result<[T::Location; 1], _> = locs.try_into() else {
        return Err(command_argument_error(
            CommandArgumentError::InvalidArgumentArity,
            command_arg_idx,
        ));
    };
    Ok(loc)
}

fn locations<Items: IntoIterator<Item = L::Argument>>(
    context: &mut Context,
    start_idx: usize,
    args: Items,
) -> Result<Vec<T::Location>, ExecutionError>
where
    Items::IntoIter: ExactSizeIterator,
{
    fn splat_arg(
        context: &mut Context,
        res: &mut Vec<T::Location>,
        arg: L::Argument,
    ) -> Result<(), EitherError> {
        match arg {
            L::Argument::GasCoin => res.push(T::Location::GasCoin),
            L::Argument::Input(i) => {
                if i as usize >= context.inputs.len() {
                    return Err(CommandArgumentError::IndexOutOfBounds { idx: i }.into());
                }
                res.push(T::Location::Input(i))
            }
            L::Argument::NestedResult(i, j) => {
                let Some(command_result) = context.results.get(i as usize) else {
                    return Err(CommandArgumentError::IndexOutOfBounds { idx: i }.into());
                };
                if j as usize >= command_result.len() {
                    return Err(CommandArgumentError::SecondaryIndexOutOfBounds {
                        result_idx: i,
                        secondary_idx: j,
                    }
                    .into());
                };
                res.push(T::Location::Result(i, j))
            }
            L::Argument::Result(i) => {
                let Some(result) = context.results.get(i as usize) else {
                    return Err(CommandArgumentError::IndexOutOfBounds { idx: i }.into());
                };
                let Ok(len): Result<u16, _> = result.len().try_into() else {
                    invariant_violation!("Result of length greater than u16::MAX");
                };
                if len != 1 {
                    // TODO protocol config to allow splatting of args
                    return Err(CommandArgumentError::InvalidResultArity { result_idx: i }.into());
                }
                res.extend((0..len).map(|j| T::Location::Result(i, j)))
            }
        }
        Ok(())
    }

    let args = args.into_iter();
    let _args_len = args.len();
    let mut res = vec![];
    for (arg_idx, arg) in args.enumerate() {
        splat_arg(context, &mut res, arg)
            .map_err(|e| e.into_execution_error(start_idx + arg_idx))?;
    }
    debug_assert_eq!(res.len(), _args_len);
    Ok(res)
}

fn arguments(
    env: &Env,
    context: &mut Context,
    start_idx: usize,
    locations: Vec<T::Location>,
    expected_tys: impl IntoIterator<Item = Type>,
) -> Result<Vec<T::Argument>, ExecutionError> {
    locations
        .into_iter()
        .zip(expected_tys)
        .enumerate()
        .map(|(i, (location, expected_ty))| {
            argument(env, context, start_idx + i, location, expected_ty)
        })
        .collect()
}

fn argument(
    env: &Env,
    context: &mut Context,
    command_arg_idx: usize,
    location: T::Location,
    expected_ty: Type,
) -> Result<T::Argument, ExecutionError> {
    let arg__ = argument_(env, context, command_arg_idx, location, &expected_ty)
        .map_err(|e| e.into_execution_error(command_arg_idx))?;
    let arg_ = (arg__, expected_ty);
    Ok(sp(command_arg_idx as u16, arg_))
}

fn argument_(
    env: &Env,
    context: &mut Context,
    command_arg_idx: usize,
    location: T::Location,
    expected_ty: &Type,
) -> Result<T::Argument__, EitherError> {
    let current_command = context.current_command;
    let actual_ty = context.location_type(env, location)?;
    Ok(match (actual_ty, expected_ty) {
        // Reference location types
        (LocationType::Fixed(Type::Reference(a_is_mut, a)), Type::Reference(b_is_mut, b))
            if !b_is_mut || a_is_mut =>
        {
            debug_assert!(!a_is_mut || *b_is_mut);
            debug_assert!(expected_ty.abilities().has_copy());
            // unused since the type is fixed
            let unused_constraint = BytesConstraint {
                command: current_command,
                argument: command_arg_idx as u16,
                usage: BytesUsage::Copied,
            };
            check_type(unused_constraint, LocationType::Fixed((*a).clone()), b)?;
            T::Argument__::new_copy(location)
        }
        (LocationType::Fixed(Type::Reference(_, a)), b) => {
            // unused since the type is fixed
            let unused_constraint = BytesConstraint {
                command: current_command,
                argument: command_arg_idx as u16,
                usage: BytesUsage::Copied,
            };
            check_type(unused_constraint, LocationType::Fixed((*a).clone()), b)?;
            if !b.abilities().has_copy() {
                // TODO this should be a different error for missing copy
                return Err(CommandArgumentError::TypeMismatch.into());
            }
            T::Argument__::Read(T::Usage::new_copy(location))
        }

        // Non reference location types
        (actual_ty, Type::Reference(is_mut, inner)) => {
            let usage = if *is_mut {
                BytesUsage::ByMutRef
            } else {
                BytesUsage::ByImmRef
            };
            let constraint = BytesConstraint {
                command: current_command,
                argument: command_arg_idx as u16,
                usage,
            };
            check_type_impl(constraint, actual_ty, inner)?;
            T::Argument__::Borrow(/* mut */ *is_mut, location)
        }
        (actual_ty, _) => {
            let constraint = BytesConstraint {
                command: current_command,
                argument: command_arg_idx as u16,
                usage: BytesUsage::Copied,
            };
            check_type(constraint, actual_ty, expected_ty)?;
            T::Argument__::Use(if expected_ty.abilities().has_copy() {
                T::Usage::new_copy(location)
            } else {
                T::Usage::new_move(location)
            })
        }
    })
}

fn check_type(
    // not used if the type is fixed
    constraint: BytesConstraint,
    actual_ty: LocationType,
    expected_ty: &Type,
) -> Result<(), CommandArgumentError> {
    debug_assert!(matches!(constraint.usage, BytesUsage::Copied));
    check_type_impl(constraint, actual_ty, expected_ty)
}

fn check_type_impl(
    // not used if the type is fixed
    constraint: BytesConstraint,
    mut actual_ty: LocationType,
    expected_ty: &Type,
) -> Result<(), CommandArgumentError> {
    match &mut actual_ty {
        LocationType::Fixed(actual_ty) | LocationType::Bytes(InputType::Fixed(actual_ty), _) => {
            if actual_ty == expected_ty {
                Ok(())
            } else {
                Err(CommandArgumentError::TypeMismatch)
            }
        }
        LocationType::Bytes(ty, types) => {
            if matches!(&constraint.usage, BytesUsage::ByMutRef) {
                **ty = InputType::Fixed(expected_ty.clone());
            }
            types.entry(expected_ty.clone()).or_insert(constraint);
            // validity of pure types is checked elsewhere
            Ok(())
        }
    }
}

fn constrained_arguments<P: FnMut(&Type) -> Result<bool, ExecutionError>>(
    env: &Env,
    context: &mut Context,
    start_idx: usize,
    locations: Vec<T::Location>,
    mut is_valid: P,
    err_case: CommandArgumentError,
) -> Result<Vec<T::Argument>, ExecutionError> {
    let is_valid = &mut is_valid;
    locations
        .into_iter()
        .enumerate()
        .map(|(i, location)| {
            constrained_argument_(env, context, start_idx + i, location, is_valid, err_case)
        })
        .collect()
}

fn constrained_argument<P: FnMut(&Type) -> Result<bool, ExecutionError>>(
    env: &Env,
    context: &mut Context,
    command_arg_idx: usize,
    location: T::Location,
    mut is_valid: P,
    err_case: CommandArgumentError,
) -> Result<T::Argument, ExecutionError> {
    constrained_argument_(
        env,
        context,
        command_arg_idx,
        location,
        &mut is_valid,
        err_case,
    )
}

fn constrained_argument_<P: FnMut(&Type) -> Result<bool, ExecutionError>>(
    env: &Env,
    context: &mut Context,
    command_arg_idx: usize,
    location: T::Location,
    is_valid: &mut P,
    err_case: CommandArgumentError,
) -> Result<T::Argument, ExecutionError> {
    let arg_ = constrained_argument__(env, context, location, is_valid, err_case)
        .map_err(|e| e.into_execution_error(command_arg_idx))?;
    Ok(sp(command_arg_idx as u16, arg_))
}

fn constrained_argument__<P: FnMut(&Type) -> Result<bool, ExecutionError>>(
    env: &Env,
    context: &mut Context,
    location: T::Location,
    is_valid: &mut P,
    err_case: CommandArgumentError,
) -> Result<T::Argument_, EitherError> {
    if let Some(ty) = constrained_type(env, context, location, is_valid)? {
        if ty.abilities().has_copy() {
            Ok((T::Argument__::new_copy(location), ty))
        } else {
            Ok((T::Argument__::new_move(location), ty))
        }
    } else {
        Err(err_case.into())
    }
}

fn constrained_type<'a, P: FnMut(&Type) -> Result<bool, ExecutionError>>(
    env: &'a Env,
    context: &'a mut Context,
    location: T::Location,
    mut is_valid: P,
) -> Result<Option<Type>, ExecutionError> {
    let LocationType::Fixed(ty) = context.location_type(env, location)? else {
        return Ok(None);
    };
    Ok(if is_valid(&ty)? { Some(ty) } else { None })
}

fn coin_mut_ref_argument(
    env: &Env,
    context: &mut Context,
    command_arg_idx: usize,
    location: T::Location,
) -> Result<T::Argument, ExecutionError> {
    let arg_ = coin_mut_ref_argument_(env, context, location)
        .map_err(|e| e.into_execution_error(command_arg_idx))?;
    Ok(sp(command_arg_idx as u16, arg_))
}

fn coin_mut_ref_argument_(
    env: &Env,
    context: &mut Context,
    location: T::Location,
) -> Result<T::Argument_, EitherError> {
    let actual_ty = context.location_type(env, location)?;

    Ok(match &actual_ty {
        LocationType::Fixed(Type::Reference(is_mut, ty)) if *is_mut => {
            check_coin_type(ty)?;
            (
                T::Argument__::new_copy(location),
                Type::Reference(*is_mut, ty.clone()),
            )
        }
        LocationType::Fixed(ty) => {
            check_coin_type(ty)?;
            (
                T::Argument__::Borrow(/* mut */ true, location),
                Type::Reference(true, Rc::new(ty.clone())),
            )
        }
        LocationType::Bytes(_, _) => {
            // TODO we do not currently bytes in any mode as that would require additional type
            // inference not currently supported
            return Err(CommandArgumentError::TypeMismatch.into());
        }
    })
}

fn check_coin_type(ty: &Type) -> Result<(), EitherError> {
    let Type::Datatype(dt) = ty else {
        return Err(CommandArgumentError::TypeMismatch.into());
    };
    let resolved = dt.qualified_ident();
    let is_coin = resolved == RESOLVED_COIN_STRUCT;
    if is_coin {
        Ok(())
    } else {
        Err(CommandArgumentError::TypeMismatch.into())
    }
}

//**************************************************************************************************
// Reference scoping
//**************************************************************************************************

mod scope_references {
    use crate::{
        sp,
        static_programmable_transactions::typing::ast::{self as T, Type},
    };
    use std::collections::BTreeSet;

    /// To mimic proper scoping of references, the last usage of a reference is made a Move instead
    /// of a Copy.
    pub fn transaction(ast: &mut T::Transaction) {
        let mut used: BTreeSet<(u16, u16)> = BTreeSet::new();
        for (c, _tys) in ast.commands.iter_mut().rev() {
            command(&mut used, c);
        }
    }

    fn command(used: &mut BTreeSet<(u16, u16)>, sp!(_, command): &mut T::Command) {
        match command {
            T::Command_::MoveCall(mc) => arguments(used, &mut mc.arguments),
            T::Command_::TransferObjects(objects, recipient) => {
                argument(used, recipient);
                arguments(used, objects);
            }
            T::Command_::SplitCoins(_, coin, amounts) => {
                arguments(used, amounts);
                argument(used, coin);
            }
            T::Command_::MergeCoins(_, target, coins) => {
                arguments(used, coins);
                argument(used, target);
            }
            T::Command_::MakeMoveVec(_, xs) => arguments(used, xs),
            T::Command_::Publish(_, _, _) => (),
            T::Command_::Upgrade(_, _, _, x, _) => argument(used, x),
        }
    }

    fn arguments(used: &mut BTreeSet<(u16, u16)>, args: &mut [T::Argument]) {
        for arg in args.iter_mut().rev() {
            argument(used, arg)
        }
    }

    fn argument(used: &mut BTreeSet<(u16, u16)>, sp!(_, (arg_, ty)): &mut T::Argument) {
        let usage = match arg_ {
            T::Argument__::Use(u) | T::Argument__::Read(u) => u,
            T::Argument__::Borrow(_, _) => return,
        };
        match (&usage, ty) {
            (T::Usage::Move(T::Location::Result(i, j)), Type::Reference(_, _)) => {
                debug_assert!(false, "No reference should be moved at this point");
                used.insert((*i, *j));
            }
            (
                T::Usage::Copy {
                    location: T::Location::Result(i, j),
                    ..
                },
                Type::Reference(_, _),
            ) => {
                // we are at the last usage of a reference result if it was not yet added to the set
                let last_usage = used.insert((*i, *j));
                if last_usage {
                    // if it was the last usage, we need to change the Copy to a Move
                    let loc = T::Location::Result(*i, *j);
                    *usage = T::Usage::Move(loc);
                }
            }
            _ => (),
        }
    }
}
