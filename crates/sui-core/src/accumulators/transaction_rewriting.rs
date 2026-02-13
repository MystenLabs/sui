// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// GasCoin Materialization for Fake Coins and Address Balance Payments
// ====================================================================
//
// When a transaction uses Argument::GasCoin, it expects to receive a Coin<SUI> object that it
// can split, transfer, etc. With real coins, this is straightforward: the gas coin is the
// first coin in the gas payment list. However, when the gas payment uses fake coins (coin
// reservations) or pure address balance, there is no real coin to provide.
//
// Strategy
// --------
// At transaction rewriting time (after scheduling but before execution), we check if the first
// coin in the gas payment list is a real coin:
//
// 1. If the first coin is a real coin: do nothing. The existing behavior works correctly.
//
// 2. If the payment list is empty, or if the first coin is a fake coin: we materialize a
//    GasCoin by adding a new input to the transaction. This input is a FundsWithdrawalArg
//    of type Balance<SUI>. The reservation size equals:
//
//        (total SUI reserved by fake coins in the payment list) - (gas budget)
//
//    We then rewrite the transaction so that any use of Argument::GasCoin now refers to this
//    new input instead. The adapter will convert this Balance into a Coin as necessary.
//
// Correctness Argument: Reservation Safety
// ----------------------------------------
// We are adding a new reservation at rewriting time, which occurs after all scheduling and
// balance checks. We must ensure that the reservation we create cannot exceed reservations
// that were visible at scheduling time.
//
// This holds because:
// - The gas budget already formed one reservation against the SUI address balance
// - The fake coins collectively form another reservation against the SUI address balance
// - The new reservation (fake coin total - gas budget) is at most the sum of existing
//   reservations minus the gas budget, which was already validated
//
// Therefore, we have not allowed an unsatisfiable reservation to enter execution.
//
// Correctness Argument: SUI Conservation
// --------------------------------------
// The coins in the gas payment in excess of the budget are not currently available to the
// transaction through any other mechanism. Making them available via this new withdrawal
// does not allow a "double redeem" to occur:
//
// - The fake coins themselves are not real objects and cannot be spent
// - The gas budget portion is excluded from the materialized coin, so we will not overdraw
//   the balance when charging gas at the end of execution
// - The total SUI available remains exactly what was reserved by the original fake coins

use sui_types::base_types::SuiAddress;
use sui_types::coin_reservation::{CoinReservationResolverTrait, ParsedObjectRefWithdrawal};
use sui_types::digests::ChainIdentifier;
use sui_types::gas_coin::GAS;
use sui_types::transaction::{
    Argument, CallArg, Command, FundsWithdrawalArg, GasData, ObjectArg, ProgrammableTransaction,
    TransactionKind,
};

pub fn rewrite_transaction_for_coin_reservations(
    chain_identifier: ChainIdentifier,
    coin_reservation_resolver: &dyn CoinReservationResolverTrait,
    sender: SuiAddress,
    gas_data: &GasData,
    transaction_kind: &mut TransactionKind,
) -> Option<Vec<bool>> {
    match transaction_kind {
        TransactionKind::ProgrammableTransaction(pt) => {
            rewrite_programmable_transaction_for_coin_reservations(
                chain_identifier,
                coin_reservation_resolver,
                sender,
                gas_data,
                pt,
            )
        }
        _ => None,
    }
}

fn rewrite_programmable_transaction_for_coin_reservations(
    chain_identifier: ChainIdentifier,
    coin_reservation_resolver: &dyn CoinReservationResolverTrait,
    sender: SuiAddress,
    gas_data: &GasData,
    pt: &mut ProgrammableTransaction,
) -> Option<Vec<bool>> {
    let has_coin_reservations_in_inputs = pt.coin_reservation_obj_refs().count() > 0;
    let needs_gas_coin_materialization = needs_gas_coin_materialization(chain_identifier, gas_data);

    if !has_coin_reservations_in_inputs && !needs_gas_coin_materialization {
        return None;
    }

    // Rewrite fake coin inputs to FundsWithdrawalArgs
    let mut compat_args = Vec::with_capacity(pt.inputs.len());
    for input in pt.inputs.iter_mut() {
        if let CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref)) = input
            && let Some(parsed) = ParsedObjectRefWithdrawal::parse(object_ref, chain_identifier)
        {
            compat_args.push(true);

            let withdraw = coin_reservation_resolver
                .resolve_funds_withdrawal(sender, parsed)
                .unwrap();
            *input = CallArg::FundsWithdrawal(withdraw);
        } else {
            compat_args.push(false);
        }
    }

    // Materialize GasCoin if needed
    if needs_gas_coin_materialization {
        materialize_gas_coin(chain_identifier, gas_data, pt, &mut compat_args);
    }

    Some(compat_args)
}

/// Checks if GasCoin materialization is needed.
/// Returns true if the payment list is empty or if the first coin is a fake coin.
fn needs_gas_coin_materialization(chain_identifier: ChainIdentifier, gas_data: &GasData) -> bool {
    match gas_data.payment.first() {
        None => true,
        Some(first_coin) => {
            ParsedObjectRefWithdrawal::parse(first_coin, chain_identifier).is_some()
        }
    }
}

/// Materializes a GasCoin by adding a FundsWithdrawalArg input and rewriting Argument::GasCoin
/// references to point to the new input.
fn materialize_gas_coin(
    chain_identifier: ChainIdentifier,
    gas_data: &GasData,
    pt: &mut ProgrammableTransaction,
    compat_args: &mut Vec<bool>,
) {
    // Calculate total fake coin reservation amount
    let total_fake_coin_amount: u64 = gas_data
        .payment
        .iter()
        .filter_map(|obj_ref| ParsedObjectRefWithdrawal::parse(obj_ref, chain_identifier))
        .map(|parsed| parsed.reservation_amount())
        .sum();

    // The materialized amount is the fake coin total minus the gas budget.
    // If there are no fake coins (total is 0), we materialize 0 (no-op withdrawal).
    let materialized_amount = total_fake_coin_amount.saturating_sub(gas_data.budget);

    // Add a new FundsWithdrawalArg input for the materialized GasCoin
    let withdrawal = FundsWithdrawalArg::balance_from_sender(materialized_amount, GAS::type_tag());
    pt.inputs.push(CallArg::FundsWithdrawal(withdrawal));

    // Mark this as a compatibility input so the executor converts the Balance to a Coin.
    // This is necessary because commands like SplitCoins expect a Coin<T>, not a Balance<T>.
    compat_args.push(true);

    // Get the index of the new input
    let new_input_index = (pt.inputs.len() - 1) as u16;
    let replacement_arg = Argument::Input(new_input_index);

    // Rewrite all Argument::GasCoin references to point to the new input
    rewrite_gas_coin_references(&mut pt.commands, replacement_arg);
}

/// Rewrites all Argument::GasCoin references in commands to the given replacement argument.
fn rewrite_gas_coin_references(commands: &mut [Command], replacement: Argument) {
    for command in commands.iter_mut() {
        match command {
            Command::MoveCall(call) => {
                for arg in call.arguments.iter_mut() {
                    if *arg == Argument::GasCoin {
                        *arg = replacement;
                    }
                }
            }
            Command::TransferObjects(args, recipient) => {
                for arg in args.iter_mut() {
                    if *arg == Argument::GasCoin {
                        *arg = replacement;
                    }
                }
                if *recipient == Argument::GasCoin {
                    *recipient = replacement;
                }
            }
            Command::SplitCoins(coin, amounts) => {
                if *coin == Argument::GasCoin {
                    *coin = replacement;
                }
                for arg in amounts.iter_mut() {
                    if *arg == Argument::GasCoin {
                        *arg = replacement;
                    }
                }
            }
            Command::MergeCoins(target, sources) => {
                if *target == Argument::GasCoin {
                    *target = replacement;
                }
                for arg in sources.iter_mut() {
                    if *arg == Argument::GasCoin {
                        *arg = replacement;
                    }
                }
            }
            Command::Publish(_, _) => {
                // No arguments to rewrite
            }
            Command::MakeMoveVec(_, args) => {
                for arg in args.iter_mut() {
                    if *arg == Argument::GasCoin {
                        *arg = replacement;
                    }
                }
            }
            Command::Upgrade(_, _, _, ticket) => {
                if *ticket == Argument::GasCoin {
                    *ticket = replacement;
                }
            }
        }
    }
}
