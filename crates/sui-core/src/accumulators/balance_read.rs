// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use sui_types::{
    accumulator_root::{AccumulatorObjId, AccumulatorValue, U128},
    base_types::SequenceNumber,
    error::{SuiErrorKind, SuiResult, UserInputError},
    storage::ChildObjectResolver,
};

pub(crate) trait AccountBalanceRead: Send + Sync {
    fn get_account_balance(
        &self,
        account_id: &AccumulatorObjId,
        // Version of the accumulator root object, used to
        // bound the version when we look for child account objects.
        accumulator_version: SequenceNumber,
    ) -> u128;

    /// Gets latest balance, without a version bound on the accumulator root object.
    /// Only used for signing time checks / RPC reads, not scheduling.
    fn get_latest_account_balance(&self, account_id: &AccumulatorObjId) -> u128;

    /// Checks if balances are available in the latest versions of the referenced acccumulator
    /// objects. This does un-sequenced reads and can only be used on the signing/voting path
    /// where deterministic results are not required.
    fn check_balances_available(
        &self,
        requested_balances: &BTreeMap<AccumulatorObjId, u64>,
    ) -> SuiResult {
        for (object_id, requested_balance) in requested_balances {
            let actual_balance = self.get_latest_account_balance(object_id);

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
}

impl AccountBalanceRead for Arc<dyn ChildObjectResolver + Send + Sync> {
    fn get_account_balance(
        &self,
        account_id: &AccumulatorObjId,
        accumulator_version: SequenceNumber,
    ) -> u128 {
        // TODO: The implementation currently relies on the fact that we could
        // load older versions of child objects. This has two problems:
        // 1. Aggressive pruning might prune old versions of child objects,
        // 2. Tidehunter might not continue to support this kinds of reads.
        // To fix this, we could also read the latest version of the accumulator root object,
        // and see if the provided accumulator version is already settled.
        let value: U128 =
            AccumulatorValue::load_by_id(self.as_ref(), Some(accumulator_version), *account_id)
                // Expect is safe because at this point we should know that we are dealing with a Balance<T>
                // object
                .expect("read cannot fail")
                .unwrap_or(U128 { value: 0 });

        value.value
    }

    fn get_latest_account_balance(&self, account_id: &AccumulatorObjId) -> u128 {
        let value = AccumulatorValue::load_by_id(self.as_ref(), None, *account_id)
            .expect("read cannot fail")
            .unwrap_or(U128 { value: 0 });

        value.value
    }
}
