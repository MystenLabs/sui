// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::PackageStore,
    sp,
    static_programmable_transactions::{env, spanned::Spanned, typing::ast as T},
};
use sui_types::error::ExecutionError;

pub fn verify(env: &env::Env, tt: &T::Transaction) -> Result<(), ExecutionError> {
    let check_type = |ty| ensure_type_defining_id_based(env, ty);
    let check_arg = |sp!(_, (_, ty)): &Spanned<_>| ensure_type_defining_id_based(env, ty);

    // Verify all types in inputs are defining-id based.
    tt.objects
        .iter()
        .try_for_each(|object_input| check_type(&object_input.ty))?;
    tt.pure
        .iter()
        .try_for_each(|pure_input| check_type(&pure_input.ty))?;
    tt.receiving
        .iter()
        .try_for_each(|receiving_input| check_type(&receiving_input.ty))?;

    // Verify all types in commands are defining-id based.
    tt.commands.iter().try_for_each(|sp!(_, c)| {
        c.result_type.iter().try_for_each(check_type)?;
        match &c.command {
            T::Command__::Publish(_, _, _) => Ok(()),
            T::Command__::Upgrade(_, _, _, sp!(_, (_, ty)), _) => check_type(ty),
            T::Command__::SplitCoins(ty, sp!(_, (_, coin_ty)), amounts) => {
                check_type(ty)?;
                check_type(coin_ty)?;
                amounts.iter().try_for_each(check_arg)
            }
            T::Command__::MergeCoins(ty, sp!(_, (_, target_ty)), coins) => {
                check_type(ty)?;
                check_type(target_ty)?;
                coins.iter().try_for_each(check_arg)
            }
            T::Command__::MakeMoveVec(ty, args) => {
                check_type(ty)?;
                args.iter().try_for_each(check_arg)
            }
            T::Command__::TransferObjects(objs, sp!(_, (_, recipient_ty))) => {
                objs.iter().try_for_each(check_arg)?;
                check_type(recipient_ty)
            }
            T::Command__::MoveCall(move_call) => {
                move_call
                    .function
                    .type_arguments
                    .iter()
                    .try_for_each(check_type)?;
                move_call.arguments.iter().try_for_each(check_arg)
            }
        }
    })
}

fn ensure_type_defining_id_based(env: &env::Env, ty: &T::Type) -> Result<(), ExecutionError> {
    match ty {
        T::Type::Bool
        | T::Type::U8
        | T::Type::U16
        | T::Type::U32
        | T::Type::U64
        | T::Type::U128
        | T::Type::U256
        | T::Type::Address
        | T::Type::Signer => Ok(()),
        T::Type::Reference(_, ty) => ensure_type_defining_id_based(env, ty),
        T::Type::Vector(vector) => ensure_type_defining_id_based(env, &vector.element_type),
        T::Type::Datatype(datatype) => {
            // Resolve the type to its defining ID and ensure it matches the module address that is
            // already written down as the defining ID.
            //
            // If we fail to resolve the type that's an invariant violation as we should be able to
            // load the package that defined this type otherwise we should have failed at
            // load/typing time.
            let Ok(Some(resolved_id)) = env.linkable_store.resolve_type_to_defining_id(
                (*datatype.module.address()).into(),
                datatype.module.name(),
                &datatype.name,
            ) else {
                invariant_violation!("[defining_ids_in_types] Unable to resolve Type {ty:?}",);
            };

            if *resolved_id != *datatype.module.address() {
                invariant_violation!(
                    "[defining_ids_in_types] Type {ty:?} has a different defining ID {} than expected: {resolved_id}",
                    datatype.module.address()
                );
            }

            datatype
                .type_arguments
                .iter()
                .try_for_each(|arg| ensure_type_defining_id_based(env, arg))
        }
    }
}
