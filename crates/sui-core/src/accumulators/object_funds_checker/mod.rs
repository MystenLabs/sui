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
    effects::{AccumulatorOperation, AccumulatorValue, TransactionEffects, TransactionEffectsAPI},
    executable_transaction::VerifiedExecutableTransaction,
    execution_params::FundsWithdrawStatus,
    transaction::TransactionDataAPI,
};
use tokio::{
    sync::{oneshot, watch},
    time::Instant,
};
use tracing::{debug, instrument};

use crate::{
    accumulators::funds_read::AccountFundsRead,
    authority::{ExecutionEnv, authority_per_epoch_store::AuthorityPerEpochStore},
    execution_scheduler::ExecutionScheduler,
};

#[cfg(test)]
mod integration_tests;
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

pub struct ObjectFundsChecker {
    /// Watchers to keep track the last settled accumulator version.
    /// This is updated whenever the settlement barrier transaction is executed.
    last_settled_version_sender: watch::Sender<SequenceNumber>,
    last_settled_version_receiver: watch::Receiver<SequenceNumber>,
    inner: RwLock<Inner>,
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

impl ObjectFundsChecker {
    pub fn new(starting_accumulator_version: SequenceNumber) -> Self {
        let (last_settled_version_sender, last_settled_version_receiver) =
            watch::channel(starting_accumulator_version);
        Self {
            last_settled_version_sender,
            last_settled_version_receiver,
            inner: RwLock::new(Inner::default()),
        }
    }

    #[instrument(level = "debug", skip_all, fields(tx_digest = ?certificate.digest()))]
    pub fn should_commit_object_funds_withdraws(
        &self,
        certificate: &VerifiedExecutableTransaction,
        effects: &TransactionEffects,
        execution_env: &ExecutionEnv,
        funds_read: &Arc<dyn AccountFundsRead>,
        execution_scheduler: &Arc<ExecutionScheduler>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> bool {
        if effects.status().is_err() {
            // This transaction already failed. It does not matter any more
            // whether it has sufficient object funds or not.
            debug!("Transaction failed, committing effects");
            return true;
        }
        let address_funds_reservations: BTreeSet<_> = certificate
            .transaction_data()
            .process_funds_withdrawals_for_execution(epoch_store.get_chain_identifier())
            .into_keys()
            .collect();
        // All withdraws will show up as accumulator events with integer values.
        // Among them, addresses that do not have funds reservations are object
        // withdraws.
        let object_withdraws: BTreeMap<_, _> = effects
            .accumulator_events()
            .into_iter()
            .filter_map(|event| {
                if address_funds_reservations.contains(&event.accumulator_obj) {
                    return None;
                }
                // Only integer splits are funds withdraws.
                if let (AccumulatorOperation::Split, AccumulatorValue::Integer(amount)) =
                    (event.write.operation, event.write.value)
                {
                    Some((event.accumulator_obj, amount))
                } else {
                    None
                }
            })
            .collect();
        // If there are no object withdraws, we can skip checking object funds.
        if object_withdraws.is_empty() {
            debug!("No object withdraws, committing effects");
            return true;
        }
        let Some(accumulator_version) = execution_env.assigned_versions.accumulator_version else {
            // Fastpath transactions that perform object funds withdraws
            // must wait for consensus to assign the accumulator version.
            // We cannot optimize the scheduling by processing fastpath object withdraws
            // sooner because these may get reverted, and we don't want them
            // pollute the scheduler tracking state.
            // TODO: We could however optimize execution by caching
            // the execution state to avoid re-execution.
            return false;
        };
        match self.check_object_funds(object_withdraws, accumulator_version, funds_read.as_ref()) {
            // Sufficient funds, we can go ahead and commit the execution results as it is.
            ObjectFundsWithdrawStatus::SufficientFunds => {
                debug!("Object funds sufficient, committing effects");
                true
            }
            // Currently insufficient funds. We need to wait until it reach a deterministic state
            // before we can determine if it is really insufficient (to include potential deposits)
            // At that time we will have to re-enqueue the transaction for execution again.
            // Re-enqueue is handled here so the caller does not need to worry about it.
            ObjectFundsWithdrawStatus::Pending(receiver) => {
                let scheduler = execution_scheduler.clone();
                let cert = certificate.clone();
                let mut execution_env = execution_env.clone();
                let epoch_store = epoch_store.clone();
                tokio::task::spawn(async move {
                    // It is possible that checkpoint executor finished executing
                    // the current epoch and went ahead with epoch change asynchronously,
                    // while this is still waiting.
                    let _ = epoch_store
                        .within_alive_epoch(async move {
                            let tx_digest = cert.digest();
                            match receiver.await {
                                Ok(FundsWithdrawStatus::MaybeSufficient) => {
                                    // The withdraw state is now deterministically known,
                                    // so we can enqueue the transaction again and it will check again
                                    // whether it is sufficient or not in the next execution.
                                    // TODO: We should be able to optimize this by avoiding re-execution.
                                    debug!(?tx_digest, "Object funds possibly sufficient");
                                }
                                Ok(FundsWithdrawStatus::Insufficient) => {
                                    // Re-enqueue with insufficient funds status, so it will be executed
                                    // in the next execution and fail through early error.
                                    // FIXME: We need to also track the amount of gas that was used,
                                    // so that we could charge properly in the next execution when we
                                    // go through early error. Otherwise we would undercharge.
                                    execution_env = execution_env.with_insufficient_funds();
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
                });
                false
            }
        }
    }

    fn check_object_funds(
        &self,
        object_withdraws: BTreeMap<AccumulatorObjId, u64>,
        accumulator_version: SequenceNumber,
        funds_read: &dyn AccountFundsRead,
    ) -> ObjectFundsWithdrawStatus {
        let last_settled_version = *self.last_settled_version_receiver.borrow();
        if accumulator_version <= last_settled_version {
            // If the version we are withdrawing from is already settled, we have all the information
            // we need to determine if the funds are sufficient or not.
            if self.try_withdraw(funds_read, &object_withdraws, accumulator_version) {
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

    fn try_withdraw(
        &self,
        funds_read: &dyn AccountFundsRead,
        object_withdraws: &BTreeMap<AccumulatorObjId, u64>,
        accumulator_version: SequenceNumber,
    ) -> bool {
        for (obj_id, amount) in object_withdraws {
            let funds = funds_read.get_account_amount_at_version(obj_id, accumulator_version);
            // Reading inner without a top-level lock is safe because no two transactions can be withdrawing
            // from the same account at the same time.
            let unsettled_withdraw = self
                .inner
                .read()
                .unsettled_withdraws
                .get(obj_id)
                .and_then(|withdraws| withdraws.get(&accumulator_version))
                .copied()
                .unwrap_or_default();
            debug!(
                ?obj_id,
                ?funds,
                ?accumulator_version,
                ?unsettled_withdraw,
                ?amount,
                "Trying to withdraw"
            );
            assert!(funds >= unsettled_withdraw);
            if funds - unsettled_withdraw < *amount as u128 {
                return false;
            }
        }
        let mut inner = self.inner.write();
        for (obj_id, amount) in object_withdraws {
            let entry = inner
                .unsettled_withdraws
                .entry(*obj_id)
                .or_default()
                .entry(accumulator_version)
                .or_default();
            debug!(?obj_id, ?amount, ?entry, "Updating unsettled withdraws");
            *entry = entry.checked_add(*amount as u128).unwrap();

            inner
                .unsettled_accounts
                .entry(accumulator_version)
                .or_default()
                .insert(*obj_id);
        }
        true
    }

    pub fn settle_accumulator_version(&self, next_accumulator_version: SequenceNumber) {
        // unwrap is safe because a receiver is always alive as part of self.
        self.last_settled_version_sender
            .send(next_accumulator_version)
            .unwrap();
    }

    pub fn commit_effects<'a>(
        &self,
        committed_effects: impl Iterator<Item = &'a TransactionEffects>,
    ) {
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

    fn commit_accumulator_versions(&self, committed_accumulator_versions: Vec<SequenceNumber>) {
        let mut inner = self.inner.write();
        for accumulator_version in committed_accumulator_versions {
            let accounts = inner
                .unsettled_accounts
                .remove(&accumulator_version)
                .unwrap_or_default();
            for account in accounts {
                let withdraws = inner.unsettled_withdraws.get_mut(&account);
                if let Some(withdraws) = withdraws {
                    withdraws.remove(&accumulator_version);
                    if withdraws.is_empty() {
                        inner.unsettled_withdraws.remove(&account);
                    }
                }
            }
        }
    }

    #[cfg(test)]
    pub fn get_current_accumulator_version(&self) -> SequenceNumber {
        *self.last_settled_version_receiver.borrow()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        let inner = self.inner.read();
        inner.unsettled_withdraws.is_empty() && inner.unsettled_accounts.is_empty()
    }
}
