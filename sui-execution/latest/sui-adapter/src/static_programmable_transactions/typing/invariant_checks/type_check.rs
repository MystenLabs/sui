// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::rc::Rc;

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

struct Context<'pc, 'vm, 'state, 'linkage, 'env, 'txn> {
    env: &'env Env<'pc, 'vm, 'state, 'linkage>,
    inputs: Vec<&'txn T::InputType>,
    usable_inputs: Vec<bool>,
    result_types: Vec<&'txn [T::Type]>,
}

impl<'pc, 'vm, 'state, 'linkage, 'env, 'txn> Context<'pc, 'vm, 'state, 'linkage, 'env, 'txn> {
    fn new(env: &'env Env<'pc, 'vm, 'state, 'linkage>, txn: &'txn T::Transaction) -> Self {
        Self {
            env,
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
    let mut context = Context::new(env, txn);
    let T::Transaction { inputs, commands } = txn;
    for (idx, (i, ty)) in inputs.iter().enumerate() {
        input::<Mode>(&mut context, idx, i, ty)?;
    }
    for (c, result_tys) in commands {
        command::<Mode>(&context, c, result_tys)?;
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
                argument(context, arg, param)?;
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
                argument(context, obj, ty)?;
            }
            argument(context, recipient, &T::Type::Address)?;
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
                context,
                coin,
                &T::Type::Reference(true, Rc::new(ty_coin.clone())),
            )?;
            for amount in amounts {
                argument(context, amount, &T::Type::U64)?;
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
                context,
                target,
                &T::Type::Reference(true, Rc::new(ty_coin.clone())),
            )?;
            for coin in coins {
                argument(context, coin, ty_coin)?;
            }
            anyhow::ensure!(
                result_tys.is_empty(),
                "merge coins should not return any value, got {result_tys:?}"
            );
        }
        T::Command_::MakeMoveVec(t, args) => {
            for arg in args {
                argument(context, arg, t)?;
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
                let cap = &context.env.upgrade_cap_type()?;
                anyhow::ensure!(
                    cap == &result_tys[0],
                    "publish should return {cap:?}, got {result_tys:?}",
                );
            }
        }
        T::Command_::Upgrade(_, _, _, arg, _) => {
            argument(context, arg, &context.env.upgrade_ticket_type()?)?;
            let receipt = &context.env.upgrade_receipt_type()?;
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
    context: &Context,
    sp!(_, (arg__, ty)): &T::Argument,
    param: &T::Type,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        ty == param,
        "argument type mismatch. Expected {param:?}, got {ty:?}"
    );
    match arg__ {
        T::Argument__::Use(u) => usage(context, u, param)?,
        T::Argument__::Read(u) => usage(
            context,
            u,
            &T::Type::Reference(false, Rc::new(param.clone())),
        )?,
        T::Argument__::Borrow(is_mut, l) => {
            let T::Type::Reference(param_mut, inner_param) = param else {
                anyhow::bail!("expected a referenc type for borrowed location, got {param:?}");
            };
            anyhow::ensure!(
                *param_mut == *is_mut,
                "borrowed location mutability mismatch. Expected {param_mut}, got {is_mut}"
            );
            location(context, *l, inner_param)?;
        }
    }
    Ok(())
}

fn usage(context: &Context, u: &T::Usage, expected: &T::Type) -> anyhow::Result<()> {
    match u {
        T::Usage::Move(l) => location(context, *l, expected),
        T::Usage::Copy {
            location: l,
            borrowed: _,
        } => {
            anyhow::ensure!(
                expected.abilities().has_copy(),
                "expected a copyable type for copied location, got {expected:?}"
            );
            location(context, *l, expected)
        }
    }
}

fn location(context: &Context, l: T::Location, expected: &T::Type) -> anyhow::Result<()> {
    let t;
    let actual = match l {
        T::Location::TxContext => {
            t = context.env.tx_context_type()?;
            &t
        }
        T::Location::GasCoin => {
            t = context.env.gas_coin_type()?;
            &t
        }
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
                T::InputType::Bytes(constraints) => {
                    anyhow::ensure!(
                        constraints.contains_key(expected),
                        "input {i} does not have the expected type {expected:?}, got {constraints:?}"
                    );
                    return Ok(());
                }
                T::InputType::Fixed(t) => t,
            }
        }
        T::Location::Result(i, j) => context
            .result_types
            .get(i as usize)
            .and_then(|v| v.get(j as usize))
            .ok_or_else(|| anyhow::anyhow!("result ({i}, {j}) out of bounds",))?,
    };
    let (actual, expected) = match (actual, expected) {
        (T::Type::Reference(a_is_mut, a_inner), T::Type::Reference(e_is_mut, e_inner)) => {
            // for reference subtyping
            // expected is mut ==> actual must be mut
            anyhow::ensure!(
                !e_is_mut || *a_is_mut,
                "reference mutability incompatibility. Expected {e_is_mut}, got {a_is_mut}"
            );
            (&**a_inner, &**e_inner)
        }
        (a, e) => (a, e),
    };
    anyhow::ensure!(
        actual == expected,
        "location type mismatch. Expected {expected:?}, got {actual:?}"
    );
    Ok(())
}
