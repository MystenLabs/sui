// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use move_core_types::language_storage::TypeTag;
use sui_types::{
    accumulator_root::AccumulatorObjId,
    balance::Balance,
    base_types::{SequenceNumber, SuiAddress},
    error::{SuiErrorKind, SuiResult, UserInputError},
};

pub trait AccountFundsRead: Send + Sync {
    /// Gets the latest amount in an account. If the account does not exist, returns 0.
    /// This does an unsequenced read and does not guarantee consistency with the root accumulator
    /// version.
    fn get_latest_account_amount(&self, account_id: &AccumulatorObjId) -> u128;

    /// Gets the account amount at a version consistent with a stable accumulator root version.
    /// If the account object has advanced ahead of that root version, this returns the amount
    /// at or before the root version instead of the latest account object amount. If the account
    /// does not exist at that root version, returns 0 for the amount.
    /// It guarantees no data race between the read of the account object and the root accumulator version.
    fn get_consistent_latest_account_amount_and_version(
        &self,
        account_id: &AccumulatorObjId,
    ) -> (u128, SequenceNumber);

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
        requested_amounts: &BTreeMap<AccumulatorObjId, (u64, TypeTag, SuiAddress)>,
    ) -> SuiResult {
        for (object_id, (requested_amount, type_tag, owner)) in requested_amounts {
            let actual_amount = self.get_latest_account_amount(object_id);

            if actual_amount < *requested_amount as u128 {
                let coin_type = Balance::maybe_get_balance_type_param(type_tag)
                    .unwrap_or_else(|| type_tag.clone());
                return Err(SuiErrorKind::UserInputError {
                    error: UserInputError::InvalidWithdrawReservation {
                        error: format!(
                            "Insufficient address balance of coin type {coin_type} \
                             for address {owner}: the transaction requires \
                             {requested_amount} but only {actual_amount} is available. \
                             Note that the address balance does not include funds held \
                             in Coin objects owned by the address; to spend those funds, \
                             use the Coin objects directly as transaction inputs.",
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
        requested_amounts: &BTreeMap<AccumulatorObjId, (u64, TypeTag, SuiAddress)>,
        min_amounts: &BTreeMap<TypeTag, u64>,
    ) -> SuiResult {
        for (object_id, (requested_amount, type_tag, owner)) in requested_amounts {
            let actual_amount = self.get_latest_account_amount(object_id);
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
                            "Invalid gasless withdrawal of coin type {coin_type} \
                             from address {owner}. \
                             Gasless transactions must either use the entire address \
                             balance, or leave at least {min_amount}. \
                             Remaining amount would be {remaining}",
                        ),
                    },
                }
                .into());
            }
        }

        Ok(())
    }
}
