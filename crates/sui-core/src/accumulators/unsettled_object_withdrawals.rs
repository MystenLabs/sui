// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use parking_lot::RwLock;
use sui_types::{accumulator_root::AccumulatorObjId, base_types::SequenceNumber};

use crate::accumulators::object_funds_checker::metrics::ObjectFundsCheckerMetrics;

pub struct UnsettledObjectWithdrawals {
    inner: RwLock<Inner>,
    metrics: Arc<ObjectFundsCheckerMetrics>,
}

#[derive(Default)]
struct Inner {
    /// Tracks the amount of pending unsettled withdraws for each account at each accumulator version.
    /// When we check object funds sufficiency, we read the balance bounded by the withdraw accumulator version.
    /// Balance are updated only by settlement transactions, not when we withdraw funds.
    /// Hence when we are checking object funds, on top of the settled balance, we also need to account for
    /// the amount of withdraws from the same consensus commit (that all reads from the same accumulator version).
    unsettled_withdraws: BTreeMap<AccumulatorObjId, BTreeMap<SequenceNumber, u128>>,
    /// Tracks the accounts that have pending withdraws at each accumulator version.
    /// This information is not required for functional correctness, but needed to garbage collect
    /// unused entries in unsettled_withdraws that are now fully committed. Without doing so unsettled_withdraws
    /// may grow unbounded.
    unsettled_accounts: BTreeMap<SequenceNumber, BTreeSet<AccumulatorObjId>>,
}

impl UnsettledObjectWithdrawals {
    pub fn new(metrics: Arc<ObjectFundsCheckerMetrics>) -> Self {
        Self {
            inner: RwLock::new(Inner::default()),
            metrics,
        }
    }

    /// Get the current unsettled withdraw amount for an account at a given accumulator version.
    pub(crate) fn get_unsettled_object_withdraw(
        &self,
        account: &AccumulatorObjId,
        accumulator_version: SequenceNumber,
    ) -> u128 {
        self.inner
            .read()
            .unsettled_withdraws
            .get(account)
            .and_then(|withdraws| withdraws.get(&accumulator_version))
            .copied()
            .unwrap_or_default()
    }

    /// Record a new unsettled withdraw for an account at a given accumulator version.
    /// This updates the unsettled withdraws map, and is called after a transaction successfully executed
    /// that withdraws funds from an object balance account.
    pub(crate) fn record_unsettled_withdraws<'a>(
        &self,
        withdraws: impl Iterator<Item = (&'a AccumulatorObjId, &'a u128)>,
        accumulator_version: SequenceNumber,
    ) {
        let mut inner = self.inner.write();
        for (account, amount) in withdraws {
            let entry = inner
                .unsettled_withdraws
                .entry(*account)
                .or_default()
                .entry(accumulator_version)
                .or_default();
            *entry = entry.checked_add(*amount).unwrap();
            inner
                .unsettled_accounts
                .entry(accumulator_version)
                .or_default()
                .insert(*account);
        }
        self.update_unsettled_metrics(&inner);
    }

    fn update_unsettled_metrics(&self, inner: &Inner) {
        self.metrics
            .unsettled_accounts
            .set(inner.unsettled_withdraws.len() as i64);
        self.metrics
            .unsettled_versions
            .set(inner.unsettled_accounts.len() as i64);
    }

    /// Garbage collect tracking of unsettled withdraws for committed accumulator versions.
    /// This isn't required for functional correctness, but ensures that the tracking data structure doesn't grow unbounded.
    pub fn commit_accumulator_versions(&self, committed_accumulator_versions: Vec<SequenceNumber>) {
        let mut inner = self.inner.write();
        for accumulator_version in committed_accumulator_versions {
            let accounts = inner
                .unsettled_accounts
                .remove(&accumulator_version)
                .unwrap_or_default();
            for account in accounts {
                if let Some(withdraws) = inner.unsettled_withdraws.get_mut(&account) {
                    withdraws.remove(&accumulator_version);
                    if withdraws.is_empty() {
                        inner.unsettled_withdraws.remove(&account);
                    }
                }
            }
        }
        self.update_unsettled_metrics(&inner);
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        let inner = self.inner.read();
        inner.unsettled_withdraws.is_empty() && inner.unsettled_accounts.is_empty()
    }
}
