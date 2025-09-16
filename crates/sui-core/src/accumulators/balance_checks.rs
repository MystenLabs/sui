// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use sui_types::{
    accumulator_root::{AccumulatorObjId, AccumulatorValue, U128},
    error::{SuiError, SuiResult, UserInputError},
    storage::ChildObjectResolver,
};

/// Checks if balances are available in the latest versions of the referenced acccumulator
/// objects. This does un-sequenced reads and can only be used on the signing/voting path
/// where deterministic results are not required.
pub fn check_balances_available(
    child_object_resolver: &dyn ChildObjectResolver,
    requested_balances: &BTreeMap<AccumulatorObjId, u64>,
) -> SuiResult<()> {
    for (object_id, balance) in requested_balances {
        let accum_value: U128 =
            AccumulatorValue::load_by_id(child_object_resolver, None, *object_id)?.ok_or_else(
                || SuiError::UserInputError {
                    error: UserInputError::InvalidWithdrawReservation {
                        error: format!("balance for object id {} is not found", object_id),
                    },
                },
            )?;

        if accum_value.value < *balance as u128 {
            return Err(SuiError::UserInputError {
                error: UserInputError::InvalidWithdrawReservation {
                    error: format!(
                        "Available balance for object id {} is less than requested: {} < {}",
                        object_id, accum_value.value, balance
                    ),
                },
            });
        }
    }

    Ok(())
}
