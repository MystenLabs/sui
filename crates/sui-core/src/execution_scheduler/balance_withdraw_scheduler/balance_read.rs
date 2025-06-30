// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
use std::collections::BTreeMap;
use std::sync::Arc;

#[cfg(test)]
use parking_lot::RwLock;
use sui_types::{
    accumulator_root::get_balance_from_account_for_testing,
    base_types::{ObjectID, SequenceNumber},
};

use crate::execution_cache::ObjectCacheRead;

pub(crate) trait AccountBalanceRead: Send + Sync {
    fn get_account_balance(
        &self,
        account_id: &ObjectID,
        // Version of the accumulator root object, used to
        // bound the version when we look for child account objects.
        accumulator_version: SequenceNumber,
    ) -> u64;
}

impl AccountBalanceRead for Arc<dyn ObjectCacheRead> {
    fn get_account_balance(
        &self,
        account_id: &ObjectID,
        accumulator_version: SequenceNumber,
    ) -> u64 {
        self.find_object_lt_or_eq_version(*account_id, accumulator_version)
            .map(|obj| get_balance_from_account_for_testing(&obj))
            .unwrap_or_default()
    }
}

// Mock implementation of a balance account book for testing.
// Allows setting the balance for a given account at different accumulator versions.
#[cfg(test)]
pub(crate) struct MockBalanceRead {
    inner: Arc<RwLock<MockBalanceReadInner>>,
}

#[cfg(test)]
struct MockBalanceReadInner {
    balances: BTreeMap<ObjectID, BTreeMap<SequenceNumber, u64>>,
}

#[cfg(test)]
impl MockBalanceRead {
    pub(crate) fn new(
        init_version: SequenceNumber,
        init_balances: BTreeMap<ObjectID, u64>,
    ) -> Self {
        let read = Self {
            inner: Arc::new(RwLock::new(MockBalanceReadInner {
                balances: BTreeMap::new(),
            })),
        };
        let balance_changes = init_balances
            .iter()
            .map(|(account_id, balance)| (*account_id, *balance as i128))
            .collect::<BTreeMap<_, _>>();
        read.settle_balance_changes(init_version, balance_changes);
        read
    }

    pub(crate) fn settle_balance_changes(
        &self,
        new_accumulator_version: SequenceNumber,
        balance_changes: BTreeMap<ObjectID, i128>,
    ) {
        self.inner
            .write()
            .settle_balance_changes(new_accumulator_version, balance_changes);
    }
}

#[cfg(test)]
impl MockBalanceReadInner {
    fn settle_balance_changes(
        &mut self,
        new_accumulator_version: SequenceNumber,
        balance_changes: BTreeMap<ObjectID, i128>,
    ) {
        for (account_id, balance_change) in balance_changes {
            let balance = self.get_account_balance(&account_id, new_accumulator_version);
            let new_balance = balance as i128 + balance_change;
            assert!(new_balance >= 0);
            self.balances
                .entry(account_id)
                .or_default()
                .insert(new_accumulator_version, new_balance as u64);
        }
    }

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

#[cfg(test)]
impl AccountBalanceRead for MockBalanceRead {
    /// Mimic the behavior of child object read.
    /// Find the balance for the given account at the max version
    /// less or equal to the given accumulator version.
    fn get_account_balance(
        &self,
        account_id: &ObjectID,
        accumulator_version: SequenceNumber,
    ) -> u64 {
        let inner = self.inner.read();
        inner.get_account_balance(account_id, accumulator_version)
    }
}
