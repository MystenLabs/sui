// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{ast as T, env::Env};
use crate::programmable_transactions::context::EitherError;
use move_core_types::language_storage::StructTag;
use move_vm_types::loaded_data::runtime_types::{CachedDatatype, Type};
use std::collections::{BTreeMap, BTreeSet};
use sui_types::{
    coin::{COIN_MODULE_NAME, COIN_STRUCT_NAME},
    error::{command_argument_error, ExecutionError},
    execution_status::CommandArgumentError,
    transaction::{self as P, CallArg, ObjectArg},
    SUI_FRAMEWORK_ADDRESS,
};

struct Context {
    gathered_input_types: BTreeMap<u16, BTreeSet<Type>>,
    inputs: Vec<(CallArg, InputType)>,
    results: Vec<T::ResultType>,
}

enum InputType {
    Bytes,
    Fixed(Type),
}

enum LocationType<'env, 'context: 'env> {
    Bytes(&'context mut InputType, &'context mut BTreeSet<Type>),
    Fixed(&'env Type),
}

impl Context {
    fn new(env: &Env, input_args: Vec<CallArg>) -> Result<Self, ExecutionError> {
        let mut context = Context {
            gathered_input_types: BTreeMap::new(),
            inputs: vec![],
            results: vec![],
        };
        context.inputs = input_args
            .into_iter()
            .enumerate()
            .map(|(i, arg)| {
                let idx = i as u16;
                let ty = match &arg {
                    CallArg::Pure(_) | CallArg::Object(ObjectArg::Receiving(_)) => {
                        context.gathered_input_types.insert(idx, BTreeSet::new());
                        InputType::Bytes
                    }
                    CallArg::Object(
                        ObjectArg::ImmOrOwnedObject((id, _, _))
                        | ObjectArg::SharedObject { id, .. },
                    ) => {
                        let obj = env.read_object(&id)?;
                        let Some(ty) = obj.type_() else {
                            invariant_violation!("Object {:?} has does not have a Move type", id);
                        };
                        let tag: StructTag = ty.clone().into();
                        let ty = env.load_type_from_struct(&tag)?;
                        InputType::Fixed(ty)
                    }
                };
                Ok((arg, ty))
            })
            .collect::<Result<Vec<_>, ExecutionError>>()?;
        Ok(context)
    }

    fn finish(self) -> Vec<(CallArg, T::InputType)> {
        let Self {
            mut gathered_input_types,
            inputs,
            ..
        } = self;
        inputs
            .into_iter()
            .enumerate()
            .map(|(i, (arg, ty))| match (&arg, ty) {
                (CallArg::Pure(_) | CallArg::Object(ObjectArg::Receiving(_)), _) => {
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

    fn location_type<'env, 'context: 'env>(
        &'context mut self,
        env: &'env Env,
        location: T::Location,
    ) -> Result<LocationType<'env, 'context>, ExecutionError> {
        Ok(match location {
            T::Location::GasCoin => LocationType::Fixed(env.gas_coin_type()?),
            T::Location::Input(i) => match &mut self.inputs[i as usize].1 {
                t @ InputType::Bytes => {
                    LocationType::Bytes(t, self.gathered_input_types.get_mut(&i).unwrap())
                }
                InputType::Fixed(t) => LocationType::Fixed(t),
            },
            T::Location::Result(i, j) => LocationType::Fixed(&self.results[i as usize][j as usize]),
        })
    }
}

pub fn translate(
    env: &Env,
    pt: P::ProgrammableTransaction,
) -> Result<T::Transaction, ExecutionError> {
    let P::ProgrammableTransaction { inputs, commands } = pt;
    let mut context = Context::new(env, inputs)?;
    let commands = commands
        .into_iter()
        .enumerate()
        .map(|(i, c)| command(env, &mut context, c).map_err(|e| e.with_command_index(i)))
        .collect::<Result<Vec<_>, _>>()?;
    let inputs = context.finish();
    Ok(T::Transaction { inputs, commands })
}

fn command(
    env: &Env,
    context: &mut Context,
    command: P::Command,
) -> Result<(T::Command, T::ResultType), ExecutionError> {
    Ok(match command {
        P::Command::MoveCall(pmc) => {
            let P::ProgrammableMoveCall {
                package,
                module,
                function: name,
                type_arguments: ptype_arguments,
                arguments: pargs,
            } = *pmc;
            let type_arguments = ptype_arguments
                .into_iter()
                .map(|ty| env.load_type_input(ty))
                .collect::<Result<Vec<_>, _>>()?;
            let function = env.load_function(package, module, name, type_arguments)?;
            let arg_locs = locations(context, 0, pargs)?;
            let args = arguments(env, context, 0, arg_locs, &function.signature.parameters)?;
            let result = function.signature.return_.clone();
            (
                T::Command::MoveCall(Box::new(T::MoveCall {
                    function,
                    arguments: args,
                })),
                result,
            )
        }
        P::Command::TransferObjects(pobjects, paddress) => {
            let object_locs = locations(context, 0, pobjects)?;
            let address_loc = one_location(context, object_locs.len(), paddress)?;
            let objects = object_arguments(env, context, 0, object_locs)?;
            let address = argument(env, context, objects.len(), address_loc, &Type::Address)?;
            (T::Command::TransferObjects(objects, address), vec![])
        }
        P::Command::SplitCoins(pcoin, pamounts) => {
            let coin_loc = one_location(context, 0, pcoin)?;
            let amount_locs = locations(context, 1, pamounts)?;
            let (coin_type, coin) = coin_mut_ref_argument(env, context, 0, coin_loc)?;
            let amounts = arguments(env, context, 1, amount_locs, std::iter::repeat(&Type::U64))?;
            let result = vec![coin_type.clone(); amounts.len()];
            (
                T::Command::SplitCoins(coin_type.clone(), coin, amounts),
                result,
            )
        }
        P::Command::MergeCoins(ptarget, pcoins) => {
            let target_loc = one_location(context, 0, ptarget)?;
            let coin_locs = locations(context, 1, pcoins)?;
            let (coin_type, target) = coin_mut_ref_argument(env, context, 0, target_loc)?;
            let coins = arguments(env, context, 1, coin_locs, std::iter::repeat(&coin_type))?;
            (T::Command::MergeCoins(coin_type, target, coins), vec![])
        }
        P::Command::MakeMoveVec(Some(pty), pelems) => {
            let ty = env.load_type_input(pty)?;
            let elem_locs = locations(context, 0, pelems)?;
            let elems = arguments(env, context, 0, elem_locs, std::iter::repeat(&ty))?;
            (
                T::Command::MakeMoveVec(ty.clone(), elems),
                vec![Type::Vector(Box::new(ty))],
            )
        }
        P::Command::MakeMoveVec(None, pelems) => {
            let mut pelems = pelems.into_iter();
            let Some(pfirst) = pelems.next() else {
                // TODO maybe this should be a different errors for CLI usage
                invariant_violation!(
                    "input checker ensures if args are empty, there is a type specified"
                );
            };
            let first_loc = one_location(context, 0, pfirst)?;
            let Some(first_ty) = object_type(env, context, first_loc)?.cloned() else {
                // TODO need a new error here
                return Err(command_argument_error(
                    CommandArgumentError::TypeMismatch,
                    0,
                ));
            };
            let elems_loc = locations(context, 1, pelems)?;
            let elems = arguments(env, context, 1, elems_loc, std::iter::repeat(&first_ty))?;
            (
                T::Command::MakeMoveVec(first_ty.clone(), elems),
                vec![Type::Vector(Box::new(first_ty))],
            )
        }
        P::Command::Publish(items, object_ids) => (T::Command::Publish(items, object_ids), vec![]),
        P::Command::Upgrade(items, object_ids, object_id, pa) => {
            let location = one_location(context, 0, pa)?;
            let expected_ty = env.upgrade_ticket_type()?;
            let a = argument(env, context, 0, location, expected_ty)?;
            let res = env.upgrade_receipt_type()?;
            (
                T::Command::Upgrade(items, object_ids, object_id, a),
                vec![res.clone()],
            )
        }
    })
}

fn one_location(
    context: &mut Context,
    command_arg_idx: usize,
    arg: P::Argument,
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

fn locations<Items: IntoIterator<Item = P::Argument>>(
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
        arg: P::Argument,
    ) -> Result<(), EitherError> {
        match arg {
            P::Argument::GasCoin => res.push(T::Location::GasCoin),
            P::Argument::Input(i) => {
                if i as usize >= context.inputs.len() {
                    return Err(CommandArgumentError::IndexOutOfBounds { idx: i }.into());
                }
                res.push(T::Location::Input(i))
            }
            P::Argument::NestedResult(i, j) => {
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
            P::Argument::Result(i) => {
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

fn arguments<'a>(
    env: &Env,
    context: &mut Context,
    start_idx: usize,
    locations: Vec<T::Location>,
    expected_tys: impl IntoIterator<Item = &'a Type>,
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
    expected_ty: &Type,
) -> Result<T::Argument, ExecutionError> {
    argument_(env, context, location, expected_ty)
        .map_err(|e| e.into_execution_error(command_arg_idx))
}

fn argument_(
    env: &Env,
    context: &mut Context,
    location: T::Location,
    expected_ty: &Type,
) -> Result<T::Argument, EitherError> {
    let actual_ty = context.location_type(env, location)?;

    Ok(match (&actual_ty, expected_ty) {
        (LocationType::Fixed(Type::Reference(a)), Type::Reference(b))
        | (LocationType::Fixed(Type::MutableReference(a)), Type::Reference(b))
        | (LocationType::Fixed(Type::MutableReference(a)), Type::MutableReference(b)) => {
            check_type(LocationType::Fixed(a), b)?;
            T::Argument::Copy(location)
        }
        (_, Type::Reference(inner)) => {
            check_type(actual_ty, inner)?;
            T::Argument::Borrow(/* mut */ false, location)
        }
        (_, Type::MutableReference(inner)) => {
            fix_type(actual_ty, inner)?;
            T::Argument::Borrow(/* mut */ true, location)
        }
        _ => {
            check_type(actual_ty, expected_ty)?;
            if env.abilities(expected_ty)?.has_copy() {
                T::Argument::Copy(location)
            } else {
                T::Argument::Move(location)
            }
        }
    })
}

fn fix_type(actual_ty: LocationType, expected_ty: &Type) -> Result<(), CommandArgumentError> {
    check_type_impl(/* fix */ true, actual_ty, expected_ty)
}

fn check_type(actual_ty: LocationType, expected_ty: &Type) -> Result<(), CommandArgumentError> {
    check_type_impl(/* fix */ false, actual_ty, expected_ty)
}

fn check_type_impl(
    fix: bool,
    actual_ty: LocationType,
    expected_ty: &Type,
) -> Result<(), CommandArgumentError> {
    match actual_ty {
        LocationType::Bytes(ty, types) => {
            types.insert(expected_ty.clone());
            if fix {
                *ty = InputType::Fixed(expected_ty.clone());
            }
            // validity of pure types is checked elsewhere
            Ok(())
        }
        LocationType::Fixed(actual_ty) => {
            if actual_ty == expected_ty {
                Ok(())
            } else {
                Err(CommandArgumentError::TypeMismatch)
            }
        }
    }
}

fn object_arguments(
    env: &Env,
    context: &mut Context,
    start_idx: usize,
    locations: Vec<T::Location>,
) -> Result<Vec<T::Argument>, ExecutionError> {
    locations
        .into_iter()
        .enumerate()
        .map(|(i, location)| object_argument(env, context, start_idx + i, location))
        .collect()
}

fn object_argument(
    env: &Env,
    context: &mut Context,
    command_arg_idx: usize,
    location: T::Location,
) -> Result<T::Argument, ExecutionError> {
    object_argument_(env, context, location).map_err(|e| e.into_execution_error(command_arg_idx))
}

fn object_argument_(
    env: &Env,
    context: &mut Context,
    location: T::Location,
) -> Result<T::Argument, EitherError> {
    if object_type(env, context, location)?.is_some() {
        Ok(T::Argument::Move(location))
    } else {
        Err(CommandArgumentError::TypeMismatch.into())
    }
}

fn object_type<'a>(
    env: &'a Env,
    context: &'a mut Context,
    location: T::Location,
) -> Result<Option<&'a Type>, ExecutionError> {
    let LocationType::Fixed(ty) = context.location_type(env, location)? else {
        return Ok(None);
    };
    let abilities = env.abilities(ty)?;
    Ok(if abilities.has_key() { Some(ty) } else { None })
}

fn coin_mut_ref_argument(
    env: &Env,
    context: &mut Context,
    command_arg_idx: usize,
    location: T::Location,
) -> Result<(Type, T::Argument), ExecutionError> {
    coin_mut_ref_argument_(env, context, location)
        .map_err(|e| e.into_execution_error(command_arg_idx))
}

fn coin_mut_ref_argument_(
    env: &Env,
    context: &mut Context,
    location: T::Location,
) -> Result<(Type, T::Argument), EitherError> {
    let actual_ty = context.location_type(env, location)?;

    Ok(match &actual_ty {
        LocationType::Fixed(Type::MutableReference(ty)) => {
            let ty = check_coin_type(env, ty)?;
            (ty, T::Argument::Copy(location))
        }
        LocationType::Fixed(ty) => {
            let ty = check_coin_type(env, ty)?;
            (ty, T::Argument::Borrow(/* mut */ true, location))
        }
        LocationType::Bytes(_, _) => {
            // TODO we do not currently bytes in any mode as that would require additional type
            // inference not currently supported
            return Err(CommandArgumentError::TypeMismatch.into());
        }
    })
}

fn check_coin_type(env: &Env, ty: &Type) -> Result<Type, EitherError> {
    let Type::DatatypeInstantiation(inst_tys) = ty else {
        return Err(CommandArgumentError::TypeMismatch.into());
    };
    let (inst, _tys) = &**inst_tys;
    let datatype = env.datatype(*inst)?;
    let datatype: &CachedDatatype = datatype.as_ref();
    let is_coin = datatype.defining_id.address() == &SUI_FRAMEWORK_ADDRESS
        && datatype.defining_id.name() == COIN_MODULE_NAME
        && datatype.name.as_ident_str() == COIN_STRUCT_NAME;
    // is_coin ==> ty.defining_id == ty.runtime_id
    debug_assert!(!is_coin || datatype.defining_id == datatype.runtime_id);
    if is_coin {
        Ok(ty.clone())
    } else {
        Err(CommandArgumentError::TypeMismatch.into())
    }
}
