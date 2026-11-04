// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution_mode::ExecutionMode,
    gas_charger::GasPayment,
    static_programmable_transactions::{
        env::Env,
        loading::ast::{self as L},
        metering::{self, translation_meter::TranslationMeter},
        preparation::ast as A,
        typing::invariant_checks::linkage_consistency,
    },
};
use move_core_types::{language_storage::StructTag, u256::U256};
use mysten_common::ZipDebugEqIteratorExt;
use std::rc::Rc;
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
    pt: A::Transaction,
) -> Result<L::Transaction, Mode::Error> {
    let A::Transaction {
        inputs,
        commands,
        unified_linkage,
    } = pt;
    assert_invariant!(
        Rc::ptr_eq(&unified_linkage.0, &env.unified_linkage.0),
        "prepared transaction and environment must share linkage"
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
    // Check every known input before typing can transform or discard unused inputs.
    linkage_consistency::verify_type_linkage::<Mode>(
        inputs.iter().filter_map(|(_, ty)| match ty {
            L::InputType::Fixed(ty) => Some(ty),
            L::InputType::Bytes => None,
        }),
        &unified_linkage,
    )?;
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
        unified_linkage,
    };
    metering::loading::meter::<Mode::Error>(meter, &loaded_tx)?;
    Ok(loaded_tx)
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
                P::WithdrawFrom::SenderAllowance { funder, allowance } => {
                    L::WithdrawalSource::Allowance {
                        funder: funder.into(),
                        id: allowance,
                    }
                }
            };
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
    command: A::Command,
) -> Result<L::Command, Mode::Error> {
    Ok(match command {
        A::Command::MoveCall(pmc) => {
            let A::MoveCall {
                package,
                module,
                function: name,
                type_arguments: ptype_arguments,
                arguments,
                visibility,
            } = *pmc;
            let type_arguments = ptype_arguments
                .into_iter()
                .enumerate()
                .map(|(idx, ty)| env.load_type_tag(idx, &ty))
                .collect::<Result<Vec<_>, _>>()?;
            let function =
                env.load_function(package, module.clone(), name.clone(), type_arguments)?;
            assert_invariant!(
                function.name == name
                    && function.visibility == visibility
                    && *function.version_mid.address() == package.into()
                    && function.version_mid.name() == module.as_ident_str()
                    && function.original_mid.name() == module.as_ident_str(),
                "prepared call to {package}::{module}::{name} (visibility {visibility:?}) was loaded as {}::{} (visibility {:?}, original module {})",
                function.version_mid,
                function.name,
                function.visibility,
                function.original_mid,
            );
            L::Command::MoveCall(Box::new(L::MoveCall {
                function,
                arguments,
            }))
        }
        A::Command::MakeMoveVec(ptype_argument, arguments) => {
            let type_argument = ptype_argument
                .map(|ty| env.load_type_tag(0, &ty))
                .transpose()?;
            L::Command::MakeMoveVec(type_argument, arguments)
        }
        A::Command::TransferObjects(objects, address) => {
            L::Command::TransferObjects(objects, address)
        }
        A::Command::SplitCoins(coin, amounts) => L::Command::SplitCoins(coin, amounts),
        A::Command::MergeCoins(target, coins) => L::Command::MergeCoins(target, coins),
        A::Command::Publish(payload, dep_ids, linkage) => {
            L::Command::Publish(payload, dep_ids, linkage)
        }
        A::Command::Upgrade(payload, dep_ids, object_id, argument, linkage) => {
            L::Command::Upgrade(payload, dep_ids, object_id, argument, linkage)
        }
    })
}
