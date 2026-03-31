// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::base_types::{SequenceNumber, SuiAddress};
use sui_types::coin_reservation::{CoinReservationResolverTrait, ParsedObjectRefWithdrawal};
use sui_types::digests::ChainIdentifier;
use sui_types::transaction::{CallArg, ObjectArg, ProgrammableTransaction, TransactionKind};

/// Rewrites coin reservation inputs (fake coins encoded as masked ObjectRefs) into
/// FundsWithdrawalArgs so the executor can resolve them as balance withdrawals.
///
/// Returns `Some(rewritten_inputs)` if any inputs were rewritten, where each bool indicates whether
/// the corresponding input was converted from a coin reservation. Returns `None` if nothing
/// was rewritten.
pub fn rewrite_transaction_for_coin_reservations(
    chain_identifier: ChainIdentifier,
    coin_reservation_resolver: &dyn CoinReservationResolverTrait,
    sender: SuiAddress,
    transaction_kind: &mut TransactionKind,
    accumulator_version: Option<SequenceNumber>,
) -> Option<Vec<bool>> {
    match transaction_kind {
        TransactionKind::ProgrammableTransaction(pt) => {
            rewrite_programmable_transaction_for_coin_reservations(
                chain_identifier,
                coin_reservation_resolver,
                sender,
                pt,
                accumulator_version,
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
    accumulator_version: Option<SequenceNumber>,
) -> Option<Vec<bool>> {
    if pt.coin_reservation_obj_refs().count() == 0 {
        return None;
    }

    let mut rewritten_inputs = Vec::with_capacity(pt.inputs.len());
    for input in pt.inputs.iter_mut() {
        if let CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref)) = input
            && let Some(parsed) = ParsedObjectRefWithdrawal::parse(object_ref, chain_identifier)
        {
            rewritten_inputs.push(true);

            // unwrap: This cannot fail because:
            // 1. Coin reservations are validated in `process_funds_withdrawals_for_signing` before
            //    execution, which checks that the accumulator exists and is owned by the sender.
            // 2. The scheduler reserves funds before allowing the transaction to execute. If the
            //    accumulator were deleted (balance dropped to 0), the reservation would fail and
            //    the transaction would not enter execution.
            let withdraw = coin_reservation_resolver
                .resolve_funds_withdrawal(sender, parsed, accumulator_version)
                .unwrap();
            *input = CallArg::FundsWithdrawal(withdraw);
        } else {
            rewritten_inputs.push(false);
        }
    }

    Some(rewritten_inputs)
}
