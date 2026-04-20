// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use move_core_types::language_storage::TypeTag;
use sui_types::{
    accumulator_root::AccumulatorObjId,
    balance::Balance,
    base_types::SequenceNumber,
    error::{SuiErrorKind, SuiResult, UserInputError},
};

pub trait AccountFundsRead: Send + Sync {
    /// Gets latest amount in account together with the version of the accumulator root object.
    /// If the account does not exist, returns the current root accumulator version.
    /// It guarantees no data race between the read of the account object and the root accumulator version.
    fn get_latest_account_amount(&self, account_id: &AccumulatorObjId) -> (u128, SequenceNumber);

    /// Read the amount at a precise version. Care must be taken to only call this function if we
    /// can guarantee that objects behind this version have not yet been pruned.
    fn get_account_amount_at_version(
        &self,
        account_id: &AccumulatorObjId,
        version: SequenceNumber,
    ) -> u128;

    /// Checks if given amounts are available in the latest versions of the referenced acccumulator
    /// objects. This does un-sequenced reads and can only be used on the signing/voting path
    /// where deterministic results are not required.
    fn check_amounts_available(
        &self,
        requested_amounts: &BTreeMap<AccumulatorObjId, (u64, TypeTag)>,
    ) -> SuiResult {
        for (object_id, (requested_amount, _type_tag)) in requested_amounts {
            let (actual_amount, _) = self.get_latest_account_amount(object_id);

            if actual_amount < *requested_amount as u128 {
                return Err(SuiErrorKind::UserInputError {
                    error: UserInputError::InvalidWithdrawReservation {
                        error: format!(
                            "Available amount in account for object id {} is less than requested: {} < {}",
                            object_id, actual_amount, requested_amount
                        ),
                    },
                }
                .into());
            }
        }

        Ok(())
    }

    /// For gasless transactions, checks that withdrawing the requested amounts does not leave
    /// a balance below the minimum in the sender's account. For each withdrawal, the remaining balance
    /// (actual - requested) must be either 0 or >= the minimum transfer amount for that
    /// token type.
    fn check_remaining_amounts_after_withdrawal(
        &self,
        requested_amounts: &BTreeMap<AccumulatorObjId, (u64, TypeTag)>,
        min_amounts: &BTreeMap<TypeTag, u64>,
    ) -> SuiResult {
        for (object_id, (requested_amount, type_tag)) in requested_amounts {
            let (actual_amount, _) = self.get_latest_account_amount(object_id);
            let remaining = actual_amount.saturating_sub(*requested_amount as u128);
            if remaining == 0 {
                continue;
            }
            let coin_type =
                Balance::maybe_get_balance_type_param(type_tag).unwrap_or_else(|| type_tag.clone());
            if let Some(&min_amount) = min_amounts.get(&coin_type)
                && min_amount > 0
                && remaining < min_amount as u128
            {
                return Err(SuiErrorKind::UserInputError {
                    error: UserInputError::InvalidWithdrawReservation {
                        error: format!(
                            "Invalid gasless withdrawal from {object_id}. \
                             Gasless transactions must either use the entire balance, \
                             or leave at least {min_amount} for token type {coin_type}. \
                             Remaining amount is {remaining}",
                        ),
                    },
                }
                .into());
            }
        }

        Ok(())
    }
}
