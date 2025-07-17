// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, rc::Rc};

use crate::{
    execution_mode::ExecutionMode,
    sp,
    static_programmable_transactions::{
        env::Env,
        loading::ast as L,
        typing::{ast as T, verify::input_arguments},
    },
};
use sui_types::{coin::RESOLVED_COIN_STRUCT, error::ExecutionError};

struct Context<'txn> {
    inputs: Vec<&'txn T::InputType>,
    usable_inputs: Vec<bool>,
    result_types: Vec<&'txn [T::Type]>,
}

impl<'txn> Context<'txn> {
    fn new(txn: &'txn T::Transaction) -> Self {
        Self {
            inputs: txn.inputs.iter().map(|(_, ty)| ty).collect(),
            usable_inputs: vec![false; txn.inputs.len()], // set later
            result_types: txn.commands.iter().map(|(_, ty)| ty.as_slice()).collect(),
        }
    }
}

pub fn verify<Mode: ExecutionMode>(env: &Env, txn: &T::Transaction) -> Result<(), ExecutionError> {
    verify_::<Mode>(env, txn).map_err(|e| make_invariant_violation!("{}. Transaction {:?}", e, txn))
}

fn verify_<Mode: ExecutionMode>(env: &Env, txn: &T::Transaction) -> anyhow::Result<()> {
    let mut context = Context::new(txn);
    let T::Transaction { inputs, commands } = txn;
    for (idx, (i, ty)) in inputs.iter().enumerate() {
        input::<Mode>(&mut context, idx, i, ty)?;
    }
    for (c, result_tys) in commands {
        command::<Mode>(env, &context, c, result_tys)?;
    }
    Ok(())
}

fn input<Mode: ExecutionMode>(
    context: &mut Context,
    idx: usize,
    arg: &T::InputArg,
    ty: &T::InputType,
) -> anyhow::Result<()> {
    match arg {
        T::InputArg::Pure(_) => {
            let T::InputType::Bytes(tys) = ty else {
                anyhow::bail!("pure should not have a fixed type",);
            };
            if !Mode::allow_arbitrary_values() {
                for ty in tys.keys() {
                    anyhow::ensure!(
                        input_arguments::is_valid_pure_type(ty)?,
                        "pure type must be valid"
                    );
                }
            }
            context.usable_inputs[idx] = !tys.is_empty();
        }
        T::InputArg::Receiving(_) => {
            let T::InputType::Bytes(tys) = ty else {
                anyhow::bail!("receiving should not have a fixed type",);
            };
            for ty in tys.keys() {
                anyhow::ensure!(
                    input_arguments::is_valid_receiving(ty),
                    "receiving type must be valid"
                );
            }
            context.usable_inputs[idx] = !tys.is_empty();
        }
        T::InputArg::Object(_) => {
            let T::InputType::Fixed(ty) = ty else {
                anyhow::bail!("object should not have a bytes type",);
            };
            anyhow::ensure!(ty.abilities().has_key(), "object type must have key");
            context.usable_inputs[idx] = true
        }
    }
    Ok(())
}

fn command<Mode: ExecutionMode>(
    env: &Env,
    context: &Context,
    sp!(_, c): &T::Command,
    result_tys: &[T::Type],
) -> anyhow::Result<()> {
    match c {
        T::Command_::MoveCall(move_call) => {
            let T::MoveCall {
                function,
                arguments,
            } = &**move_call;
            let L::LoadedFunction { signature, .. } = function;
            let L::LoadedFunctionInstantiation {
                parameters,
                return_,
            } = signature;
            anyhow::ensure!(
                arguments.len() == parameters.len(),
                "arity mismatch. Expected {}, got {}",
                parameters.len(),
                arguments.len()
            );
            for (arg, param) in arguments.iter().zip(parameters) {
                argument(env, context, arg, param)?;
            }
            anyhow::ensure!(
                return_.len() == result_tys.len(),
                "result arity mismatch. Expected {}, got {}",
                return_.len(),
                result_tys.len()
            );
            for (actual, expected) in return_.iter().zip(result_tys) {
                anyhow::ensure!(
                    actual == expected,
                    "return type mismatch. Expected {expected:?}, got {actual:?}"
                );
            }
        }
        T::Command_::TransferObjects(objs, recipient) => {
            for obj in objs {
                let ty = &obj.value.1;
                anyhow::ensure!(
                    ty.abilities().has_key(),
                    "transfer object type must have key, got {ty:?}"
                );
                argument(env, context, obj, ty)?;
            }
            argument(env, context, recipient, &T::Type::Address)?;
            anyhow::ensure!(
                result_tys.is_empty(),
                "transfer objects should not return any value, got {result_tys:?}"
            );
        }
        T::Command_::SplitCoins(ty_coin, coin, amounts) => {
            let T::Type::Datatype(dt) = ty_coin else {
                anyhow::bail!("split coins should have a coin type, got {ty_coin:?}");
            };
            let resolved = dt.qualified_ident();
            anyhow::ensure!(
                resolved == RESOLVED_COIN_STRUCT,
                "split coins should have a coin type, got {resolved:?}"
            );
            argument(
                env,
                context,
                coin,
                &T::Type::Reference(true, Rc::new(ty_coin.clone())),
            )?;
            for amount in amounts {
                argument(env, context, amount, &T::Type::U64)?;
            }
            anyhow::ensure!(
                amounts.len() == result_tys.len(),
                "split coins should return as many values as amounts, expected {} got {}",
                amounts.len(),
                result_tys.len()
            );
            anyhow::ensure!(
                result_tys.iter().all(|t| t == ty_coin),
                "split coins should return coin<{ty_coin:?}>, got {result_tys:?}"
            );
        }
        T::Command_::MergeCoins(ty_coin, target, coins) => {
            let T::Type::Datatype(dt) = ty_coin else {
                anyhow::bail!("split coins should have a coin type, got {ty_coin:?}");
            };
            let resolved = dt.qualified_ident();
            anyhow::ensure!(
                resolved == RESOLVED_COIN_STRUCT,
                "split coins should have a coin type, got {resolved:?}"
            );
            argument(
                env,
                context,
                target,
                &T::Type::Reference(true, Rc::new(ty_coin.clone())),
            )?;
            for coin in coins {
                argument(env, context, coin, ty_coin)?;
            }
            anyhow::ensure!(
                result_tys.is_empty(),
                "merge coins should not return any value, got {result_tys:?}"
            );
        }
        T::Command_::MakeMoveVec(t, args) => {
            for arg in args {
                argument(env, context, arg, t)?;
            }
            anyhow::ensure!(
                result_tys.len() == 1,
                "make move vec should return exactly one vector"
            );
            let T::Type::Vector(inner) = &result_tys[0] else {
                anyhow::bail!("make move vec should return a vector type, got {result_tys:?}");
            };
            anyhow::ensure!(
                t == &inner.element_type,
                "make move vec should return vector<{t:?}>, got {result_tys:?}"
            );
        }
        T::Command_::Publish(_, _, _) => {
            if Mode::packages_are_predefined() {
                anyhow::ensure!(
                    result_tys.is_empty(),
                    "publish should not return upgrade cap for predefined packages"
                );
            } else {
                anyhow::ensure!(
                    result_tys.len() == 1,
                    "publish should return exactly one upgrade cap"
                );
                let cap = &env.upgrade_cap_type()?;
                anyhow::ensure!(
                    cap == &result_tys[0],
                    "publish should return {cap:?}, got {result_tys:?}",
                );
            }
        }
        T::Command_::Upgrade(_, _, _, arg, _) => {
            argument(env, context, arg, &env.upgrade_ticket_type()?)?;
            let receipt = &env.upgrade_receipt_type()?;
            anyhow::ensure!(
                result_tys.len() == 1,
                "upgrade should return exactly one receipt"
            );
            anyhow::ensure!(
                receipt == &result_tys[0],
                "upgrade should return {receipt:?}, got {result_tys:?}"
            );
        }
    }
    Ok(())
}

fn argument(
    env: &Env,
    context: &Context,
    sp!(_, (arg__, ty)): &T::Argument,
    param: &T::Type,
) -> anyhow::Result<()> {
    let t;
    anyhow::ensure!(
        ty == param,
        "argument type mismatch. Expected {param:?}, got {ty:?}"
    );
    let (actual, expected) = match arg__ {
        T::Argument__::Use(u) => (usage(env, context, u)?, param),
        T::Argument__::Read(u) => {
            let actual = usage(env, context, u)?;
            t = match actual {
                LocationType::Fixed(T::Type::Reference(_, inner)) => (*inner).clone(),
                LocationType::Bytes(_) => {
                    anyhow::bail!("should never ReadRef a non-reference location")
                }
                LocationType::Fixed(ty) => {
                    anyhow::bail!("should never ReadRef a non-reference type, got {ty:?}");
                }
            };
            (LocationType::Fixed(t), param)
        }
        T::Argument__::Freeze(u) => {
            let actual = usage(env, context, u)?;
            t = match actual {
                LocationType::Fixed(T::Type::Reference(true, inner)) => {
                    T::Type::Reference(false, inner.clone())
                }
                LocationType::Bytes(_) => {
                    anyhow::bail!("should never FreezeRef a non-reference location")
                }
                LocationType::Fixed(T::Type::Reference(false, _)) => {
                    anyhow::bail!("should never FreezeRef an immutable reference")
                }
                LocationType::Fixed(ty) => {
                    anyhow::bail!("should never Freeze a non-reference type, got {ty:?}");
                }
            };
            (LocationType::Fixed(t), param)
        }
        T::Argument__::Borrow(is_mut, l) => {
            let T::Type::Reference(param_mut, expected) = param else {
                anyhow::bail!("expected a reference type for borrowed location, got {param:?}");
            };
            anyhow::ensure!(
                *param_mut == *is_mut,
                "borrowed location mutability mismatch. Expected {param_mut}, got {is_mut}"
            );
            let actual = location(env, context, *l)?;
            (actual, &**expected)
        }
    };
    // check actual == expected
    match actual {
        LocationType::Bytes(constraints) => {
            anyhow::ensure!(
                constraints.contains_key(expected),
                "Bytes are not constrained for expected type {expected:?}"
            );
        }
        LocationType::Fixed(actual_ty) => {
            anyhow::ensure!(
                &actual_ty == expected,
                "argument type mismatch. Expected {expected:?}, got {actual_ty:?}"
            );
        }
    }
    // check copy usage
    match arg__ {
        T::Argument__::Use(T::Usage::Copy { .. }) | T::Argument__::Read(_) => {
            anyhow::ensure!(
                param.abilities().has_copy(),
                "expected type does not have copy, {expected:?}"
            );
        }
        T::Argument__::Use(T::Usage::Move(_))
        | T::Argument__::Freeze(_)
        | T::Argument__::Borrow(_, _) => (),
    }
    Ok(())
}

fn usage<'a>(env: &Env, context: &Context<'a>, u: &T::Usage) -> anyhow::Result<LocationType<'a>> {
    match u {
        T::Usage::Move(l)
        | T::Usage::Copy {
            location: l,
            borrowed: _,
        } => location(env, context, *l),
    }
}

enum LocationType<'a> {
    Bytes(&'a BTreeMap<T::Type, T::BytesConstraint>),
    Fixed(T::Type),
}

fn location<'a>(
    env: &Env,
    context: &Context<'a>,
    l: T::Location,
) -> anyhow::Result<LocationType<'a>> {
    Ok(LocationType::Fixed(match l {
        T::Location::TxContext => env.tx_context_type()?,
        T::Location::GasCoin => env.gas_coin_type()?,
        T::Location::Input(i) => {
            let usable = context.usable_inputs.get(i as usize).ok_or_else(|| {
                anyhow::anyhow!(
                    "input {i} out of bounds for inputs of length {}",
                    context.inputs.len()
                )
            })?;
            anyhow::ensure!(
                *usable,
                "input {i} is not usable as it does not have constrained Pure bytes"
            );
            let input_ty = context.inputs.get(i as usize).ok_or_else(|| {
                anyhow::anyhow!(
                    "input {i} out of bounds for inputs of length {}",
                    context.inputs.len()
                )
            })?;
            match input_ty {
                T::InputType::Bytes(constraints) => return Ok(LocationType::Bytes(constraints)),
                T::InputType::Fixed(t) => t.clone(),
            }
        }
        T::Location::Result(i, j) => context
            .result_types
            .get(i as usize)
            .and_then(|v| v.get(j as usize))
            .ok_or_else(|| anyhow::anyhow!("result ({i}, {j}) out of bounds",))?
            .clone(),
    }))
}
