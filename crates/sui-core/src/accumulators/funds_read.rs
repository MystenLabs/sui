// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use sui_types::{
    accumulator_root::AccumulatorObjId,
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
        requested_amounts: &BTreeMap<AccumulatorObjId, u64>,
    ) -> SuiResult {
        for (object_id, requested_amount) in requested_amounts {
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
}
