// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::base_types::{SequenceNumber, SuiAddress};
use sui_types::coin_reservation::{CoinReservationResolverTrait, ParsedObjectRefWithdrawal};
use sui_types::digests::ChainIdentifier;
use sui_types::error::UserInputResult;
use sui_types::transaction::{CallArg, ObjectArg, ProgrammableTransaction, TransactionKind};

/// Rewrites coin reservation inputs (fake coins encoded as masked ObjectRefs) into
/// FundsWithdrawalArgs so the executor can resolve them as balance withdrawals.
///
/// Returns `Ok(Some(rewritten))` where each bool flags whether that input was rewritten,
/// `Ok(None)` if nothing was rewritten, or `Err` if a reservation cannot be resolved.
///
/// `accumulator_version` selects the accumulator version for MVCC lookup during checkpoint
/// replay (read before any settlement modifies it); pass `None` for the latest version.
pub fn rewrite_transaction_for_coin_reservations(
    chain_identifier: ChainIdentifier,
    coin_reservation_resolver: &dyn CoinReservationResolverTrait,
    sender: SuiAddress,
    transaction_kind: &mut TransactionKind,
    accumulator_version: Option<SequenceNumber>,
) -> UserInputResult<Option<Vec<bool>>> {
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
        _ => Ok(None),
    }
}

fn rewrite_programmable_transaction_for_coin_reservations(
    chain_identifier: ChainIdentifier,
    coin_reservation_resolver: &dyn CoinReservationResolverTrait,
    sender: SuiAddress,
    pt: &mut ProgrammableTransaction,
    accumulator_version: Option<SequenceNumber>,
) -> UserInputResult<Option<Vec<bool>>> {
    if pt.coin_reservation_obj_refs().count() == 0 {
        return Ok(None);
    }

    let mut rewritten_inputs = Vec::with_capacity(pt.inputs.len());
    for input in pt.inputs.iter_mut() {
        if let CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref)) = input
            && let Some(parsed) = ParsedObjectRefWithdrawal::parse(object_ref, chain_identifier)
        {
            rewritten_inputs.push(true);

            let withdraw = coin_reservation_resolver.resolve_funds_withdrawal(
                sender,
                parsed,
                accumulator_version,
            )?;
            *input = CallArg::FundsWithdrawal(withdraw);
        } else {
            rewritten_inputs.push(false);
        }
    }

    Ok(Some(rewritten_inputs))
}
