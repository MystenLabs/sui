// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use parking_lot::RwLock;
use sui_types::base_types::ObjectID;
use sui_types::{accumulator_root::AccumulatorObjId, base_types::SequenceNumber};

use crate::accumulators::funds_read::AccountFundsRead;

// Mock implementation of a funds accumulator account book for testing.
// Allows setting the funds for a given account at different accumulator versions.
pub(crate) struct MockFundsRead {
    inner: Arc<RwLock<MockFundsReadInner>>,
}

struct MockFundsReadInner {
    cur_version: SequenceNumber,
    amounts: BTreeMap<AccumulatorObjId, BTreeMap<SequenceNumber, Option<u128>>>,
}

impl MockFundsRead {
    pub(crate) fn new(
        init_version: SequenceNumber,
        init_amounts: BTreeMap<ObjectID, u128>,
    ) -> Self {
        let amounts = init_amounts
            .iter()
            .map(|(account_id, amount)| {
                (
                    AccumulatorObjId::new_unchecked(*account_id),
                    BTreeMap::from_iter([(init_version, Some(*amount))]),
                )
            })
            .collect::<BTreeMap<_, _>>();
        Self {
            inner: Arc::new(RwLock::new(MockFundsReadInner {
                cur_version: init_version,
                amounts,
            })),
        }
    }

    pub(crate) fn cur_version(&self) -> SequenceNumber {
        let inner = self.inner.read();
        inner.cur_version
    }

    pub(crate) fn settle_funds_changes(
        &self,
        funds_changes: BTreeMap<AccumulatorObjId, i128>,
        next_accumulator_version: SequenceNumber,
    ) {
        let mut inner = self.inner.write();
        inner.settle_funds_changes(funds_changes, next_accumulator_version);
    }
}

impl MockFundsReadInner {
    fn settle_funds_changes(
        &mut self,
        funds_changes: BTreeMap<AccumulatorObjId, i128>,
        next_accumulator_version: SequenceNumber,
    ) {
        use tracing::debug;

        debug!(
            ?next_accumulator_version,
            "Updating funds states in MockFundsRead: {:?}", funds_changes,
        );
        let new_accumulator_version = self.cur_version.next();
        assert_eq!(new_accumulator_version, next_accumulator_version);
        self.cur_version = new_accumulator_version;
        for (account_id, balance_change) in funds_changes {
            let balance = self
                .get_account_amount(&account_id, self.cur_version)
                .unwrap_or_default();
            let new_balance = balance as i128 + balance_change;
            assert!(new_balance >= 0);
            let new_entry = if new_balance == 0 {
                None
            } else {
                Some(new_balance as u128)
            };
            self.amounts
                .entry(account_id)
                .or_default()
                .insert(new_accumulator_version, new_entry);
        }
    }

    fn get_account_amount(
        &self,
        account_id: &AccumulatorObjId,
        accumulator_version: SequenceNumber,
    ) -> Option<u128> {
        let account_amounts = self.amounts.get(account_id)?;
        account_amounts
            .range(..=accumulator_version)
            .last()
            .and_then(|(_, amount)| *amount)
    }

    fn get_latest_account_amount(&self, account_id: &AccumulatorObjId) -> (u128, SequenceNumber) {
        let account_amounts = self.amounts.get(account_id);
        match account_amounts {
            Some(amounts) => {
                let (version, amount) = amounts.iter().last().unwrap();
                (amount.unwrap_or(0), *version)
            }
            None => (0, self.cur_version),
        }
    }

    fn get_account_amount_at_version(
        &self,
        account_id: &AccumulatorObjId,
        version: SequenceNumber,
    ) -> u128 {
        let account_amounts = self.amounts.get(account_id);
        match account_amounts {
            Some(amounts) => {
                let (_, amount) = amounts.range(..=version).last().unwrap();
                amount.unwrap_or(0)
            }
            None => 0,
        }
    }
}

impl AccountFundsRead for MockFundsRead {
    fn get_latest_account_amount(&self, account_id: &AccumulatorObjId) -> (u128, SequenceNumber) {
        let inner = self.inner.read();
        inner.get_latest_account_amount(account_id)
    }

    fn get_account_amount_at_version(
        &self,
        account_id: &AccumulatorObjId,
        version: SequenceNumber,
    ) -> u128 {
        let inner = self.inner.read();
        inner.get_account_amount_at_version(account_id, version)
    }
}
