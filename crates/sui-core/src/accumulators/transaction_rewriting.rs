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
use sui_types::transaction::{CallArg, ObjectArg, ProgrammableTransaction, TransactionKind};

pub fn rewrite_transaction_for_coin_reservations(
    chain_identifier: ChainIdentifier,
    coin_reservation_resolver: &dyn CoinReservationResolverTrait,
    sender: SuiAddress,
    transaction_kind: &mut TransactionKind,
) -> Option<Vec<bool>> {
    match transaction_kind {
        TransactionKind::ProgrammableTransaction(pt) => {
            rewrite_programmable_transaction_for_coin_reservations(
                chain_identifier,
                coin_reservation_resolver,
                sender,
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
    pt: &mut ProgrammableTransaction,
) -> Option<Vec<bool>> {
    if pt.coin_reservation_obj_refs().count() == 0 {
        return None;
    }

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
    Some(compat_args)
}
