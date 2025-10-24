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
    accumulator_root::{AccumulatorObjId, AccumulatorValue},
    base_types::SequenceNumber,
    storage::ObjectStore,
};

pub(crate) trait AccountBalanceRead: Send + Sync {
    /// Given an account ID, return the latest balance and the current version of the account object.
    fn get_latest_account_balance(
        &self,
        account_id: &AccumulatorObjId,
    ) -> Option<(u128, SequenceNumber)>;
}

impl AccountBalanceRead for Arc<dyn ObjectStore + Send + Sync> {
    fn get_latest_account_balance(
        &self,
        account_id: &AccumulatorObjId,
    ) -> Option<(u128, SequenceNumber)> {
        AccumulatorValue::load_latest_by_id(self.as_ref(), *account_id).expect("read cannot fail")
    }
}

impl AccountBalanceRead for &Arc<dyn ObjectStore + Send + Sync> {
    fn get_latest_account_balance(
        &self,
        account_id: &AccumulatorObjId,
    ) -> Option<(u128, SequenceNumber)> {
        AccumulatorValue::load_latest_by_id(self.as_ref(), *account_id).expect("read cannot fail")
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
    balances: BTreeMap<AccumulatorObjId, BTreeMap<SequenceNumber, u128>>,
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
                    BTreeMap::from_iter([(init_version, *balance)]),
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
                .get_latest_account_balance(&account_id)
                .map(|(balance, _)| balance)
                .unwrap_or_default();
            let new_balance = balance as i128 + balance_change;
            assert!(new_balance >= 0);
            self.balances
                .entry(account_id)
                .or_default()
                .insert(new_accumulator_version, new_balance as u128);
        }
    }

    fn get_latest_account_balance(
        &self,
        account_id: &AccumulatorObjId,
    ) -> Option<(u128, SequenceNumber)> {
        let account_balances = self.balances.get(account_id)?;
        account_balances
            .last_key_value()
            .map(|(version, balance)| (*balance, *version))
    }
}

#[cfg(test)]
impl AccountBalanceRead for MockBalanceRead {
    /// Mimic the behavior of child object read.
    /// Find the balance for the given account at the max version
    /// less or equal to the given accumulator version.
    fn get_latest_account_balance(
        &self,
        account_id: &AccumulatorObjId,
    ) -> Option<(u128, SequenceNumber)> {
        let inner = self.inner.read();
        inner.get_latest_account_balance(account_id)
    }
}
