// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use sui_types::{
    accumulator_root::AccumulatorObjId,
    error::{SuiErrorKind, SuiResult, UserInputError},
};

use crate::execution_scheduler::balance_withdraw_scheduler::balance_read::AccountBalanceRead;

/// Checks if balances are available in the latest versions of the referenced acccumulator
/// objects. This does un-sequenced reads and can only be used on the signing/voting path
/// where deterministic results are not required.
pub(crate) fn check_balances_available(
    balance_read: &dyn AccountBalanceRead,
    requested_balances: &BTreeMap<AccumulatorObjId, u64>,
) -> SuiResult<()> {
    for (object_id, requested_balance) in requested_balances {
        let actual_balance = balance_read.get_latest_account_balance(object_id);

        if actual_balance < *requested_balance as u128 {
            return Err(SuiErrorKind::UserInputError {
                error: UserInputError::InvalidWithdrawReservation {
                    error: format!(
                        "Available balance for object id {} is less than requested: {} < {}",
                        object_id, actual_balance, requested_balance
                    ),
                },
            }
            .into());
        }
    }

    Ok(())
}
