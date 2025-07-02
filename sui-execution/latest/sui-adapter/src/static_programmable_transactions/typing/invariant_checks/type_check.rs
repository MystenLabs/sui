// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::PackageStore,
    sp,
    static_programmable_transactions::{
        env::Env,
        spanned::Spanned,
        typing::{ast as T, verify::input_arguments},
    },
};
use sui_types::error::ExecutionError;

struct Context<'a> {
    inputs: Vec<&'a T::InputType>,
    usable: Vec<bool>,
}

impl<'a> Context<'a> {
    fn new(inputs: &'a T::Inputs) -> Self {
        Self {
            inputs: inputs.iter().map(|(_, ty)| ty).collect(),
            usable: vec![false; inputs.len()], // set later
        }
    }
}

pub fn verify(env: &Env, txn: &T::Transaction) -> Result<(), ExecutionError> {
    let T::Transaction { inputs, commands } = txn;
    let mut context = Context::new(&inputs);
    for (idx, (i, ty)) in inputs.iter().enumerate() {
        input(&mut context, idx, i, ty)?;
    }
    for (c, result_ty) in commands {
        command(&context, c, result_ty)?;
    }
    Ok(())
}

fn input(
    context: &mut Context,
    idx: usize,
    arg: &T::InputArg,
    ty: &T::InputType,
) -> Result<(), ExecutionError> {
    match arg {
        T::InputArg::Pure(_) => {
            let T::InputType::Bytes(tys) = ty else {
                invariant_violation!("pure should not have a fixed type",);
            };
            context.usable[idx] = !tys.is_empty();
        }
        T::InputArg::Receiving(_) => {
            let T::InputType::Bytes(tys) = ty else {
                invariant_violation!("receiving should not have a fixed type",);
            };
            for (ty, _) in tys {
                assert_invariant!(
                    input_arguments::is_valid_receiving(ty),
                    "receiving type must be valid"
                );
            }
            context.usable[idx] = !tys.is_empty();
        }
        T::InputArg::Object(_) => {
            let T::InputType::Fixed(ty) = ty else {
                invariant_violation!("object should not have a bytes type",);
            };
            assert_invariant!(ty.abilities().has_key(), "object type must have key");
            context.usable[idx] = true
        }
    }
    Ok(())
}

fn command(
    context: &mut Context,
    sp!(_, c): &T::Command,
    result_ty: &T::Type,
) -> Result<(), ExecutionError> {
    match c {
        T::Command_::MoveCall(move_call) => {
            let T::MoveCall {
                function,
                arguments,
                type_arguments,
                spanned,
            } = move_call;
            assert_invariant!(
                m
            )
            for
        },
        T::Command_::TransferObjects(spanneds, spanned) => todo!(),
        T::Command_::SplitCoins(_, spanned, spanneds) => todo!(),
        T::Command_::MergeCoins(_, spanned, spanneds) => todo!(),
        T::Command_::MakeMoveVec(_, spanneds) => todo!(),
        T::Command_::Publish(items, object_ids, resolved_linkage) => todo!(),
        T::Command_::Upgrade(items, object_ids, object_id, spanned, resolved_linkage) => todo!(),
    }
}
