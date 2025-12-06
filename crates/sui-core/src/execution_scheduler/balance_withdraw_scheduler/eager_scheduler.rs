// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    sync::Arc,
};

use mysten_common::in_test_configuration;
use parking_lot::Mutex;
use sui_types::{
    accumulator_root::AccumulatorObjId, base_types::SequenceNumber, digests::TransactionDigest,
};
use tokio::sync::oneshot::Sender;
use tracing::{debug, instrument};

use crate::{
    accumulators::balance_read::AccountBalanceRead,
    execution_scheduler::balance_withdraw_scheduler::{
        BalanceSettlement, ScheduleResult, ScheduleStatus, TxBalanceWithdraw,
        scheduler::{BalanceWithdrawSchedulerTrait, WithdrawReservations},
    },
};

pub(crate) struct EagerBalanceWithdrawScheduler {
    balance_read: Arc<dyn AccountBalanceRead>,
    inner_state: Arc<Mutex<InnerState>>,
}

struct InnerState {
    /// For each address balance account that we have seen withdraws through `schedule_withdraws`,
    /// we track the current state of that account, and only remove it from the map after
    /// we have settled all withdraws for that account.
    tracked_accounts: HashMap<AccumulatorObjId, AccountState>,
    /// Tracks all the acddress balance accounts that have a withdraw transaction tracked,
    /// mapping from the accumulator version that the withdraw transaction reads from.
    /// If a withdraw transaction needs to withdraw from account O at version V,
    /// we must process and settle that withdraw transaction whenever we settle all transactions
    /// scheduled for version V.
    pending_settlements: HashMap<SequenceNumber, BTreeSet<AccumulatorObjId>>,
    /// The current version of the accumulator object.
    accumulator_version: SequenceNumber,
}

struct AccountState {
    account_id: AccumulatorObjId,
    /// The amount of balance that has been reserved for this account, for each accumulator version.
    /// This is tracked so that we could add them back to the account balance when we settle the withdraws.
    reserved_balance: HashMap<SequenceNumber, u128>,
    /// Withdraws that could not yet be scheduled due to insufficient balance, and
    /// hence have not reserved any balance yet. We track them so that we could schedule them
    /// anytime we may have sufficient balance.
    pending_reservations: VecDeque<Arc<PendingWithdraw>>,
    /// The lower bound of the current balance of this account.
    /// It is the amount of guaranteed balance that we could withdraw from this account at this point.
    /// This is maintained as the most recent settled balance, subtracted by the reserved balance.
    balance_lower_bound: u128,
}

struct PendingWithdraw {
    accumulator_version: SequenceNumber,
    tx_digest: TransactionDigest,
    sender: Mutex<Option<Sender<ScheduleResult>>>,
    pending: Mutex<BTreeMap<AccumulatorObjId, u64>>,
}

enum TryReserveResult {
    SufficientBalance,
    InsufficientBalance,
    Pending,
}

impl EagerBalanceWithdrawScheduler {
    pub fn new(
        balance_read: Arc<dyn AccountBalanceRead>,
        starting_accumulator_version: SequenceNumber,
    ) -> Arc<Self> {
        Arc::new(Self {
            balance_read,
            inner_state: Arc::new(Mutex::new(InnerState {
                tracked_accounts: HashMap::new(),
                pending_settlements: HashMap::new(),
                accumulator_version: starting_accumulator_version,
            })),
        })
    }
}

#[async_trait::async_trait]
impl BalanceWithdrawSchedulerTrait for EagerBalanceWithdrawScheduler {
    #[instrument(level = "debug", skip_all, fields(withdraw_accumulator_version = ?withdraws.accumulator_version.value()))]
    async fn schedule_withdraws(&self, withdraws: WithdrawReservations) {
        let mut inner_state = self.inner_state.lock();
        let cur_accumulator_version = inner_state.accumulator_version;
        if withdraws.accumulator_version < cur_accumulator_version {
            // This accumulator version is already settled.
            // There is no need to schedule the withdraws.
            withdraws.notify_skip_schedule();
            return;
        }

        for (withdraw, sender) in withdraws.withdraws.into_iter().zip(withdraws.senders) {
            let accounts = withdraw.reservations.keys().cloned().collect::<Vec<_>>();
            inner_state
                .pending_settlements
                .entry(withdraws.accumulator_version)
                .or_default()
                .extend(&accounts);
            let pending_withdraw =
                PendingWithdraw::new(withdraws.accumulator_version, withdraw, sender);
            for account_id in accounts {
                let account_state = inner_state
                    .tracked_accounts
                    .entry(account_id)
                    .or_insert_with(|| {
                        // TODO: This will be doing a DF read while holding the state lock.
                        // We may need to look at ways to make the DF reads non-blocking.
                        // We also need to get rid of the need to read old versions of the account balance.
                        AccountState::new(
                            self.balance_read.as_ref(),
                            account_id,
                            cur_accumulator_version,
                        )
                    });
                let has_blocking_reservations = !account_state.pending_reservations.is_empty();
                let result = account_state.try_reserve(
                    cur_accumulator_version,
                    &pending_withdraw,
                    has_blocking_reservations,
                );
                if matches!(result, TryReserveResult::Pending) {
                    account_state
                        .pending_reservations
                        .push_back(pending_withdraw.clone());
                }
            }
        }
    }

    async fn settle_balances(&self, settlement: BalanceSettlement) {
        let next_accumulator_version = settlement.next_accumulator_version;
        let mut inner_state = self.inner_state.lock();
        if next_accumulator_version <= inner_state.accumulator_version {
            // This accumulator version is already settled.
            // There is no need to settle the balances.
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
        inner_state.accumulator_version.increment();
        debug!(
            "Bumping accumulator version from {:?} to {:?}",
            cleanup_version.value(),
            next_accumulator_version.value(),
        );

        // A settlement bumps the accumulator version from `cleanup_version` to `last_settled_version`.
        // We must processing the following types of accounts:
        // 1. Accounts that had withdraws scheduled at version `cleanup_version`. These withdraws
        // are now settled and we know their exact balance changes.
        // 2. Accounts that had withdraws scheduled at version `last_settled_version`. These withdraws
        // if not yet scheduled, can now be deterministically scheduled, since
        // we have the final state before them.
        // 3. Accounts that have balance changes through settle_balances, this can include
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
        affected_accounts.extend(settlement.balance_changes.keys().cloned());

        debug!(
            "Processing withdraws affecting accounts: {:?}",
            affected_accounts,
        );
        for object_id in affected_accounts {
            let Some(account_state) = inner_state.tracked_accounts.get_mut(&object_id) else {
                continue;
            };
            debug!(
                account_id = ?object_id,
                "Settling account",
            );
            let reserved = account_state
                .reserved_balance
                .remove(&cleanup_version)
                .unwrap_or_default() as i128;
            let settled = settlement
                .balance_changes
                .get(&object_id)
                .copied()
                .unwrap_or_default();
            let net = u128::try_from(reserved.checked_add(settled).unwrap())
                .expect("Withdraw amounts must be bounded by reservations");
            account_state.balance_lower_bound += net;
            debug!(
                account_id = ?object_id,
                "Reserved balance: {:?}, settled balance: {:?}, new min guaranteed balance: {:?}",
                reserved, settled, account_state.balance_lower_bound,
            );

            while !account_state.pending_reservations.is_empty() {
                let pending_withdraw = account_state.pending_reservations.pop_front().unwrap();
                assert!(pending_withdraw.accumulator_version >= next_accumulator_version);

                let result =
                    account_state.try_reserve(next_accumulator_version, &pending_withdraw, false);
                if matches!(result, TryReserveResult::Pending) {
                    account_state
                        .pending_reservations
                        .push_front(pending_withdraw);
                    break;
                }
            }

            if in_test_configuration() {
                account_state.debug_check_account_state_invariants(
                    self.balance_read.as_ref(),
                    next_accumulator_version,
                );
            }

            if account_state.is_empty() {
                debug!(
                    account_id = ?object_id,
                    "Removing account state since it is empty",
                );
                inner_state.tracked_accounts.remove(&object_id);
            }
            // TODO: Debug invariant check on account state and accumulator version match.
        }
        debug!("Settled accumulator version {:?}", next_accumulator_version);
    }

    fn close_epoch(&self) {
        debug!("Closing epoch in EagerBalanceWithdrawScheduler",);
        let inner_state = self.inner_state.lock();
        assert!(inner_state.pending_settlements.is_empty());
        assert!(inner_state.tracked_accounts.is_empty());
    }

    #[cfg(test)]
    fn get_current_accumulator_version(&self) -> SequenceNumber {
        let inner_state = self.inner_state.lock();
        inner_state.accumulator_version
    }
}

impl AccountState {
    fn new(
        balance_read: &dyn AccountBalanceRead,
        account_id: AccumulatorObjId,
        last_settled_version: SequenceNumber,
    ) -> Self {
        let balance = balance_read.get_account_balance(&account_id, last_settled_version);
        debug!(
            last_settled_version =? last_settled_version.value(),
            account_id = ?account_id.inner(),
            "New account state tracked with initial balance {:?}",
            balance,
        );
        Self {
            account_id,
            reserved_balance: HashMap::new(),
            pending_reservations: VecDeque::new(),
            balance_lower_bound: balance,
        }
    }

    #[instrument(
        level = "debug",
        skip_all,
        fields(
            cur_accumulator_version =? cur_accumulator_version.value(),
            account_id = ?self.account_id.inner(),
            tx_digest = ?pending_withdraw.tx_digest,
            ?has_blocking_reservations
        )
    )]
    fn try_reserve(
        &mut self,
        cur_accumulator_version: SequenceNumber,
        pending_withdraw: &Arc<PendingWithdraw>,
        has_blocking_reservations: bool,
    ) -> TryReserveResult {
        let to_reserve = pending_withdraw.pending_amount(&self.account_id);
        debug!(
            "Trying to reserve {}, min_guaranteed_balance: {}, pending_reservations size: {}",
            to_reserve,
            self.balance_lower_bound,
            self.pending_reservations.len(),
        );
        let insufficient_balance = to_reserve > self.balance_lower_bound;
        if insufficient_balance || has_blocking_reservations {
            if cur_accumulator_version == pending_withdraw.accumulator_version {
                assert!(!has_blocking_reservations);
                pending_withdraw.notify_insufficient_balance();
                TryReserveResult::InsufficientBalance
            } else {
                debug!("Adding to pending reservations since we cannot schedule it yet.");
                TryReserveResult::Pending
            }
        } else {
            self.commit_reservation(pending_withdraw);
            TryReserveResult::SufficientBalance
        }
    }

    fn commit_reservation(&mut self, pending_withdraw: &Arc<PendingWithdraw>) {
        let mut pending = pending_withdraw.pending.lock();
        let to_reserve = pending.remove(&self.account_id).unwrap() as u128;
        assert!(self.balance_lower_bound >= to_reserve);
        self.balance_lower_bound -= to_reserve;
        *self
            .reserved_balance
            .entry(pending_withdraw.accumulator_version)
            .or_default() += to_reserve;
        debug!(
            "Successfully reserved {} for account. New min guaranteed balance: {}",
            to_reserve, self.balance_lower_bound
        );
        if pending.is_empty() {
            debug!("Successfully reserved all accounts for withdraw transaction");
            let sender = pending_withdraw.sender.lock().take().unwrap();
            let _ = sender.send(ScheduleResult {
                tx_digest: pending_withdraw.tx_digest,
                status: ScheduleStatus::SufficientBalance,
            });
        }
    }

    fn is_empty(&self) -> bool {
        self.reserved_balance.is_empty() && self.pending_reservations.is_empty()
    }

    fn debug_check_account_state_invariants(
        &self,
        balance_read: &dyn AccountBalanceRead,
        last_settled_version: SequenceNumber,
    ) {
        let total_reserved = self.reserved_balance.values().sum::<u128>();
        let expected_balance = self.balance_lower_bound + total_reserved;
        let actual_balance =
            balance_read.get_account_balance(&self.account_id, last_settled_version);
        assert_eq!(expected_balance, actual_balance);
    }
}

impl PendingWithdraw {
    fn new(
        accumulator_version: SequenceNumber,
        withdraw: TxBalanceWithdraw,
        sender: Sender<ScheduleResult>,
    ) -> Arc<Self> {
        Arc::new(Self {
            accumulator_version,
            tx_digest: withdraw.tx_digest,
            sender: Mutex::new(Some(sender)),
            pending: Mutex::new(withdraw.reservations),
        })
    }

    fn pending_amount(&self, account_id: &AccumulatorObjId) -> u128 {
        self.pending.lock().get(account_id).copied().unwrap() as u128
    }

    fn notify_insufficient_balance(&self) {
        let mut sender_guard = self.sender.lock();
        // sender may be None because this pending withdraw may have multiple
        // insufficient accounts, and when processing the first one, the sender
        // is already taken.
        if let Some(sender) = sender_guard.take() {
            debug!("Insufficient balance for withdraw");
            let _ = sender.send(ScheduleResult {
                tx_digest: self.tx_digest,
                status: ScheduleStatus::InsufficientBalance,
            });
        }
    }
}
