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

use super::{
    FundsSettlement, ScheduleResult, ScheduleStatus, TxFundsWithdraw,
    scheduler::{FundsWithdrawSchedulerTrait, WithdrawReservations},
};
use crate::accumulators::funds_read::AccountFundsRead;

pub(crate) struct EagerFundsWithdrawScheduler {
    funds_read: Arc<dyn AccountFundsRead>,
    inner_state: Arc<Mutex<InnerState>>,
}

struct InnerState {
    /// For each address funds account that we have seen withdraws through `schedule_withdraws`,
    /// we track the current state of that account, and only remove it from the map after
    /// we have settled all withdraws for that account.
    tracked_accounts: HashMap<AccumulatorObjId, AccountState>,
    /// Tracks all the address funds accounts that have a withdraw transaction tracked,
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
    /// The amount of funds that has been reserved for this account, for each accumulator version.
    /// This is tracked so that we could add them back to the account funds when we settle the withdraws.
    reserved_funds: HashMap<SequenceNumber, u128>,
    /// Withdraws that could not yet be scheduled due to insufficient funds, and
    /// hence have not reserved any funds yet. We track them so that we could schedule them
    /// anytime we may have sufficient funds.
    pending_reservations: VecDeque<Arc<PendingWithdraw>>,
    /// The lower bound of the current funds of this account.
    /// It is the amount of guaranteed funds that we could withdraw from this account at this point.
    /// This is maintained as the most recent settled funds, subtracted by the reserved funds.
    funds_lower_bound: u128,
}

struct PendingWithdraw {
    accumulator_version: SequenceNumber,
    tx_digest: TransactionDigest,
    sender: Mutex<Option<Sender<ScheduleResult>>>,
    pending: Mutex<BTreeMap<AccumulatorObjId, u64>>,
}

enum TryReserveResult {
    SufficientFunds,
    InsufficientFunds,
    Pending,
}

impl EagerFundsWithdrawScheduler {
    pub fn new(
        funds_read: Arc<dyn AccountFundsRead>,
        starting_accumulator_version: SequenceNumber,
    ) -> Arc<Self> {
        Arc::new(Self {
            funds_read,
            inner_state: Arc::new(Mutex::new(InnerState {
                tracked_accounts: HashMap::new(),
                pending_settlements: HashMap::new(),
                accumulator_version: starting_accumulator_version,
            })),
        })
    }
}

#[async_trait::async_trait]
impl FundsWithdrawSchedulerTrait for EagerFundsWithdrawScheduler {
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
                        // We also need to get rid of the need to read old versions of the account funds.
                        AccountState::new(
                            self.funds_read.as_ref(),
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
        inner_state.accumulator_version.increment();
        debug!(
            "Bumping accumulator version from {:?} to {:?}",
            cleanup_version.value(),
            next_accumulator_version.value(),
        );

        // A settlement bumps the accumulator version from `cleanup_version` to `last_settled_version`.
        // We must processing the following types of accounts:
        // 1. Accounts that had withdraws scheduled at version `cleanup_version`. These withdraws
        // are now settled and we know their exact funds changes.
        // 2. Accounts that had withdraws scheduled at version `last_settled_version`. These withdraws
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
                .reserved_funds
                .remove(&cleanup_version)
                .unwrap_or_default() as i128;
            let settled = settlement
                .funds_changes
                .get(&object_id)
                .copied()
                .unwrap_or_default();
            let net = u128::try_from(reserved.checked_add(settled).unwrap())
                .expect("Withdraw amounts must be bounded by reservations");
            account_state.funds_lower_bound += net;
            debug!(
                account_id = ?object_id,
                "Reserved funds: {:?}, settled funds: {:?}, new min guaranteed funds: {:?}",
                reserved, settled, account_state.funds_lower_bound,
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
                    self.funds_read.as_ref(),
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
        debug!("Closing epoch in EagerFundsWithdrawScheduler",);
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
        funds_read: &dyn AccountFundsRead,
        account_id: AccumulatorObjId,
        last_settled_version: SequenceNumber,
    ) -> Self {
        let funds = funds_read.get_account_amount(&account_id, last_settled_version);
        debug!(
            last_settled_version =? last_settled_version.value(),
            account_id = ?account_id.inner(),
            "New account state tracked with initial funds {:?}",
            funds,
        );
        Self {
            account_id,
            reserved_funds: HashMap::new(),
            pending_reservations: VecDeque::new(),
            funds_lower_bound: funds,
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
            "Trying to reserve {}, min_guaranteed_funds: {}, pending_reservations size: {}",
            to_reserve,
            self.funds_lower_bound,
            self.pending_reservations.len(),
        );
        let insufficient_funds = to_reserve > self.funds_lower_bound;
        if insufficient_funds || has_blocking_reservations {
            if cur_accumulator_version == pending_withdraw.accumulator_version {
                assert!(!has_blocking_reservations);
                pending_withdraw.notify_insufficient_funds();
                TryReserveResult::InsufficientFunds
            } else {
                debug!("Adding to pending reservations since we cannot schedule it yet.");
                TryReserveResult::Pending
            }
        } else {
            self.commit_reservation(pending_withdraw);
            TryReserveResult::SufficientFunds
        }
    }

    fn commit_reservation(&mut self, pending_withdraw: &Arc<PendingWithdraw>) {
        let mut pending = pending_withdraw.pending.lock();
        let to_reserve = pending.remove(&self.account_id).unwrap() as u128;
        assert!(self.funds_lower_bound >= to_reserve);
        self.funds_lower_bound -= to_reserve;
        *self
            .reserved_funds
            .entry(pending_withdraw.accumulator_version)
            .or_default() += to_reserve;
        debug!(
            "Successfully reserved {} for account. New min guaranteed funds: {}",
            to_reserve, self.funds_lower_bound
        );
        if pending.is_empty() {
            debug!("Successfully reserved all accounts for withdraw transaction");
            let sender = pending_withdraw.sender.lock().take().unwrap();
            let _ = sender.send(ScheduleResult {
                tx_digest: pending_withdraw.tx_digest,
                status: ScheduleStatus::SufficientFunds,
            });
        }
    }

    fn is_empty(&self) -> bool {
        self.reserved_funds.is_empty() && self.pending_reservations.is_empty()
    }

    fn debug_check_account_state_invariants(
        &self,
        funds_read: &dyn AccountFundsRead,
        last_settled_version: SequenceNumber,
    ) {
        let total_reserved = self.reserved_funds.values().sum::<u128>();
        let expected_funds = self.funds_lower_bound + total_reserved;
        let actual_funds = funds_read.get_account_amount(&self.account_id, last_settled_version);
        assert_eq!(expected_funds, actual_funds);
    }
}

impl PendingWithdraw {
    fn new(
        accumulator_version: SequenceNumber,
        withdraw: TxFundsWithdraw,
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

    fn notify_insufficient_funds(&self) {
        let mut sender_guard = self.sender.lock();
        // sender may be None because this pending withdraw may have multiple
        // insufficient accounts, and when processing the first one, the sender
        // is already taken.
        if let Some(sender) = sender_guard.take() {
            debug!("Insufficient funds for withdraw");
            let _ = sender.send(ScheduleResult {
                tx_digest: self.tx_digest,
                status: ScheduleStatus::InsufficientFunds,
            });
        }
    }
}
