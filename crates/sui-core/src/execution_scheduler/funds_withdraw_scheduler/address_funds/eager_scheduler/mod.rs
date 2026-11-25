// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use parking_lot::Mutex;
use sui_types::{
    accumulator_root::AccumulatorObjId, base_types::SequenceNumber, digests::TransactionDigest,
};
use tokio::sync::oneshot;
use tracing::debug;

use crate::{
    accumulators::funds_read::AccountFundsRead,
    execution_scheduler::funds_withdraw_scheduler::{
        FundsSettlement, ScheduleStatus, WithdrawReservations,
        address_funds::{
            ScheduleResult,
            eager_scheduler::{account_state::AccountState, pending_withdraw::PendingWithdraw},
        },
        scheduler::FundsWithdrawSchedulerTrait,
    },
};

mod account_state;
mod pending_withdraw;

#[cfg(test)]
mod eager_scheduler_tests;

pub(crate) struct EagerFundsWithdrawScheduler {
    funds_read: Arc<dyn AccountFundsRead>,
    inner_state: Arc<Mutex<InnerState>>,
}

struct InnerState {
    /// For each address funds account that we have seen withdraws through `schedule_withdraws`,
    /// we track the current state of that account, and only remove it from the map after
    /// we have settled all withdraws for that account.
    tracked_accounts: BTreeMap<AccumulatorObjId, AccountState>,
    /// Tracks all the address funds accounts that have a withdraw transaction tracked,
    /// mapping from the accumulator version that the withdraw transaction reads from.
    /// If a withdraw transaction needs to withdraw from account O at version V,
    /// we must process and settle that withdraw transaction whenever we settle all transactions
    /// scheduled for version V.
    pending_settlements: BTreeMap<SequenceNumber, BTreeSet<AccumulatorObjId>>,
    /// The current version of the accumulator object known to the scheduler.
    accumulator_version: SequenceNumber,
}

impl EagerFundsWithdrawScheduler {
    pub fn new(
        funds_read: Arc<dyn AccountFundsRead>,
        starting_accumulator_version: SequenceNumber,
    ) -> Arc<Self> {
        Arc::new(Self {
            funds_read,
            inner_state: Arc::new(Mutex::new(InnerState {
                tracked_accounts: BTreeMap::new(),
                pending_settlements: BTreeMap::new(),
                accumulator_version: starting_accumulator_version,
            })),
        })
    }

    /// Reserves `reservations` against each account's tracked state, starting to track
    /// accounts from `init_balances`, and records the version for settlement.
    fn reserve_withdraws(
        inner_state: &mut InnerState,
        reservations: WithdrawReservations,
        init_balances: &BTreeMap<AccumulatorObjId, (u128, SequenceNumber)>,
        cur_accumulator_version: SequenceNumber,
        all_accounts: BTreeSet<AccumulatorObjId>,
    ) -> Vec<(Arc<PendingWithdraw>, oneshot::Receiver<ScheduleStatus>)> {
        let mut pending = Vec::with_capacity(reservations.withdraws.len());
        for withdraw in reservations.withdraws {
            let (sender, receiver) = oneshot::channel();
            let accounts = withdraw.reservations.keys().cloned().collect::<Vec<_>>();
            let pending_withdraw =
                PendingWithdraw::new(reservations.accumulator_version, withdraw, sender);
            for account_id in accounts {
                let entry = inner_state
                    .tracked_accounts
                    .entry(account_id)
                    .or_insert_with(|| {
                        let (balance, version) = init_balances.get(&account_id).cloned().unwrap();
                        AccountState::new(account_id, balance, version)
                    });
                entry.try_reserve_new_withdraw(pending_withdraw.clone(), cur_accumulator_version);
            }
            pending.push((pending_withdraw, receiver));
        }
        let old = inner_state
            .pending_settlements
            .insert(reservations.accumulator_version, all_accounts);
        assert!(old.is_none());
        pending
    }
}

#[async_trait::async_trait]
impl FundsWithdrawSchedulerTrait for EagerFundsWithdrawScheduler {
    fn schedule_withdraws(
        &self,
        mut reservations: WithdrawReservations,
    ) -> BTreeMap<TransactionDigest, ScheduleResult> {
        let mut inner_state = self.inner_state.lock();
        let cur_accumulator_version = inner_state.accumulator_version;
        if reservations.accumulator_version < cur_accumulator_version {
            // This accumulator version is already settled.
            // There is no need to schedule the withdraws.
            return reservations.notify_skip_schedule();
        }
        let mut all_accounts = reservations.all_accounts();
        let mut init_balances = BTreeMap::new();
        let mut settled_in_storage = false;
        for account_id in &all_accounts {
            if inner_state.tracked_accounts.contains_key(account_id) {
                continue;
            }
            // TODO: We can warm up the cache prior to holding the lock.
            let (balance, version) = self
                .funds_read
                .get_consistent_latest_account_amount_and_version(account_id);
            if version > reservations.accumulator_version {
                settled_in_storage = true;
                break;
            }
            init_balances.insert(*account_id, (balance, version));
        }

        if settled_in_storage {
            let skip_results = reservations.notify_skip_schedule();
            // Storage has settled this batch, but this scheduler's `settle_funds` for it is
            // still to come, so tracked accounts must hold the batch's reservations until then.
            // Untracked accounts stay untracked; a later read of them includes this batch.
            let tracked = &inner_state.tracked_accounts;
            reservations.withdraws.retain_mut(|withdraw| {
                withdraw
                    .reservations
                    .retain(|account_id, _| tracked.contains_key(account_id));
                !withdraw.reservations.is_empty()
            });
            all_accounts.retain(|account_id| tracked.contains_key(account_id));
            if !all_accounts.is_empty() {
                Self::reserve_withdraws(
                    &mut inner_state,
                    reservations,
                    &BTreeMap::new(),
                    cur_accumulator_version,
                    all_accounts,
                );
            }
            return skip_results;
        }

        Self::reserve_withdraws(
            &mut inner_state,
            reservations,
            &init_balances,
            cur_accumulator_version,
            all_accounts,
        )
        .into_iter()
        .map(|(pending_withdraw, mut receiver)| {
            let result = if pending_withdraw.is_scheduled() {
                ScheduleResult::ScheduleResult(receiver.try_recv().unwrap())
            } else {
                ScheduleResult::Pending(receiver)
            };
            (pending_withdraw.tx_digest(), result)
        })
        .collect()
    }

    fn settle_funds(&self, settlement: FundsSettlement) {
        let next_accumulator_version = settlement.next_accumulator_version;
        let mut inner_state = self.inner_state.lock();
        if next_accumulator_version <= inner_state.accumulator_version {
            // This accumulator version is already settled.
            // There is no need to settle the funds.
            debug!(
                next_accumulator_version =? next_accumulator_version.value(),
                "Skipping settlement since it is already settled",
            );
            return;
        }
        assert_eq!(
            next_accumulator_version,
            inner_state.accumulator_version.next()
        );
        let cleanup_version = inner_state.accumulator_version;
        inner_state.accumulator_version = next_accumulator_version;
        debug!(
            "Bumping accumulator version from {:?} to {:?}",
            cleanup_version.value(),
            next_accumulator_version.value(),
        );

        // A settlement bumps the accumulator version from `cleanup_version` to `next_accumulator_version`.
        // We must process the following types of accounts:
        // 1. Accounts that had withdraws scheduled at version `cleanup_version`. These withdraws
        // are now settled and we know their exact funds changes.
        // 2. Accounts that had withdraws scheduled at version `next_accumulator_version`. These withdraws
        // if not yet scheduled, can now be deterministically scheduled, since
        // we have the final state before them.
        // 3. Accounts that have funds changes through settle_funds, this can include
        // both withdraws and deposits.
        let mut affected_accounts = inner_state
            .pending_settlements
            .remove(&cleanup_version)
            .unwrap_or_default();
        // Since we just bumped the accumulator version, any withdraws scheduled that depend
        // on this version can now be deterministically scheduled.
        if let Some(current_version_accounts) = inner_state
            .pending_settlements
            .get(&next_accumulator_version)
        {
            affected_accounts.extend(current_version_accounts);
        }
        affected_accounts.extend(settlement.funds_changes.keys().cloned());

        for object_id in affected_accounts {
            let Some(account_state) = inner_state.tracked_accounts.get_mut(&object_id) else {
                continue;
            };
            account_state.settle_funds(
                settlement
                    .funds_changes
                    .get(&object_id)
                    .cloned()
                    .unwrap_or_default(),
                next_accumulator_version,
            );

            if account_state.is_empty() {
                debug!(
                    account_id = ?object_id,
                    "Removing account state since it is empty",
                );
                inner_state.tracked_accounts.remove(&object_id);
            }
        }
        debug!("Settled accumulator version {:?}", next_accumulator_version);
    }

    fn close_epoch(&self) {
        debug!("Closing epoch in EagerFundsWithdrawScheduler");
    }

    fn num_tracked_accounts(&self) -> usize {
        self.inner_state.lock().tracked_accounts.len()
    }

    #[cfg(test)]
    fn get_current_accumulator_version(&self) -> SequenceNumber {
        self.inner_state.lock().accumulator_version
    }
}
