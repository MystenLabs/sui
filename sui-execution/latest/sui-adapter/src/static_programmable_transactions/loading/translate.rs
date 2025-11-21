// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::{
    env::Env,
    linkage::resolved_linkage::RootedLinkage,
    loading::ast as L,
    metering::{self, translation_meter::TranslationMeter},
};
use move_core_types::{account_address::AccountAddress, language_storage::StructTag, u256::U256};
use sui_types::{
    base_types::TxContext,
    error::ExecutionError,
    object::Owner,
    transaction::{self as P, CallArg, FundsWithdrawalArg, ObjectArg, SharedObjectMutability},
};

pub fn transaction(
    meter: &mut TranslationMeter<'_, '_>,
    env: &Env,
    tx_context: &TxContext,
    pt: P::ProgrammableTransaction,
) -> Result<L::Transaction, ExecutionError> {
    metering::pre_translation::meter(meter, &pt)?;
    let P::ProgrammableTransaction { inputs, commands } = pt;
    let inputs = inputs
        .into_iter()
        .map(|arg| input(env, tx_context, arg))
        .collect::<Result<Vec<_>, _>>()?;
    let commands = commands
        .into_iter()
        .enumerate()
        .map(|(idx, cmd)| command(env, cmd).map_err(|e| e.with_command_index(idx)))
        .collect::<Result<Vec<_>, _>>()?;
    let loaded_tx = L::Transaction { inputs, commands };
    metering::loading::meter(meter, &loaded_tx)?;
    Ok(loaded_tx)
}

fn input(
    env: &Env,
    tx_context: &TxContext,
    arg: CallArg,
) -> Result<(L::InputArg, L::InputType), ExecutionError> {
    Ok(match arg {
        CallArg::Pure(bytes) => (L::InputArg::Pure(bytes), L::InputType::Bytes),
        CallArg::Object(ObjectArg::Receiving(oref)) => {
            (L::InputArg::Receiving(oref), L::InputType::Bytes)
        }
        CallArg::Object(ObjectArg::ImmOrOwnedObject(oref)) => {
            let id = &oref.0;
            let obj = env.read_object(id)?;
            let Some(ty) = obj.type_() else {
                invariant_violation!("Object {:?} has does not have a Move type", id);
            };
            let tag: StructTag = ty.clone().into();
            let ty = env.load_type_from_struct(&tag)?;
            let arg = match obj.owner {
                Owner::AddressOwner(_) => L::ObjectArg::OwnedObject(oref),
                Owner::Immutable => L::ObjectArg::ImmObject(oref),
                Owner::ObjectOwner(_)
                | Owner::Shared { .. }
                | Owner::ConsensusAddressOwner { .. } => {
                    invariant_violation!("Unexpected owner for ImmOrOwnedObject: {:?}", obj.owner);
                }
            };
            (L::InputArg::Object(arg), L::InputType::Fixed(ty))
        }
        CallArg::Object(ObjectArg::SharedObject {
            id,
            initial_shared_version,
            mutability,
        }) => {
            let obj = env.read_object(&id)?;
            let Some(ty) = obj.type_() else {
                invariant_violation!("Object {:?} has does not have a Move type", id);
            };
            let tag: StructTag = ty.clone().into();
            let ty = env.load_type_from_struct(&tag)?;
            let kind = match obj.owner {
                Owner::AddressOwner(_) | Owner::ObjectOwner(_) | Owner::Immutable => {
                    invariant_violation!("Unexpected owner for SharedObject: {:?}", obj.owner)
                }
                Owner::Shared { .. } => L::SharedObjectKind::Legacy,
                Owner::ConsensusAddressOwner { .. } => L::SharedObjectKind::Party,
            };
            (
                L::InputArg::Object(L::ObjectArg::SharedObject {
                    id,
                    initial_shared_version,
                    mutability: object_mutability(mutability),
                    kind,
                }),
                L::InputType::Fixed(ty),
            )
        }
        CallArg::FundsWithdrawal(f) => {
            let FundsWithdrawalArg {
                reservation,
                type_arg,
                withdraw_from,
            } = f;
            let amount = match reservation {
                P::Reservation::EntireBalance => {
                    invariant_violation!("Entire balance reservation amount is not yet supported")
                }
                P::Reservation::MaxAmountU64(u) => U256::from(u),
                // TODO when types other than u64 are supported, we must check that this is a
                // valid amount for the type
            };
            let funds_ty = match type_arg {
                P::WithdrawalTypeArg::Balance(inner) => {
                    let inner = env.load_type_input(0, inner)?;
                    env.balance_type(inner)?
                }
            };
            let ty = env.withdrawal_type(funds_ty.clone())?;
            let owner: AccountAddress = match withdraw_from {
                P::WithdrawFrom::Sender => tx_context.sender().into(),
                P::WithdrawFrom::Sponsor => tx_context
                    .sponsor()
                    .ok_or_else(|| {
                        make_invariant_violation!(
                            "A sponsor withdrawal requires a sponsor and should have been \
                            checked at signing"
                        )
                    })?
                    .into(),
            };
            (
                L::InputArg::FundsWithdrawal(L::FundsWithdrawalArg {
                    amount,
                    ty: ty.clone(),
                    owner,
                }),
                L::InputType::Fixed(ty),
            )
        }
    })
}

fn object_mutability(mutability: SharedObjectMutability) -> L::ObjectMutability {
    match mutability {
        SharedObjectMutability::Mutable => L::ObjectMutability::Mutable,
        SharedObjectMutability::NonExclusiveWrite => L::ObjectMutability::NonExclusiveWrite,
        SharedObjectMutability::Immutable => L::ObjectMutability::Immutable,
    }
}

fn command(env: &Env, command: P::Command) -> Result<L::Command, ExecutionError> {
    Ok(match command {
        P::Command::MoveCall(pmc) => {
            let resolved_linkage = env
                .linkage_analysis
                .compute_call_linkage(&pmc, env.linkable_store)?;
            let P::ProgrammableMoveCall {
                package,
                module,
                function: name,
                type_arguments: ptype_arguments,
                arguments,
            } = *pmc;
            let linkage = RootedLinkage::new(*package, resolved_linkage);
            let type_arguments = ptype_arguments
                .into_iter()
                .enumerate()
                .map(|(idx, ty)| env.load_type_input(idx, ty))
                .collect::<Result<Vec<_>, _>>()?;
            let function = env.load_function(package, module, name, type_arguments, linkage)?;
            L::Command::MoveCall(Box::new(L::MoveCall {
                function,
                arguments,
            }))
        }
        P::Command::MakeMoveVec(ptype_argument, arguments) => {
            let type_argument = ptype_argument
                .map(|ty| env.load_type_input(0, ty))
                .transpose()?;
            L::Command::MakeMoveVec(type_argument, arguments)
        }
        P::Command::TransferObjects(objects, address) => {
            L::Command::TransferObjects(objects, address)
        }
        P::Command::SplitCoins(coin, amounts) => L::Command::SplitCoins(coin, amounts),
        P::Command::MergeCoins(target, coins) => L::Command::MergeCoins(target, coins),
        P::Command::Publish(items, object_ids) => {
            let resolved_linkage = env
                .linkage_analysis
                .compute_publication_linkage(&object_ids, env.linkable_store)?;
            L::Command::Publish(items, object_ids, resolved_linkage)
        }
        P::Command::Upgrade(items, object_ids, object_id, argument) => {
            let resolved_linkage = env
                .linkage_analysis
                .compute_publication_linkage(&object_ids, env.linkable_store)?;
            L::Command::Upgrade(items, object_ids, object_id, argument, resolved_linkage)
        }
    })
}
