// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use parking_lot::RwLock;
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID,
    accumulator_root::AccumulatorObjId,
    base_types::SequenceNumber,
    effects::{TransactionEffects, TransactionEffectsAPI},
};

use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::transaction::TransactionDataAPI;

use crate::{
    accumulators::object_funds_checker::metrics::ObjectFundsCheckerMetrics,
    authority::authority_per_epoch_store::AuthorityPerEpochStore,
};

/// A transaction's withdraw amounts relevant to unsettled-withdrawal accounting, computed by
/// [`compute_unsettled_withdraw_updates`].
pub(crate) struct UnsettledWithdrawUpdates {
    /// Peak withdraw exposure per object account at any point during execution
    /// (address-reservation accounts excluded). This is what a sufficiency check must cover.
    pub object_running_max_withdraws: BTreeMap<AccumulatorObjId, u128>,
    /// The amounts to record as unsettled: the per-account net withdraws from the effects (what
    /// settlement will actually deduct) when `record_net_unsettled_object_withdraws` is enabled,
    /// otherwise the running max.
    pub unsettled_withdraw_updates: BTreeMap<AccumulatorObjId, u128>,
}

/// Splits a transaction's running-max withdraws into the object-account peaks a sufficiency check
/// must cover and the amounts to record as unsettled once the transaction commits.
pub(crate) fn compute_unsettled_withdraw_updates(
    certificate: &VerifiedExecutableTransaction,
    effects: &TransactionEffects,
    accumulator_running_max_withdraws: &BTreeMap<AccumulatorObjId, u128>,
    epoch_store: &Arc<AuthorityPerEpochStore>,
) -> UnsettledWithdrawUpdates {
    // Address-reservation withdraws are settled separately; among all withdraws (which show up as
    // accumulator events with integer values), only those from accounts without a funds
    // reservation are object withdraws.
    let address_funds_reservations: BTreeSet<_> = certificate
        .transaction_data()
        .process_funds_withdrawals_for_execution(epoch_store.get_chain_identifier())
        .into_keys()
        .collect();
    let object_running_max_withdraws: BTreeMap<_, _> = accumulator_running_max_withdraws
        .clone()
        .into_iter()
        .filter(|(account, _)| !address_funds_reservations.contains(account))
        .collect();
    // The sufficiency check must use the running max withdraws (the peak withdraw exposure at any
    // point during execution), but the amount that settlement will actually deduct from each
    // account is the net amount recorded in the effects. E.g. a tx that withdraws 10 and deposits
    // 10 back has a running max of 10 but nets to 0. Recording the running max as unsettled would
    // over-count against other withdraws in the same consensus commit.
    let unsettled_withdraw_updates = if epoch_store
        .protocol_config()
        .record_net_unsettled_object_withdraws()
    {
        let updates: BTreeMap<_, _> = effects
            .accumulator_events()
            .into_iter()
            .filter(|event| !address_funds_reservations.contains(&event.accumulator_obj))
            .filter_map(|event| {
                event
                    .write
                    .get_fund_withdraw_amount()
                    // A zero-amount withdraw emits a single Split(0) accumulator event, which
                    // survives effects folding as a Split (the fold's Merge tie-break only
                    // applies when an account has multiple writes). It contributes nothing to
                    // the running max nor to settlement, so recording it would be a no-op;
                    // skip it.
                    .filter(|amount| *amount > 0)
                    .map(|amount| (event.accumulator_obj, amount))
            })
            .collect();
        // A positive net withdraw in effects implies a positive peak, so the account must have
        // a running max entry that the net cannot exceed. Recording more than what the
        // sufficiency check covered could break the funds >= unsettled_withdraw invariant.
        debug_assert!(
            updates.iter().all(|(obj_id, net)| {
                object_running_max_withdraws
                    .get(obj_id)
                    .is_some_and(|max| net <= max)
            }),
            "net withdraw exceeds running max: tx={:?} updates={:?} running_max={:?}",
            certificate.digest(),
            updates,
            object_running_max_withdraws,
        );
        updates
    } else {
        object_running_max_withdraws.clone()
    };
    UnsettledWithdrawUpdates {
        object_running_max_withdraws,
        unsettled_withdraw_updates,
    }
}

/// Tracks object-funds withdrawals that have executed but not yet settled into the accumulator
/// root. Withdrawals only change on-chain balances when a settlement transaction runs, so any
/// balance read bounded by an accumulator version must additionally account for the withdrawals
/// recorded here at that version.
///
/// The post-execution [`ObjectFundsChecker`] checks and records withdrawals through this store.
/// Entries are garbage-collected at checkpoint commit once their accumulator version has settled.
///
/// [`ObjectFundsChecker`]: crate::accumulators::object_funds_checker::ObjectFundsChecker
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
