// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution_mode::ExecutionMode,
    gas_charger::GasPayment,
    static_programmable_transactions::{
        env::Env,
        linkage,
        loading::ast::{self as L, PackagePayload},
        metering::{self, translation_meter::TranslationMeter},
    },
};
use move_core_types::{language_storage::StructTag, u256::U256};
use mysten_common::ZipDebugEqIteratorExt;
use sui_types::{
    base_types::TxContext,
    error::ExecutionErrorTrait,
    object::{ObjectPermissions, Owner},
    transaction::{self as P, CallArg, FundsWithdrawalArg, ObjectArg, SharedObjectMutability},
};

pub fn transaction<Mode: ExecutionMode>(
    meter: &mut TranslationMeter<'_, '_>,
    env: &Env<Mode>,
    tx_context: &TxContext,
    // which inputs are withdrawals that need to be converted to coins, must
    // be the same length as the inputs
    withdrawal_compatibility_inputs: Option<Vec<bool>>,
    gas_payment: Option<GasPayment>,
    pt: P::ProgrammableTransaction,
) -> Result<L::Transaction, Mode::Error> {
    metering::pre_translation::meter::<Mode::Error>(meter, &pt)?;
    let P::ProgrammableTransaction { inputs, commands } = pt;
    // withdrawal_compatibility_inputs specified ==> the protocol config flag is set
    assert_invariant!(
        withdrawal_compatibility_inputs.is_none()
            || env
                .protocol_config
                .convert_withdrawal_compatibility_ptb_arguments(),
        "if withdrawal compatibility must be specified, then the flag is set in the protocol config"
    );
    let withdrawal_compatibility_inputs =
        withdrawal_compatibility_inputs.unwrap_or_else(|| vec![false; inputs.len()]);
    assert_invariant!(
        inputs.len() == withdrawal_compatibility_inputs.len(),
        "withdrawal compatibility inputs must be the same length as the inputs"
    );
    let inputs = withdrawal_compatibility_inputs
        .into_iter()
        .zip_debug_eq(inputs)
        .map(|(is_withdrawal_compatibility_input, arg)| {
            input::<Mode>(env, tx_context, is_withdrawal_compatibility_input, arg)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let original_command_len = commands.len();
    let commands = commands
        .into_iter()
        .enumerate()
        .map(|(idx, cmd)| command::<Mode>(env, cmd).map_err(|e| e.with_command_index(idx)))
        .collect::<Result<Vec<_>, _>>()?;
    let loaded_tx = L::Transaction {
        gas_payment,
        inputs,
        original_command_len,
        commands,
    };
    metering::loading::meter::<Mode::Error>(meter, &loaded_tx)?;
    linkage::refine_linkage::<Mode>(
        loaded_tx,
        env.linkage_analysis,
        env.linkable_store,
        env.protocol_config,
    )
}

fn input<Mode: ExecutionMode>(
    env: &Env<Mode>,
    tx_context: &TxContext,
    // True iff this is a withdrawal that needs to be converted to a coin
    is_withdrawal_compatibility_input: bool,
    arg: CallArg,
) -> Result<(L::InputArg, L::InputType), Mode::Error> {
    // is_withdrawal_compatibility_input ==> FundsWithdrawal
    assert_invariant!(
        !is_withdrawal_compatibility_input || matches!(arg, CallArg::FundsWithdrawal(_)),
        "withdrawal compatibility inputs must be FundsWithdrawal"
    );
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
            let arg = match &obj.owner {
                Owner::AddressOwner(_) => L::ObjectArg {
                    kind: L::ObjectArgKind::OwnedObject(oref),
                    refined_permissions: ObjectPermissions::ALL,
                },
                Owner::Immutable => L::ObjectArg {
                    kind: L::ObjectArgKind::ImmObject(oref),
                    refined_permissions: ObjectPermissions::IMMUTABLE_USAGE,
                },
                Owner::ObjectOwner(_)
                | Owner::Shared { .. }
                | Owner::ConsensusAddressOwner { .. } => {
                    assert_invariant!(
                        Mode::allow_arbitrary_values(),
                        "Unexpected owner for ImmOrOwnedObject: {:?}",
                        obj.owner,
                    );
                    let kind = L::ObjectArgKind::OwnedObject(oref);
                    L::ObjectArg {
                        kind,
                        refined_permissions: ObjectPermissions::ALL,
                    }
                }
                Owner::Party { permissions, .. } => {
                    assert_invariant!(
                        Mode::allow_arbitrary_values(),
                        "Unexpected owner for ImmOrOwnedObject: {:?}",
                        obj.owner,
                    );
                    let refined_permissions = permissions.permissions_for(&tx_context.sender());
                    L::ObjectArg {
                        kind: L::ObjectArgKind::OwnedObject(oref),
                        refined_permissions,
                    }
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
                invariant_violation!("Object {:?} does not have a Move type", id);
            };
            let tag: StructTag = ty.clone().into();
            let ty = env.load_type_from_struct(&tag)?;
            let owner_permissions = match &obj.owner {
                Owner::AddressOwner(_) | Owner::ObjectOwner(_) | Owner::Immutable => {
                    assert_invariant!(
                        Mode::allow_arbitrary_values(),
                        "Unexpected owner for SharedObject: {:?}",
                        obj.owner
                    );
                    ObjectPermissions::ALL
                }
                Owner::Shared { .. } => ObjectPermissions::LEGACY_SHARED_OBJECT,
                Owner::ConsensusAddressOwner { .. } => ObjectPermissions::ALL,
                Owner::Party { permissions, .. } => {
                    permissions.permissions_for(&tx_context.sender())
                }
            };
            let refined_permissions = refine_permissions::<Mode>(mutability, owner_permissions)?;
            let kind = L::ObjectArgKind::ConsensusObject {
                id,
                initial_shared_version,
            };
            (
                L::InputArg::Object(L::ObjectArg {
                    kind,
                    refined_permissions,
                }),
                L::InputType::Fixed(ty),
            )
        }
        CallArg::FundsWithdrawal(f) => {
            assert_invariant!(
                env.protocol_config.enable_accumulators(),
                "Withdrawals should be rejected at signing if accumulators are not enabled"
            );
            let FundsWithdrawalArg {
                reservation,
                type_arg,
                withdraw_from,
            } = f;
            let amount = match reservation {
                P::Reservation::MaxAmountU64(u) => U256::from(u),
                // TODO when types other than u64 are supported, we must check that this is a
                // valid amount for the type
            };
            let funds_ty = match type_arg {
                P::WithdrawalTypeArg::Balance(inner) => {
                    let inner = env.load_type_tag(0, &inner)?;
                    env.balance_type(inner)?
                }
            };
            let source = match withdraw_from {
                P::WithdrawFrom::Sender => L::WithdrawalSource::Direct {
                    owner: tx_context.sender().into(),
                },
                P::WithdrawFrom::Sponsor => L::WithdrawalSource::Direct {
                    owner: tx_context
                        .sponsor()
                        .ok_or_else(|| {
                            make_invariant_violation!(
                                "A sponsor withdrawal requires a sponsor and should have been \
                                checked at signing"
                            )
                        })?
                        .into(),
                },
                // Signing verified the declared funder against the allowance object
                // (which is immutable post-issue), so it can be used directly here.
                P::WithdrawFrom::Allowance { funder, allowance } => {
                    L::WithdrawalSource::Allowance {
                        funder: funder.into(),
                        id: allowance,
                    }
                }
            };
            // Compat inputs are rewritten coin reservations, which always resolve
            // to sender withdrawals; an allowance here would be a rewriter bug.
            debug_assert!(
                !is_withdrawal_compatibility_input
                    || matches!(source, L::WithdrawalSource::Direct { .. })
            );
            let ty = env.withdrawal_type_for_source(&source, funds_ty)?;
            (
                L::InputArg::FundsWithdrawal(L::FundsWithdrawalArg {
                    from_compatibility_object: is_withdrawal_compatibility_input,
                    amount,
                    ty: ty.clone(),
                    source,
                }),
                L::InputType::Fixed(ty),
            )
        }
    })
}

fn refine_permissions<Mode: ExecutionMode>(
    mutability: SharedObjectMutability,
    permissions: ObjectPermissions,
) -> Result<ObjectPermissions, Mode::Error> {
    Ok(match mutability {
        SharedObjectMutability::Mutable | SharedObjectMutability::NonExclusiveWrite => {
            assert_invariant!(
                permissions.can_use_mutably(),
                "Mutable shared object usage requires mutable usage permission"
            );
            permissions
        }
        SharedObjectMutability::Immutable => {
            assert_invariant!(
                permissions.can_use_immutably(),
                "Immutable shared object usage requires immutable usage permission"
            );
            ObjectPermissions::IMMUTABLE_USAGE
        }
    })
}

fn command<Mode: ExecutionMode>(
    env: &Env<Mode>,
    command: P::Command,
) -> Result<L::Command, Mode::Error> {
    Ok(match command {
        P::Command::MoveCall(pmc) => {
            let P::ProgrammableMoveCall {
                package,
                module,
                function: name,
                type_arguments: ptype_arguments,
                arguments,
            } = *pmc;
            let type_arguments = ptype_arguments
                .into_iter()
                .enumerate()
                .map(|(idx, ty)| env.load_type_input(idx, ty))
                .collect::<Result<Vec<_>, _>>()?;
            let function = env.load_function(package, module, name, type_arguments)?;
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
        P::Command::Publish(items, dep_ids) => {
            let resolved_linkage = env
                .linkage_analysis
                .compute_publication_linkage::<Mode::Error>(&dep_ids, env.linkable_store)?;
            let payload = if env.protocol_config.enable_unified_linkage() {
                let deserialized_pkg = env.deserialize_package(&items, &dep_ids)?;
                PackagePayload::Deserialized(deserialized_pkg)
            } else {
                PackagePayload::Serialized(items)
            };
            L::Command::Publish(payload, dep_ids, resolved_linkage)
        }
        P::Command::Upgrade(items, dep_ids, object_id, argument) => {
            let resolved_linkage = env
                .linkage_analysis
                .compute_publication_linkage::<Mode::Error>(&dep_ids, env.linkable_store)?;
            let payload = if env.protocol_config.enable_unified_linkage() {
                let deserialized_pkg = env.deserialize_package(&items, &dep_ids)?;
                PackagePayload::Deserialized(deserialized_pkg)
            } else {
                PackagePayload::Serialized(items)
            };
            L::Command::Upgrade(payload, dep_ids, object_id, argument, resolved_linkage)
        }
    })
}
