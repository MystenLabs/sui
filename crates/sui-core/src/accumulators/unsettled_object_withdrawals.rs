// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use parking_lot::RwLock;
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID,
    accumulator_root::{AccumulatorObjId, UnsettledObjectFundsRead},
    base_types::SequenceNumber,
    effects::{TransactionEffects, TransactionEffectsAPI},
};

use crate::accumulators::object_funds_checker::metrics::ObjectFundsCheckerMetrics;

/// Tracks object-funds withdrawals that have executed but not yet settled into the accumulator
/// root. Withdrawals only change on-chain balances when a settlement transaction runs, so any
/// balance read bounded by an accumulator version must additionally account for the withdrawals
/// recorded here at that version.
///
/// Two consumers share this store: the in-execution sufficiency check (the Move VM reads it
/// through [`UnsettledObjectFundsRead`], and the authority records a successful transaction's net
/// withdrawals after execution), and the post-execution [`ObjectFundsCheckerDEPRECATED`] path (which checks
/// and records through its own logic). Entries are garbage-collected at checkpoint commit once
/// their accumulator version has settled.
///
/// [`ObjectFundsCheckerDEPRECATED`]: crate::accumulators::object_funds_checker::ObjectFundsCheckerDEPRECATED
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
    /// When `record_net_unsettled_object_withdraws` is enabled, the recorded amounts are the per-account
    /// net withdraws from effects (what settlement will actually deduct); otherwise they are the
    /// running max withdraws.
    unsettled_withdraws: BTreeMap<AccumulatorObjId, BTreeMap<SequenceNumber, u128>>,
    /// Tracks the accounts that have pending withdraws at each accumulator version.
    /// This information is not required for functional correctness, but needed to garbage collect
    /// unused entries in unsettled_withdraws that are now fully committed. Without doing so unsettled_withdraws
    /// may grow unbounded.
    unsettled_accounts: BTreeMap<SequenceNumber, BTreeSet<AccumulatorObjId>>,
}

impl UnsettledObjectFundsRead for UnsettledObjectWithdrawals {
    fn get_unsettled_object_withdraw(
        &self,
        account: &AccumulatorObjId,
        accumulator_version: SequenceNumber,
    ) -> u128 {
        UnsettledObjectWithdrawals::get_unsettled_object_withdraw(
            self,
            account,
            accumulator_version,
        )
    }
}

impl UnsettledObjectWithdrawals {
    pub fn new(metrics: Arc<ObjectFundsCheckerMetrics>) -> Self {
        Self {
            inner: RwLock::new(Inner::default()),
            metrics,
        }
    }

    /// Total amount withdrawn from `account` against `accumulator_version` by transactions that
    /// have executed but not yet settled. Returns 0 if there are none.
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

    /// Records object withdraws as unsettled at `accumulator_version`, so later transactions in the
    /// same consensus commit (which read the same version) account for them on top of the settled
    /// balance.
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

    /// Garbage-collects the unsettled-withdraw entries for the accumulator versions that the given
    /// committed effects settled. Called from the checkpoint executor at commit time, when every
    /// transaction in the checkpoint has executed, so no funds check can still read them.
    /// This is a memory optimization, not required for correctness: each transaction reads unsettled
    /// withdraws at its own required version, which is not GC'd until it settles.
    pub fn commit_effects<'a>(
        &self,
        committed_effects: impl Iterator<Item = &'a TransactionEffects>,
    ) {
        // Called on every checkpoint commit on every node; skip the effects scan when there is
        // nothing to garbage-collect.
        {
            let inner = self.inner.read();
            if inner.unsettled_withdraws.is_empty() && inner.unsettled_accounts.is_empty() {
                return;
            }
        }
        let committed_accumulator_versions = committed_effects
            .filter_map(|effects| {
                effects.object_changes().into_iter().find_map(|o| {
                    if o.id == SUI_ACCUMULATOR_ROOT_OBJECT_ID {
                        o.input_version
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>();
        self.commit_accumulator_versions(committed_accumulator_versions);
    }

    pub(crate) fn commit_accumulator_versions(
        &self,
        committed_accumulator_versions: Vec<SequenceNumber>,
    ) {
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
