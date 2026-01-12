// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use parking_lot::Mutex;
use sui_types::{accumulator_root::AccumulatorObjId, base_types::SequenceNumber};
use tracing::debug;

use crate::{
    accumulators::funds_read::AccountFundsRead,
    execution_scheduler::funds_withdraw_scheduler::{
        FundsSettlement,
        address_funds::eager_scheduler::{
            account_state::AccountState, pending_withdraw::PendingWithdraw,
        },
        scheduler::{FundsWithdrawSchedulerTrait, WithdrawReservations},
    },
};

mod account_state;
mod pending_withdraw;

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
}

#[async_trait::async_trait]
impl FundsWithdrawSchedulerTrait for EagerFundsWithdrawScheduler {
    async fn schedule_withdraws(&self, withdraws: WithdrawReservations) {
        let mut inner_state = self.inner_state.lock();
        if withdraws.accumulator_version < inner_state.accumulator_version {
            // This accumulator version is already settled.
            // There is no need to schedule the withdraws.
            withdraws.notify_skip_schedule();
            return;
        }
        let all_accounts = withdraws.all_accounts();
        let untracked_accounts = all_accounts
            .iter()
            .filter(|account_id| !inner_state.tracked_accounts.contains_key(account_id));
        let mut init_balances = BTreeMap::new();
        for account_id in untracked_accounts {
            // TODO: We can warm up the cache prior to holding the lock.
            let (balance, version) = self.funds_read.get_latest_account_amount(account_id);
            if version > withdraws.accumulator_version {
                withdraws.notify_skip_schedule();
                return;
            }
            init_balances.insert(account_id, (balance, version));
        }
        let cur_accumulator_version = inner_state.accumulator_version;
        for (withdraw, sender) in withdraws.withdraws.into_iter().zip(withdraws.senders) {
            let accounts = withdraw.reservations.keys().cloned().collect::<Vec<_>>();
            let pending_withdraw =
                PendingWithdraw::new(withdraws.accumulator_version, withdraw, sender);
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
        }
        let old = inner_state
            .pending_settlements
            .insert(withdraws.accumulator_version, all_accounts);
        assert!(old.is_none());
    }

    async fn settle_funds(&self, settlement: FundsSettlement) {
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

    #[cfg(test)]
    fn get_current_accumulator_version(&self) -> SequenceNumber {
        self.inner_state.lock().accumulator_version
    }
}
