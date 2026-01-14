// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::base_types::SuiAddress;
use sui_types::coin_reservation::{CoinReservationResolverTrait, ParsedObjectRefWithdrawal};
use sui_types::digests::ChainIdentifier;
use sui_types::transaction::{CallArg, ObjectArg, ProgrammableTransaction, TransactionKind};

pub fn rewrite_transaction_for_coin_reservations(
    chain_identifier: ChainIdentifier,
    coin_reservation_resolver: &dyn CoinReservationResolverTrait,
    sender: SuiAddress,
    transaction_kind: &mut TransactionKind,
) -> Vec<bool> {
    match transaction_kind {
        TransactionKind::ProgrammableTransaction(pt) => {
            rewrite_programmable_transaction_for_coin_reservations(
                chain_identifier,
                coin_reservation_resolver,
                sender,
                pt,
            )
        }
        _ => vec![],
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
