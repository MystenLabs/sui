// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
use std::collections::BTreeMap;
use std::sync::Arc;

#[cfg(test)]
use parking_lot::RwLock;
#[cfg(test)]
use sui_types::base_types::ObjectID;
use sui_types::{
    accumulator_root::{AccumulatorObjId, AccumulatorValue, U128},
    base_types::SequenceNumber,
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
}

impl AccountBalanceRead for Arc<dyn ChildObjectResolver + Send + Sync> {
    fn get_account_balance(
        &self,
        account_id: &AccumulatorObjId,
        accumulator_version: SequenceNumber,
    ) -> u128 {
        let value: U128 =
            AccumulatorValue::load_by_id(self.as_ref(), Some(accumulator_version), *account_id)
                // Expect is safe because at this point we should know that we are dealing with a Balance<T>
                // object
                .expect("read cannot fail")
                .unwrap_or(U128 { value: 0 });

        value.value
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
    cur_version: SequenceNumber,
    balances: BTreeMap<AccumulatorObjId, BTreeMap<SequenceNumber, Option<u128>>>,
}

#[cfg(test)]
impl MockBalanceRead {
    pub(crate) fn new(
        init_version: SequenceNumber,
        init_balances: BTreeMap<ObjectID, u128>,
    ) -> Self {
        let balances = init_balances
            .iter()
            .map(|(account_id, balance)| {
                (
                    AccumulatorObjId::new_unchecked(*account_id),
                    BTreeMap::from_iter([(init_version, Some(*balance))]),
                )
            })
            .collect::<BTreeMap<_, _>>();
        Self {
            inner: Arc::new(RwLock::new(MockBalanceReadInner {
                cur_version: init_version,
                balances,
            })),
        }
    }

    pub(crate) fn cur_version(&self) -> SequenceNumber {
        let inner = self.inner.read();
        inner.cur_version
    }

    pub(crate) fn settle_balance_changes(
        &self,
        balance_changes: BTreeMap<AccumulatorObjId, i128>,
        next_accumulator_version: SequenceNumber,
    ) {
        let mut inner = self.inner.write();
        inner.settle_balance_changes(balance_changes, next_accumulator_version);
    }
}

#[cfg(test)]
impl MockBalanceReadInner {
    fn settle_balance_changes(
        &mut self,
        balance_changes: BTreeMap<AccumulatorObjId, i128>,
        next_accumulator_version: SequenceNumber,
    ) {
        use tracing::debug;

        debug!(
            ?next_accumulator_version,
            "Updating balance states in MockBalanceRead: {:?}", balance_changes,
        );
        let new_accumulator_version = self.cur_version.next();
        assert_eq!(new_accumulator_version, next_accumulator_version);
        self.cur_version = new_accumulator_version;
        for (account_id, balance_change) in balance_changes {
            let balance = self
                .get_account_balance(&account_id, self.cur_version)
                .unwrap_or_default();
            let new_balance = balance as i128 + balance_change;
            assert!(new_balance >= 0);
            let new_entry = if new_balance == 0 {
                None
            } else {
                Some(new_balance as u128)
            };
            self.balances
                .entry(account_id)
                .or_default()
                .insert(new_accumulator_version, new_entry);
        }
    }

    fn get_account_balance(
        &self,
        account_id: &AccumulatorObjId,
        accumulator_version: SequenceNumber,
    ) -> Option<u128> {
        let account_balances = self.balances.get(account_id)?;
        account_balances
            .range(..=accumulator_version)
            .last()
            .and_then(|(_, balance)| *balance)
    }
}

#[cfg(test)]
impl AccountBalanceRead for MockBalanceRead {
    /// Mimic the behavior of child object read.
    /// Find the balance for the given account at the max version
    /// less or equal to the given accumulator version.
    fn get_account_balance(
        &self,
        account_id: &AccumulatorObjId,
        accumulator_version: SequenceNumber,
    ) -> u128 {
        let inner = self.inner.read();
        inner
            .get_account_balance(account_id, accumulator_version)
            .unwrap_or_default()
    }
}
