// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
use std::collections::BTreeMap;
use std::sync::Arc;

use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    SUI_ACCUMULATOR_ROOT_OBJECT_ID,
};

use crate::execution_cache::ObjectCacheRead;

pub(crate) trait AccountBalanceRead: Send + Sync {
    fn get_accumulator_version(&self) -> SequenceNumber;

    fn get_account_balance(
        &self,
        account_id: &ObjectID,
        // Version of the accumulator root object, used to
        // bound the version when we look for child account objects.
        accumulator_version: SequenceNumber,
    ) -> u64;
}

impl AccountBalanceRead for Arc<dyn ObjectCacheRead> {
    fn get_accumulator_version(&self) -> SequenceNumber {
        self.get_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
            .expect("Accumulator object must exist")
            .version()
    }

    fn get_account_balance(
        &self,
        account_id: &ObjectID,
        accumulator_version: SequenceNumber,
    ) -> u64 {
        self.find_object_lt_or_eq_version(*account_id, accumulator_version)
            .map(|_obj| {
                // TODO: Get the balance from the object.
                0
            })
            .unwrap_or_default()
    }
}

// Mock implementation of a balance account book for testing.
// Allows setting the balance for a given account at different accumulator versions.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct MockBalanceRead {
    accumulator_version: SequenceNumber,
    balances: BTreeMap<ObjectID, BTreeMap<SequenceNumber, u64>>,
}

#[cfg(test)]
impl MockBalanceRead {
    pub(crate) fn new(init_accumulator_version: SequenceNumber) -> Self {
        Self {
            accumulator_version: init_accumulator_version,
            balances: BTreeMap::new(),
        }
    }

    pub(crate) fn settle_balance_changes(
        &mut self,
        new_accumulator_version: SequenceNumber,
        balance_changes: BTreeMap<ObjectID, i128>,
    ) {
        assert!(new_accumulator_version > self.accumulator_version);
        self.accumulator_version = new_accumulator_version;
        for (account_id, balance_change) in balance_changes {
            self.adjust_balance(account_id, new_accumulator_version, balance_change);
        }
    }

    fn adjust_balance(
        &mut self,
        account_id: ObjectID,
        version: SequenceNumber,
        balance_change: i128,
    ) {
        let balance = self.get_account_balance(&account_id, version);
        let new_balance = balance as i128 + balance_change;
        assert!(new_balance >= 0);
        self.balances
            .entry(account_id)
            .or_default()
            .insert(version, new_balance as u64);
    }
}

#[cfg(test)]
impl AccountBalanceRead for MockBalanceRead {
    fn get_accumulator_version(&self) -> SequenceNumber {
        unimplemented!()
    }

    /// Mimic the behavior of child object read.
    /// Find the balance for the given account at the max version
    /// less or equal to the given accumulator version.
    fn get_account_balance(
        &self,
        account_id: &ObjectID,
        accumulator_version: SequenceNumber,
    ) -> u64 {
        let Some(account_balances) = self.balances.get(account_id) else {
            return 0;
        };
        account_balances
            .range(..=accumulator_version)
            .last()
            .map(|(_, balance)| *balance)
            .unwrap_or_default()
    }
}
