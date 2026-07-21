// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use mysten_common::assert_reachable;
use sui_types::{
    accumulator_root::AccumulatorObjId,
    base_types::SequenceNumber,
    effects::{TransactionEffects, TransactionEffectsAPI},
    executable_transaction::VerifiedExecutableTransaction,
    execution_params::FundsWithdrawStatus,
};
use tokio::{
    sync::{oneshot, watch},
    time::Instant,
};
use tracing::{debug, instrument};

use crate::{
    accumulators::{
        funds_read::AccountFundsRead,
        unsettled_object_withdrawals::{
            UnsettledObjectWithdrawals, compute_unsettled_withdraw_updates,
        },
    },
    authority::{ExecutionEnv, authority_per_epoch_store::AuthorityPerEpochStore},
    execution_scheduler::ExecutionScheduler,
};

#[cfg(test)]
mod integration_tests;
pub mod metrics;
#[cfg(test)]
mod unit_tests;

/// Note that there is no need to have a separate InsufficientFunds variant.
/// If the funds are insufficient, the execution would still have to abort and rely on
/// a rescheduling to be able to execute again.
pub enum ObjectFundsWithdrawStatus {
    SufficientFunds,
    // The receiver will be notified when the funds are determined to be sufficient or insufficient.
    // The bool is true if the funds are sufficient, false if the funds are insufficient.
    Pending(oneshot::Receiver<FundsWithdrawStatus>),
}

/// The post-execution object-funds sufficiency checker: decides after execution whether a
/// transaction's object withdrawals are covered, waiting for settlement when the answer is not yet
/// deterministic. The unsettled-withdrawal bookkeeping lives in [`UnsettledObjectWithdrawals`];
/// this type holds only the checking logic and the settlement-version watch that its pending-wait
/// machinery needs.
pub struct ObjectFundsChecker {
    /// Watchers to keep track the last settled accumulator version.
    /// This is updated whenever the settlement barrier transaction is executed.
    last_settled_version_sender: watch::Sender<SequenceNumber>,
    last_settled_version_receiver: watch::Receiver<SequenceNumber>,
    unsettled: Arc<UnsettledObjectWithdrawals>,
    metrics: Arc<metrics::ObjectFundsCheckerMetrics>,
}

impl ObjectFundsChecker {
    pub fn new(
        starting_accumulator_version: SequenceNumber,
        unsettled: Arc<UnsettledObjectWithdrawals>,
        metrics: Arc<metrics::ObjectFundsCheckerMetrics>,
    ) -> Self {
        let (last_settled_version_sender, last_settled_version_receiver) =
            watch::channel(starting_accumulator_version);
        Self {
            last_settled_version_sender,
            last_settled_version_receiver,
            unsettled,
            metrics,
        }
    }

    /// Construct with a store of its own, for tests that exercise the checker in isolation.
    #[cfg(test)]
    pub fn new_for_testing(
        starting_accumulator_version: SequenceNumber,
        metrics: Arc<metrics::ObjectFundsCheckerMetrics>,
    ) -> Self {
        Self::new(
            starting_accumulator_version,
            Arc::new(UnsettledObjectWithdrawals::new(metrics.clone())),
            metrics,
        )
    }

    /// The shared unsettled-withdrawal store (owned by `AuthorityState`).
    #[cfg(test)]
    fn unsettled(&self) -> &Arc<UnsettledObjectWithdrawals> {
        &self.unsettled
    }

    #[instrument(level = "debug", skip_all, fields(tx_digest = ?certificate.digest()))]
    pub fn should_commit_object_funds_withdraws(
        &self,
        certificate: &VerifiedExecutableTransaction,
        effects: &TransactionEffects,
        accumulator_running_max_withdraws: &BTreeMap<AccumulatorObjId, u128>,
        execution_env: &ExecutionEnv,
        funds_read: &Arc<dyn AccountFundsRead>,
        execution_scheduler: &Arc<ExecutionScheduler>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> bool {
        if effects.status().is_err() {
            // This transaction already failed. It does not matter any more
            // whether it has sufficient object funds or not.
            return true;
        }
        // A transaction with object withdraws can only exist when accumulators are enabled for
        // the epoch, and every production path that produces such a transaction also assigns an
        // accumulator version. When there is none — a transaction from before accumulators were
        // enabled, or an end-of-epoch transaction — no object withdraws are possible and there
        // is nothing to check.
        let Some(accumulator_version) = execution_env.assigned_versions.accumulator_version()
        else {
            return true;
        };
        let updates = compute_unsettled_withdraw_updates(
            certificate,
            effects,
            accumulator_running_max_withdraws,
            epoch_store,
        );
        // If there are no object withdraws, we can skip checking object funds.
        if updates.object_running_max_withdraws.is_empty() {
            return true;
        }
        match self.check_object_funds(
            updates.object_running_max_withdraws,
            updates.unsettled_withdraw_updates,
            accumulator_version,
            funds_read.as_ref(),
        ) {
            // Sufficient funds, we can go ahead and commit the execution results as it is.
            ObjectFundsWithdrawStatus::SufficientFunds => {
                assert_reachable!("object funds sufficient");
                debug!("Object funds sufficient, committing effects");
                self.metrics
                    .check_result
                    .with_label_values(&["sufficient"])
                    .inc();
                true
            }
            // Currently insufficient funds. We need to wait until it reach a deterministic state
            // before we can determine if it is really insufficient (to include potential deposits)
            // At that time we will have to re-enqueue the transaction for execution again.
            // Re-enqueue is handled here so the caller does not need to worry about it.
            ObjectFundsWithdrawStatus::Pending(receiver) => {
                self.metrics.pending_checks.inc();
                let timer = self.metrics.pending_check_latency.start_timer();
                let pending_metrics = self.metrics.clone();
                let scheduler = execution_scheduler.clone();
                let cert = certificate.clone();
                let mut execution_env = execution_env.clone();
                let epoch_store = epoch_store.clone();
                tokio::task::spawn(async move {
                    // It is possible that checkpoint executor finished executing
                    // the current epoch and went ahead with epoch change asynchronously,
                    // while this is still waiting.
                    let inner_metrics = pending_metrics.clone();
                    let _ = epoch_store
                        .within_alive_epoch(async move {
                            let tx_digest = cert.digest();
                            match receiver.await {
                                Ok(FundsWithdrawStatus::MaybeSufficient) => {
                                    assert_reachable!("object funds maybe sufficient");
                                    // The withdraw state is now deterministically known,
                                    // so we can enqueue the transaction again and it will check again
                                    // whether it is sufficient or not in the next execution.
                                    // TODO: We should be able to optimize this by avoiding re-execution.
                                    debug!(?tx_digest, "Object funds possibly sufficient");
                                }
                                Ok(FundsWithdrawStatus::Insufficient) => {
                                    assert_reachable!("object funds insufficient");
                                    // Re-enqueue with insufficient funds status, so it will be executed
                                    // in the next execution and fail through early error.
                                    // FIXME: We need to also track the amount of gas that was used,
                                    // so that we could charge properly in the next execution when we
                                    // go through early error. Otherwise we would undercharge.
                                    execution_env = execution_env.with_insufficient_funds();
                                    inner_metrics
                                        .check_result
                                        .with_label_values(&["insufficient"])
                                        .inc();
                                    debug!(?tx_digest, "Object funds insufficient");
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Error receiving funds withdraw status: {:?}",
                                        e
                                    );
                                }
                            }
                            scheduler.send_transaction_for_execution(
                                &cert,
                                execution_env,
                                // TODO: Should the enqueue_time be the original enqueue time
                                // of this transaction?
                                Instant::now(),
                            );
                        })
                        .await;
                    timer.observe_duration();
                    pending_metrics.pending_checks.dec();
                });
                false
            }
        }
    }

    fn check_object_funds(
        &self,
        object_running_max_withdraws: BTreeMap<AccumulatorObjId, u128>,
        unsettled_withdraw_updates: BTreeMap<AccumulatorObjId, u128>,
        accumulator_version: SequenceNumber,
        funds_read: &dyn AccountFundsRead,
    ) -> ObjectFundsWithdrawStatus {
        let last_settled_version = *self.last_settled_version_receiver.borrow();
        if accumulator_version <= last_settled_version {
            // If the version we are withdrawing from is already settled, we have all the information
            // we need to determine if the funds are sufficient or not.
            if self.try_withdraw(
                funds_read,
                &object_running_max_withdraws,
                &unsettled_withdraw_updates,
                accumulator_version,
            ) {
                return ObjectFundsWithdrawStatus::SufficientFunds;
            } else {
                let (sender, receiver) = oneshot::channel();
                // unwrap is safe because the receiver is defined right above.
                sender.send(FundsWithdrawStatus::Insufficient).unwrap();
                return ObjectFundsWithdrawStatus::Pending(receiver);
            }
        }

        // Spawn a task to wait for the last settled version to become accumulator_version,
        // before we could check again.
        let last_settled_version_sender = self.last_settled_version_sender.clone();
        let (sender, receiver) = oneshot::channel();
        tokio::spawn(async move {
            let mut version_receiver = last_settled_version_sender.subscribe();
            // The wait is guaranteed to be notified because we update version after executing each settlement transaction,
            // and every settlement transaction must eventually be executed.
            let res = version_receiver
                .wait_for(|v| *v >= accumulator_version)
                .await;
            if res.is_err() {
                // This shouldn't happen, but just to be safe.
                tracing::error!("Last settled accumulator version receiver channel closed");
                return;
            }
            // We notify the waiter that the funds are now deterministically known,
            // but we don't need to check here whether they are sufficient or not.
            // Next time during execution we will check again.
            let _ = sender.send(FundsWithdrawStatus::MaybeSufficient);
        });
        ObjectFundsWithdrawStatus::Pending(receiver)
    }

    /// Checks that each account can cover its running max withdraw (`object_running_max_withdraws`),
    /// and if so, adds `unsettled_withdraw_updates` to the unsettled withdraws of each account.
    fn try_withdraw(
        &self,
        funds_read: &dyn AccountFundsRead,
        object_running_max_withdraws: &BTreeMap<AccumulatorObjId, u128>,
        unsettled_withdraw_updates: &BTreeMap<AccumulatorObjId, u128>,
        accumulator_version: SequenceNumber,
    ) -> bool {
        for (obj_id, amount) in object_running_max_withdraws {
            let funds = funds_read.get_account_amount_at_version(obj_id, accumulator_version);
            // Reading without holding a lock across check-and-record is safe because no two
            // transactions can be withdrawing from the same account at the same time.
            let unsettled_withdraw = self
                .unsettled
                .get_unsettled_object_withdraw(obj_id, accumulator_version);
            debug!(
                ?obj_id,
                ?funds,
                ?accumulator_version,
                ?unsettled_withdraw,
                ?amount,
                "Trying to withdraw"
            );
            assert!(funds >= unsettled_withdraw);
            if funds - unsettled_withdraw < *amount {
                return false;
            }
        }
        self.unsettled
            .record_unsettled_withdraws(unsettled_withdraw_updates.iter(), accumulator_version);
        true
    }

    /// Advances the last-settled accumulator version, unblocking funds checks that were
    /// waiting for this version to settle. This runs when the barrier settle tx *executes*, which
    /// may be concurrent with other transactions in the same checkpoint — so it only advances the
    /// watch (safe: it just enables reads of the now-settled balance) and does not garbage-collect
    /// unsettled entries. GC happens later, at checkpoint commit (`commit_effects`), once every
    /// transaction that could still read those entries has executed.
    pub fn settle_accumulator_version(&self, next_accumulator_version: SequenceNumber) {
        // unwrap is safe because a receiver is always alive as part of self.
        self.last_settled_version_sender
            .send(next_accumulator_version)
            .unwrap();
        self.metrics
            .highest_settled_version
            .set(next_accumulator_version.value() as i64);
    }

    #[cfg(test)]
    pub fn get_current_accumulator_version(&self) -> SequenceNumber {
        *self.last_settled_version_receiver.borrow()
    }
}
