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
use sui_types::{
    coin::RESOLVED_COIN_STRUCT, error::ExecutionError,
    funds_accumulator::RESOLVED_WITHDRAWAL_STRUCT,
};

struct Context<'txn> {
    objects: Vec<&'txn T::Type>,
    withdrawals: Vec<&'txn T::Type>,
    pure: Vec<&'txn T::Type>,
    receiving: Vec<&'txn T::Type>,
    result_types: Vec<&'txn [T::Type]>,
}

impl<'txn> Context<'txn> {
    fn new(txn: &'txn T::Transaction) -> Self {
        Self {
            objects: txn.objects.iter().map(|o| &o.ty).collect(),
            withdrawals: txn.withdrawals.iter().map(|w| &w.ty).collect(),
            pure: txn.pure.iter().map(|p| &p.ty).collect(),
            receiving: txn.receiving.iter().map(|r| &r.ty).collect(),
            result_types: txn
                .commands
                .iter()
                .map(|sp!(_, c)| c.result_type.as_slice())
                .collect(),
        }
    }
}

/// Verifies the correctness of the typing on the AST
/// - All object inputs have key
/// - All pure inputs are valid types
/// - All receiving inputs types have key
/// - All commands are well formed with correct argument/result types
/// - All dropped result values have the `drop` ability
pub fn verify<Mode: ExecutionMode>(env: &Env, txn: &T::Transaction) -> Result<(), ExecutionError> {
    verify_::<Mode>(env, txn).map_err(|e| make_invariant_violation!("{}. Transaction {:?}", e, txn))
}

fn verify_<Mode: ExecutionMode>(env: &Env, txn: &T::Transaction) -> anyhow::Result<()> {
    let context = Context::new(txn);
    let T::Transaction {
        bytes: _,
        objects,
        withdrawals,
        pure,
        receiving,
        commands,
    } = txn;
    for obj in objects {
        object_input(obj)?;
    }
    for w in withdrawals {
        withdrawal_input(&w.ty)?;
    }
    for p in pure {
        pure_input::<Mode>(p)?;
    }
    for r in receiving {
        receiving_input(r)?;
    }
    for c in commands {
        command::<Mode>(env, &context, c)?;
    }
    Ok(())
}

fn object_input(obj: &T::ObjectInput) -> anyhow::Result<()> {
    anyhow::ensure!(obj.ty.abilities().has_key(), "object type must have key");
    Ok(())
}

fn withdrawal_input(ty: &T::Type) -> anyhow::Result<()> {
    anyhow::ensure!(ty.abilities().has_drop(), "withdrawal type must have drop");
    let T::Type::Datatype(dt) = ty else {
        anyhow::bail!("withdrawal input must be a datatype, got {ty:?}");
    };
    anyhow::ensure!(
        dt.type_arguments.len() == 1,
        "withdrawal input must have exactly one type argument, got {}",
        dt.type_arguments.len()
    );
    anyhow::ensure!(
        dt.qualified_ident() == RESOLVED_WITHDRAWAL_STRUCT,
        "withdrawal input must be sui::funds_accumulator::Withdrawal, got {:?}",
        dt.qualified_ident()
    );
    Ok(())
}

fn pure_input<Mode: ExecutionMode>(p: &T::PureInput) -> anyhow::Result<()> {
    if !Mode::allow_arbitrary_values() {
        anyhow::ensure!(
            input_arguments::is_valid_pure_type(&p.ty)?,
            "pure type must be valid"
        );
    }
    Ok(())
}

fn receiving_input(r: &T::ReceivingInput) -> anyhow::Result<()> {
    anyhow::ensure!(
        input_arguments::is_valid_receiving(&r.ty),
        "receiving type must be valid"
    );
    Ok(())
}

fn command<Mode: ExecutionMode>(
    env: &Env,
    context: &Context,
    sp!(_, c): &T::Command,
) -> anyhow::Result<()> {
    let result_tys = &c.result_type;
    match &c.command {
        T::Command__::MoveCall(move_call) => {
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
        T::Command__::TransferObjects(objs, recipient) => {
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
        T::Command__::SplitCoins(ty_coin, coin, amounts) => {
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
        T::Command__::MergeCoins(ty_coin, target, coins) => {
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
        T::Command__::MakeMoveVec(t, args) => {
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
        T::Command__::Publish(_, _, _) => {
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
        T::Command__::Upgrade(_, _, _, arg, _) => {
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
    assert_invariant!(
        c.drop_values.len() == result_tys.len(),
        "drop values should match result types, expected {} got {}",
        c.drop_values.len(),
        result_tys.len()
    );
    for (drop_value, result_ty) in c.drop_values.iter().copied().zip(result_tys) {
        // drop value ==> `ty: drop`
        assert_invariant!(
            !drop_value || result_ty.abilities().has_drop(),
            "result was marked for drop but does not have the `drop` ability"
        );
    }
    Ok(())
}

fn argument(
    env: &Env,
    context: &Context,
    sp!(_, (arg__, ty)): &T::Argument,
    param: &T::Type,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        ty == param,
        "argument type mismatch. Expected {param:?}, got {ty:?}"
    );
    let (actual, expected) = match arg__ {
        T::Argument__::Use(u) => (usage(env, context, u)?, param),
        T::Argument__::Read(u) => {
            let actual = match usage(env, context, u)? {
                T::Type::Reference(_, inner) => (*inner).clone(),
                _ => {
                    anyhow::bail!("should never ReadRef a non-reference type, got {ty:?}");
                }
            };
            (actual, param)
        }
        T::Argument__::Freeze(u) => {
            let actual = match usage(env, context, u)? {
                T::Type::Reference(true, inner) => T::Type::Reference(false, inner.clone()),
                T::Type::Reference(false, _) => {
                    anyhow::bail!("should never FreezeRef an immutable reference")
                }
                ty => {
                    anyhow::bail!("should never Freeze a non-reference type, got {ty:?}");
                }
            };
            (actual, param)
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
    anyhow::ensure!(
        &actual == expected,
        "argument type mismatch. Expected {expected:?}, got {actual:?}"
    );
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

fn usage(env: &Env, context: &Context, u: &T::Usage) -> anyhow::Result<T::Type> {
    match u {
        T::Usage::Move(l)
        | T::Usage::Copy {
            location: l,
            borrowed: _,
        } => location(env, context, *l),
    }
}

fn location(env: &Env, context: &Context, l: T::Location) -> anyhow::Result<T::Type> {
    Ok(match l {
        T::Location::TxContext => env.tx_context_type()?,
        T::Location::GasCoin => env.gas_coin_type()?,
        T::Location::ObjectInput(i) => context
            .objects
            .get(i as usize)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("object input {i} out of bounds"))?
            .clone(),
        T::Location::WithdrawalInput(i) => context
            .withdrawals
            .get(i as usize)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("withdrawal input {i} out of bounds"))?
            .clone(),
        T::Location::PureInput(i) => context
            .pure
            .get(i as usize)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("pure input {i} out of bounds"))?
            .clone(),
        T::Location::ReceivingInput(i) => context
            .receiving
            .get(i as usize)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("receiving input {i} out of bounds"))?
            .clone(),
        T::Location::Result(i, j) => context
            .result_types
            .get(i as usize)
            .and_then(|v| v.get(j as usize))
            .ok_or_else(|| anyhow::anyhow!("result ({i}, {j}) out of bounds",))?
            .clone(),
    })
}
