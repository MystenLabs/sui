// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{ast as T, env::Env};
use crate::{
    execution_mode::ExecutionMode,
    programmable_transactions::context::EitherError,
    static_programmable_transactions::{
        loading::ast::{self as L, Type},
        spanned::sp,
        typing::ast::BytesConstraint,
    },
};
use indexmap::{IndexMap, IndexSet};
use std::rc::Rc;
use sui_types::{
    base_types::{ObjectRef, TxContextKind},
    coin::RESOLVED_COIN_STRUCT,
    error::{ExecutionError, ExecutionErrorKind, command_argument_error},
    execution_status::CommandArgumentError,
};

#[derive(Debug, Clone, Copy)]
enum SplatLocation {
    GasCoin,
    Input(T::InputIndex),
    Result(u16, u16),
}

#[derive(Debug, Clone, Copy)]
enum InputKind {
    Object,
    Pure,
    Receiving,
}

struct Context {
    current_command: u16,
    /// What kind of input is at each original index
    input_resolution: Vec<InputKind>,
    bytes: IndexSet<Vec<u8>>,
    // Mapping from original index to `bytes`
    bytes_idx_remapping: IndexMap<T::InputIndex, T::ByteIndex>,
    receiving_refs: IndexMap<T::InputIndex, ObjectRef>,
    objects: IndexMap<T::InputIndex, T::ObjectInput>,
    pure: IndexMap<(T::InputIndex, Type), T::PureInput>,
    receiving: IndexMap<(T::InputIndex, Type), T::ReceivingInput>,
    results: Vec<T::ResultType>,
}

impl Context {
    fn new(linputs: L::Inputs) -> Result<Self, ExecutionError> {
        let mut context = Context {
            current_command: 0,
            input_resolution: vec![],
            bytes: IndexSet::new(),
            bytes_idx_remapping: IndexMap::new(),
            receiving_refs: IndexMap::new(),
            objects: IndexMap::new(),
            pure: IndexMap::new(),
            receiving: IndexMap::new(),
            results: vec![],
        };
        // clone inputs for debug assertions
        #[cfg(debug_assertions)]
        let cloned_inputs = linputs
            .iter()
            .map(|(arg, _)| arg.clone())
            .collect::<Vec<_>>();
        // - intern the bytes
        // - build maps for object, pure, and receiving inputs
        for (i, (arg, ty)) in linputs.into_iter().enumerate() {
            let idx = T::InputIndex(i as u16);
            let kind = match (arg, ty) {
                (L::InputArg::Pure(bytes), L::InputType::Bytes) => {
                    let (byte_index, _) = context.bytes.insert_full(bytes);
                    context.bytes_idx_remapping.insert(idx, byte_index);
                    InputKind::Pure
                }
                (L::InputArg::Receiving(oref), L::InputType::Bytes) => {
                    context.receiving_refs.insert(idx, oref);
                    InputKind::Receiving
                }
                (L::InputArg::Object(arg), L::InputType::Fixed(ty)) => {
                    let o = T::ObjectInput {
                        original_input_index: idx,
                        arg,
                        ty,
                    };
                    context.objects.insert(idx, o);
                    InputKind::Object
                }
                (arg, ty) => invariant_violation!(
                    "Input arg, type mismatch. Unexpected {arg:?} with type {ty:?}"
                ),
            };
            context.input_resolution.push(kind);
        }
        #[cfg(debug_assertions)]
        {
            // iterate to check the correctness of bytes interning
            for (i, arg) in cloned_inputs.iter().enumerate() {
                if let L::InputArg::Pure(bytes) = &arg {
                    let idx = T::InputIndex(i as u16);
                    let Some(byte_index) = context.bytes_idx_remapping.get(&idx) else {
                        invariant_violation!("Unbound pure input {}", idx.0);
                    };
                    let Some(interned_bytes) = context.bytes.get_index(*byte_index) else {
                        invariant_violation!("Interned bytes not found for index {}", byte_index);
                    };
                    if interned_bytes != bytes {
                        assert_invariant!(
                            interned_bytes == bytes,
                            "Interned bytes mismatch for input {i}",
                        );
                    }
                }
            }
        }
        Ok(context)
    }

    fn finish(self, commands: T::Commands) -> T::Transaction {
        let Self {
            bytes,
            objects,
            pure,
            receiving,
            ..
        } = self;
        let objects = objects.into_iter().map(|(_, o)| o).collect();
        let pure = pure.into_iter().map(|(_, p)| p).collect();
        let receiving = receiving.into_iter().map(|(_, r)| r).collect();
        T::Transaction {
            bytes,
            objects,
            pure,
            receiving,
            commands,
        }
    }

    // Get the fixed type of a location. Returns `None` for Pure and Receiving inputs,
    fn fixed_type(
        &mut self,
        env: &Env,
        location: SplatLocation,
    ) -> Result<Option<(T::Location, Type)>, ExecutionError> {
        Ok(Some(match location {
            SplatLocation::GasCoin => (T::Location::GasCoin, env.gas_coin_type()?),
            SplatLocation::Result(i, j) => (
                T::Location::Result(i, j),
                self.results[i as usize][j as usize].clone(),
            ),
            SplatLocation::Input(i) => match &self.input_resolution[i.0 as usize] {
                InputKind::Object => {
                    let Some((object_index, _, object_input)) = self.objects.get_full(&i) else {
                        invariant_violation!("Unbound object input {}", i.0)
                    };
                    (
                        T::Location::ObjectInput(object_index as u16),
                        object_input.ty.clone(),
                    )
                }
                InputKind::Pure | InputKind::Receiving => return Ok(None),
            },
        }))
    }

    fn resolve_location(
        &mut self,
        env: &Env,
        location: SplatLocation,
        expected_ty: &Type,
        bytes_constraint: BytesConstraint,
    ) -> Result<(T::Location, Type), ExecutionError> {
        Ok(match location {
            SplatLocation::GasCoin | SplatLocation::Result(_, _) => self
                .fixed_type(env, location)?
                .ok_or_else(|| make_invariant_violation!("Expected fixed type for {location:?}"))?,
            SplatLocation::Input(i) => match &self.input_resolution[i.0 as usize] {
                InputKind::Object => self.fixed_type(env, location)?.ok_or_else(|| {
                    make_invariant_violation!("Expected fixed type for {location:?}")
                })?,
                InputKind::Pure => {
                    let ty = match expected_ty {
                        Type::Reference(_, inner) => (**inner).clone(),
                        ty => ty.clone(),
                    };
                    let k = (i, ty.clone());
                    if !self.pure.contains_key(&k) {
                        let Some(byte_index) = self.bytes_idx_remapping.get(&i).copied() else {
                            invariant_violation!("Unbound pure input {}", i.0);
                        };
                        let pure = T::PureInput {
                            original_input_index: i,
                            byte_index,
                            ty: ty.clone(),
                            constraint: bytes_constraint,
                        };
                        self.pure.insert(k.clone(), pure);
                    }
                    let byte_index = self.pure.get_index_of(&k).unwrap();
                    (T::Location::PureInput(byte_index as u16), ty)
                }
                InputKind::Receiving => {
                    let ty = match expected_ty {
                        Type::Reference(_, inner) => (**inner).clone(),
                        ty => ty.clone(),
                    };
                    let k = (i, ty.clone());
                    if !self.receiving.contains_key(&k) {
                        let Some(object_ref) = self.receiving_refs.get(&i).copied() else {
                            invariant_violation!("Unbound receiving input {}", i.0);
                        };
                        let receiving = T::ReceivingInput {
                            original_input_index: i,
                            object_ref,
                            ty: ty.clone(),
                            constraint: bytes_constraint,
                        };
                        self.receiving.insert(k.clone(), receiving);
                    }
                    let byte_index = self.receiving.get_index_of(&k).unwrap();
                    (T::Location::ReceivingInput(byte_index as u16), ty)
                }
            },
        })
    }
}

pub fn transaction<Mode: ExecutionMode>(
    env: &Env,
    lt: L::Transaction,
) -> Result<T::Transaction, ExecutionError> {
    let L::Transaction { inputs, commands } = lt;
    let mut context = Context::new(inputs)?;
    let commands = commands
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let idx = i as u16;
            context.current_command = idx;
            let (c_, tys) =
                command::<Mode>(env, &mut context, c).map_err(|e| e.with_command_index(i))?;
            context.results.push(tys.clone());
            let c = T::Command_ {
                command: c_,
                result_type: tys,
                // computed later
                drop_values: vec![],
                // computed later
                consumed_shared_objects: vec![],
            };
            Ok(sp(idx, c))
        })
        .collect::<Result<Vec<_>, ExecutionError>>()?;
    let mut ast = context.finish(commands);
    // mark the last usage of references as Move instead of Copy
    scope_references::transaction(&mut ast);
    // mark unused results to be dropped
    unused_results::transaction(&mut ast);
    // track shared object IDs
    consumed_shared_objects::transaction(&mut ast)?;
    Ok(ast)
}

fn command<Mode: ExecutionMode>(
    env: &Env,
    context: &mut Context,
    command: L::Command,
) -> Result<(T::Command__, T::ResultType), ExecutionError> {
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
                    let Some(n_) = function.signature.parameters.len().checked_sub(1) else {
                        invariant_violation!(
                            "A function with a TxContext should have at least one parameter"
                        )
                    };
                    &function.signature.parameters[0..n_]
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
                T::Command__::MoveCall(Box::new(T::MoveCall {
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
            (T::Command__::TransferObjects(objects, address), vec![])
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
            (T::Command__::SplitCoins(coin_type, coin, amounts), result)
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
            (T::Command__::MergeCoins(coin_type, target, coins), vec![])
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
                T::Command__::MakeMoveVec(ty.clone(), elems),
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
                T::Command__::MakeMoveVec(first_ty.clone(), elems),
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
            (T::Command__::Publish(items, object_ids, linkage), result)
        }
        L::Command::Upgrade(items, object_ids, object_id, la, linkage) => {
            let location = one_location(context, 0, la)?;
            let expected_ty = env.upgrade_ticket_type()?;
            let a = argument(env, context, 0, location, expected_ty)?;
            let res = env.upgrade_receipt_type()?;
            (
                T::Command__::Upgrade(items, object_ids, object_id, a, linkage),
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
) -> Result<SplatLocation, ExecutionError> {
    let locs = locations(context, command_arg_idx, vec![arg])?;
    let Ok([loc]): Result<[SplatLocation; 1], _> = locs.try_into() else {
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
) -> Result<Vec<SplatLocation>, ExecutionError>
where
    Items::IntoIter: ExactSizeIterator,
{
    fn splat_arg(
        context: &mut Context,
        res: &mut Vec<SplatLocation>,
        arg: L::Argument,
    ) -> Result<(), EitherError> {
        match arg {
            L::Argument::GasCoin => res.push(SplatLocation::GasCoin),
            L::Argument::Input(i) => {
                if i as usize >= context.input_resolution.len() {
                    return Err(CommandArgumentError::IndexOutOfBounds { idx: i }.into());
                }
                res.push(SplatLocation::Input(T::InputIndex(i)))
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
                res.push(SplatLocation::Result(i, j))
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
                res.extend((0..len).map(|j| SplatLocation::Result(i, j)))
            }
        }
        Ok(())
    }

    let args = args.into_iter();
    let _args_len = args.len();
    let mut res = vec![];
    for (arg_idx, arg) in args.enumerate() {
        splat_arg(context, &mut res, arg).map_err(|e| {
            let Some(idx) = start_idx.checked_add(arg_idx) else {
                return make_invariant_violation!("usize overflow when calculating argument index");
            };
            e.into_execution_error(idx)
        })?
    }
    debug_assert_eq!(res.len(), _args_len);
    Ok(res)
}

fn arguments(
    env: &Env,
    context: &mut Context,
    start_idx: usize,
    locations: Vec<SplatLocation>,
    expected_tys: impl IntoIterator<Item = Type>,
) -> Result<Vec<T::Argument>, ExecutionError> {
    locations
        .into_iter()
        .zip(expected_tys)
        .enumerate()
        .map(|(i, (location, expected_ty))| {
            let Some(idx) = start_idx.checked_add(i) else {
                invariant_violation!("usize overflow when calculating argument index");
            };
            argument(env, context, idx, location, expected_ty)
        })
        .collect()
}

fn argument(
    env: &Env,
    context: &mut Context,
    command_arg_idx: usize,
    location: SplatLocation,
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
    location: SplatLocation,
    expected_ty: &Type,
) -> Result<T::Argument__, EitherError> {
    let current_command = context.current_command;
    let bytes_constraint = BytesConstraint {
        command: current_command,
        argument: command_arg_idx as u16,
    };
    let (location, actual_ty): (T::Location, Type) =
        context.resolve_location(env, location, expected_ty, bytes_constraint)?;
    Ok(match (actual_ty, expected_ty) {
        // Reference location types
        (Type::Reference(a_is_mut, a), Type::Reference(b_is_mut, b)) => {
            let needs_freeze = match (a_is_mut, b_is_mut) {
                // same mutability
                (true, true) | (false, false) => false,
                // mut *can* be used as imm
                (true, false) => true,
                // imm cannot be used as mut
                (false, true) => return Err(CommandArgumentError::TypeMismatch.into()),
            };
            debug_assert!(expected_ty.abilities().has_copy());
            // unused since the type is fixed
            check_type(&a, b)?;
            if needs_freeze {
                T::Argument__::Freeze(T::Usage::new_copy(location))
            } else {
                T::Argument__::new_copy(location)
            }
        }
        (Type::Reference(_, a), b) => {
            check_type(&a, b)?;
            if !b.abilities().has_copy() {
                // TODO this should be a different error for missing copy
                return Err(CommandArgumentError::TypeMismatch.into());
            }
            T::Argument__::Read(T::Usage::new_copy(location))
        }

        // Non reference location types
        (actual_ty, Type::Reference(is_mut, inner)) => {
            check_type(&actual_ty, inner)?;
            T::Argument__::Borrow(/* mut */ *is_mut, location)
        }
        (actual_ty, _) => {
            check_type(&actual_ty, expected_ty)?;
            T::Argument__::Use(if expected_ty.abilities().has_copy() {
                T::Usage::new_copy(location)
            } else {
                T::Usage::new_move(location)
            })
        }
    })
}

fn check_type(actual_ty: &Type, expected_ty: &Type) -> Result<(), CommandArgumentError> {
    if actual_ty == expected_ty {
        Ok(())
    } else {
        Err(CommandArgumentError::TypeMismatch)
    }
}

fn constrained_arguments<P: FnMut(&Type) -> Result<bool, ExecutionError>>(
    env: &Env,
    context: &mut Context,
    start_idx: usize,
    locations: Vec<SplatLocation>,
    mut is_valid: P,
    err_case: CommandArgumentError,
) -> Result<Vec<T::Argument>, ExecutionError> {
    let is_valid = &mut is_valid;
    locations
        .into_iter()
        .enumerate()
        .map(|(i, location)| {
            let Some(idx) = start_idx.checked_add(i) else {
                invariant_violation!("usize overflow when calculating argument index");
            };
            constrained_argument_(env, context, idx, location, is_valid, err_case)
        })
        .collect()
}

fn constrained_argument<P: FnMut(&Type) -> Result<bool, ExecutionError>>(
    env: &Env,
    context: &mut Context,
    command_arg_idx: usize,
    location: SplatLocation,
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
    location: SplatLocation,
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
    location: SplatLocation,
    is_valid: &mut P,
    err_case: CommandArgumentError,
) -> Result<T::Argument_, EitherError> {
    if let Some((location, ty)) = constrained_type(env, context, location, is_valid)? {
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
    location: SplatLocation,
    mut is_valid: P,
) -> Result<Option<(T::Location, Type)>, ExecutionError> {
    let Some((location, ty)) = context.fixed_type(env, location)? else {
        return Ok(None);
    };
    Ok(if is_valid(&ty)? {
        Some((location, ty))
    } else {
        None
    })
}

fn coin_mut_ref_argument(
    env: &Env,
    context: &mut Context,
    command_arg_idx: usize,
    location: SplatLocation,
) -> Result<T::Argument, ExecutionError> {
    let arg_ = coin_mut_ref_argument_(env, context, location)
        .map_err(|e| e.into_execution_error(command_arg_idx))?;
    Ok(sp(command_arg_idx as u16, arg_))
}

fn coin_mut_ref_argument_(
    env: &Env,
    context: &mut Context,
    location: SplatLocation,
) -> Result<T::Argument_, EitherError> {
    let Some((location, actual_ty)) = context.fixed_type(env, location)? else {
        // TODO we do not currently bytes in any mode as that would require additional type
        // inference not currently supported
        return Err(CommandArgumentError::TypeMismatch.into());
    };
    Ok(match &actual_ty {
        Type::Reference(is_mut, ty) if *is_mut => {
            check_coin_type(ty)?;
            (
                T::Argument__::new_copy(location),
                Type::Reference(*is_mut, ty.clone()),
            )
        }
        ty => {
            check_coin_type(ty)?;
            (
                T::Argument__::Borrow(/* mut */ true, location),
                Type::Reference(true, Rc::new(ty.clone())),
            )
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
        for c in ast.commands.iter_mut().rev() {
            command(&mut used, c);
        }
    }

    fn command(used: &mut BTreeSet<(u16, u16)>, sp!(_, c): &mut T::Command) {
        match &mut c.command {
            T::Command__::MoveCall(mc) => arguments(used, &mut mc.arguments),
            T::Command__::TransferObjects(objects, recipient) => {
                argument(used, recipient);
                arguments(used, objects);
            }
            T::Command__::SplitCoins(_, coin, amounts) => {
                arguments(used, amounts);
                argument(used, coin);
            }
            T::Command__::MergeCoins(_, target, coins) => {
                arguments(used, coins);
                argument(used, target);
            }
            T::Command__::MakeMoveVec(_, xs) => arguments(used, xs),
            T::Command__::Publish(_, _, _) => (),
            T::Command__::Upgrade(_, _, _, x, _) => argument(used, x),
        }
    }

    fn arguments(used: &mut BTreeSet<(u16, u16)>, args: &mut [T::Argument]) {
        for arg in args.iter_mut().rev() {
            argument(used, arg)
        }
    }

    fn argument(used: &mut BTreeSet<(u16, u16)>, sp!(_, (arg_, ty)): &mut T::Argument) {
        let usage = match arg_ {
            T::Argument__::Use(u) | T::Argument__::Read(u) | T::Argument__::Freeze(u) => u,
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

//**************************************************************************************************
// Unused results
//**************************************************************************************************

mod unused_results {
    use indexmap::IndexSet;

    use crate::{sp, static_programmable_transactions::typing::ast as T};

    /// Finds what `Result` indexes are never used in the transaction.
    /// For each command, marks the indexes of result values with `drop` that are never referred to
    /// via `Result`.
    pub fn transaction(ast: &mut T::Transaction) {
        // Collect all used result locations (i, j) across all commands
        let mut used: IndexSet<(u16, u16)> = IndexSet::new();
        for c in &ast.commands {
            command(&mut used, c);
        }

        // For each command, mark unused result indexes with `drop`
        for (i, sp!(_, c)) in ast.commands.iter_mut().enumerate() {
            debug_assert!(c.drop_values.is_empty());
            let i = i as u16;
            c.drop_values = c
                .result_type
                .iter()
                .enumerate()
                .map(|(j, ty)| (j as u16, ty))
                .map(|(j, ty)| ty.abilities().has_drop() && !used.contains(&(i, j)))
                .collect();
        }
    }

    fn command(used: &mut IndexSet<(u16, u16)>, sp!(_, c): &T::Command) {
        match &c.command {
            T::Command__::MoveCall(mc) => arguments(used, &mc.arguments),
            T::Command__::TransferObjects(objects, recipient) => {
                argument(used, recipient);
                arguments(used, objects);
            }
            T::Command__::SplitCoins(_, coin, amounts) => {
                arguments(used, amounts);
                argument(used, coin);
            }
            T::Command__::MergeCoins(_, target, coins) => {
                arguments(used, coins);
                argument(used, target);
            }
            T::Command__::MakeMoveVec(_, elements) => arguments(used, elements),
            T::Command__::Publish(_, _, _) => (),
            T::Command__::Upgrade(_, _, _, x, _) => argument(used, x),
        }
    }

    fn arguments(used: &mut IndexSet<(u16, u16)>, args: &[T::Argument]) {
        for arg in args {
            argument(used, arg)
        }
    }

    fn argument(used: &mut IndexSet<(u16, u16)>, sp!(_, (arg_, _)): &T::Argument) {
        if let T::Location::Result(i, j) = arg_.location() {
            used.insert((i, j));
        }
    }
}

//**************************************************************************************************
// consumed shared object IDs
//**************************************************************************************************

mod consumed_shared_objects {

    use crate::{
        sp, static_programmable_transactions::loading::ast as L,
        static_programmable_transactions::typing::ast as T,
    };
    use sui_types::{base_types::ObjectID, error::ExecutionError};

    // Shared object (non-party) IDs contained in each location
    struct Context {
        // (legacy) shared object IDs that are used as inputs
        inputs: Vec<Option<ObjectID>>,
        results: Vec<Vec<Option<Vec<ObjectID>>>>,
    }

    impl Context {
        pub fn new(ast: &T::Transaction) -> Self {
            let T::Transaction {
                bytes: _,
                objects,
                pure: _,
                receiving: _,
                commands: _,
            } = ast;
            let inputs = objects
                .iter()
                .map(|o| match &o.arg {
                    L::ObjectArg::SharedObject {
                        id,
                        kind: L::SharedObjectKind::Legacy,
                        ..
                    } => Some(*id),
                    L::ObjectArg::ImmObject(_)
                    | L::ObjectArg::OwnedObject(_)
                    | L::ObjectArg::SharedObject {
                        kind: L::SharedObjectKind::Party,
                        ..
                    } => None,
                })
                .collect::<Vec<_>>();
            Self {
                inputs,
                results: vec![],
            }
        }
    }

    /// Finds what shared objects are taken by-value by each command and must be either
    /// deleted or re-shared.
    /// MakeMoveVec is the only command that can take shared objects by-value and propagate them
    /// for another command.
    pub fn transaction(ast: &mut T::Transaction) -> Result<(), ExecutionError> {
        let mut context = Context::new(ast);

        // For each command, find what shared objects are taken by-value and mark them as being
        // consumed
        for c in &mut ast.commands {
            debug_assert!(c.value.consumed_shared_objects.is_empty());
            command(&mut context, c)?;
            debug_assert!(context.results.last().unwrap().len() == c.value.result_type.len());
        }
        Ok(())
    }

    fn command(context: &mut Context, sp!(_, c): &mut T::Command) -> Result<(), ExecutionError> {
        let mut acc = vec![];
        match &c.command {
            T::Command__::MoveCall(mc) => arguments(context, &mut acc, &mc.arguments),
            T::Command__::TransferObjects(objects, recipient) => {
                argument(context, &mut acc, recipient);
                arguments(context, &mut acc, objects);
            }
            T::Command__::SplitCoins(_, coin, amounts) => {
                arguments(context, &mut acc, amounts);
                argument(context, &mut acc, coin);
            }
            T::Command__::MergeCoins(_, target, coins) => {
                arguments(context, &mut acc, coins);
                argument(context, &mut acc, target);
            }
            T::Command__::MakeMoveVec(_, elements) => arguments(context, &mut acc, elements),
            T::Command__::Publish(_, _, _) => (),
            T::Command__::Upgrade(_, _, _, x, _) => argument(context, &mut acc, x),
        }
        let (consumed, result) = match &c.command {
            // make move vec does not "consume" any by-value shared objects, and can propagate
            // them to a later command
            T::Command__::MakeMoveVec(_, _) => {
                assert_invariant!(
                    c.result_type.len() == 1,
                    "MakeMoveVec must return a single value"
                );
                (vec![], vec![Some(acc)])
            }
            // these commands do not propagate shared objects, and consume any in the acc
            T::Command__::MoveCall(_)
            | T::Command__::TransferObjects(_, _)
            | T::Command__::SplitCoins(_, _, _)
            | T::Command__::MergeCoins(_, _, _)
            | T::Command__::Publish(_, _, _)
            | T::Command__::Upgrade(_, _, _, _, _) => (acc, vec![None; c.result_type.len()]),
        };
        c.consumed_shared_objects = consumed;
        context.results.push(result);
        Ok(())
    }

    fn arguments(context: &mut Context, acc: &mut Vec<ObjectID>, args: &[T::Argument]) {
        for arg in args {
            argument(context, acc, arg)
        }
    }

    fn argument(context: &mut Context, acc: &mut Vec<ObjectID>, sp!(_, (arg_, _)): &T::Argument) {
        let T::Argument__::Use(T::Usage::Move(loc)) = arg_ else {
            // only Move usage can take shared objects by-value since they cannot be copied
            return;
        };
        match loc {
            // no shared objects in these locations
            T::Location::TxContext
            | T::Location::GasCoin
            | T::Location::PureInput(_)
            | T::Location::ReceivingInput(_) => (),
            T::Location::ObjectInput(i) => {
                if let Some(id) = context.inputs[*i as usize] {
                    acc.push(id);
                }
            }

            T::Location::Result(i, j) => {
                if let Some(ids) = &context.results[*i as usize][*j as usize] {
                    acc.extend(ids.iter().copied());
                }
            }
        }
    }
}
