// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    sync::Arc,
};

use parking_lot::Mutex;
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
};
use tokio::sync::oneshot::Sender;
use tracing::debug;

use crate::execution_scheduler::balance_withdraw_scheduler::{
    balance_read::AccountBalanceRead,
    scheduler::{BalanceWithdrawSchedulerTrait, WithdrawReservations},
    BalanceSettlement, ScheduleResult, ScheduleStatus, TxBalanceWithdraw,
};

pub(crate) struct EagerBalanceWithdrawScheduler {
    balance_read: Arc<dyn AccountBalanceRead>,
    inner_state: Arc<Mutex<InnerState>>,
}

struct InnerState {
    /// For each address balance account that we have seen withdraws through `schedule_withdraws`,
    /// we track the current state of that account, and only remove it from the map after
    /// we have settled all withdraws for that account.
    tracked_accounts: HashMap<ObjectID, AccountState>,
    /// Tracks all the acddress balance accounts that have a withdraw transaction tracked,
    /// mapping from the accumulator version that the withdraw transaction reads from.
    /// If a withdraw transaction needs to withdraw from account O at version V,
    /// we must process and settle that withdraw transaction whenever we settle all transactions
    /// scheduled for version V.
    pending_settlements: HashMap<SequenceNumber, BTreeSet<ObjectID>>,
    /// The last version that we have settled, i.e. the accumulator object becomes this version.
    /// All withdraw transactions scheduled prior to this version have been processed.
    last_settled_version: SequenceNumber,
}

struct AccountState {
    object_id: ObjectID,
    /// The amount of balance that has been reserved for this account, for each accumulator version.
    /// This is tracked so that we could add them back to the account balance when we settle the withdraws.
    reserved_balance: HashMap<SequenceNumber, u128>,
    /// Withdraws that could not yet be scheduled due to insufficient balance, and
    /// hence have not reserved any balance yet. We track them so that we could schedule them
    /// anytime we may have sufficient balance.
    pending_reservations: VecDeque<Arc<PendingWithdraw>>,
    /// The minimum guaranteed balance that we could withdraw from this account.
    /// This is maintained as the most recent settled balance, subtracted by the reserved balance.
    min_guaranteed_balance: u128,
}

struct PendingWithdraw {
    accumulator_version: SequenceNumber,
    tx_digest: TransactionDigest,
    sender: Mutex<Option<Sender<ScheduleResult>>>,
    pending: Mutex<BTreeMap<ObjectID, u64>>,
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
                last_settled_version: starting_accumulator_version,
            })),
        })
    }
}

// TODO: Check that it must be empty at end of epoch.
#[async_trait::async_trait]
impl BalanceWithdrawSchedulerTrait for EagerBalanceWithdrawScheduler {
    async fn schedule_withdraws(&self, withdraws: WithdrawReservations) {
        let mut inner_state = self.inner_state.lock();
        let last_settled_version = inner_state.last_settled_version;
        if withdraws.accumulator_version < last_settled_version {
            debug!(
                "Accumulator version {:?} is already settled",
                withdraws.accumulator_version
            );
            for (withdraw, sender) in withdraws.withdraws.into_iter().zip(withdraws.senders) {
                let _ = sender.send(ScheduleResult {
                    tx_digest: withdraw.tx_digest,
                    status: ScheduleStatus::AlreadyExecuted,
                });
            }
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
            for object_id in accounts {
                let account_state = inner_state
                    .tracked_accounts
                    .entry(object_id)
                    .or_insert_with(|| {
                        // TODO: This will be doing a DF read while holding the state lock.
                        // We may need to look at ways to make the DF reads non-blocking.
                        AccountState::new(
                            self.balance_read.as_ref(),
                            object_id,
                            last_settled_version,
                        )
                    });
                let success = account_state.try_reserve(last_settled_version, &pending_withdraw);
                debug!(
                    tx_digest = ?pending_withdraw.tx_digest,
                    "Reserving for account {:?} success: {:?}",
                    object_id, success
                );
            }
        }
    }

    async fn settle_balances(&self, settlement: BalanceSettlement) {
        debug!(
            balance_changes = ?settlement.balance_changes,
            "Settling balances",
        );
        let mut inner_state = self.inner_state.lock();
        let cleanup_version = inner_state.last_settled_version;
        inner_state.last_settled_version.increment();
        let last_settled_version = inner_state.last_settled_version;
        debug!(
            "Bumping last settled version from {:?} to {:?}",
            cleanup_version, last_settled_version,
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
        // on this version can now be deterministically scheduled, if not yet.
        if let Some(current_version_accounts) =
            inner_state.pending_settlements.get(&last_settled_version)
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
            // Withdraw amounts must be bounded by reservations.
            let net = u128::try_from(reserved + settled).unwrap();
            account_state.min_guaranteed_balance += net;
            debug!(
                account_id = ?object_id,
                "Reserved balance: {:?}, settled balance: {:?}, new min guaranteed balance: {:?}",
                reserved, settled, account_state.min_guaranteed_balance,
            );
            while !account_state.pending_reservations.is_empty() {
                let pending_withdraw = account_state.pending_reservations.pop_front().unwrap();
                let pending_amount = pending_withdraw.pending_amount(&object_id);
                debug!(
                    tx_digest = ?pending_withdraw.tx_digest,
                    account_id = ?object_id,
                    "Processing pending withdraw with amount {}", pending_amount,
                );
                assert!(pending_withdraw.accumulator_version >= last_settled_version);
                if account_state.min_guaranteed_balance >= pending_amount {
                    account_state.commit_reservation(&pending_withdraw);
                } else if pending_withdraw.accumulator_version == last_settled_version {
                    // If we have just settled this version, we can deterministically tell
                    // this account does not have enough balance.
                    pending_withdraw.notify_insufficient_balance();
                } else {
                    debug!(
                        tx_digest = ?pending_withdraw.tx_digest,
                        "Adding back to pending reservations since we cannot schedule it yet",
                    );
                    account_state
                        .pending_reservations
                        .push_front(pending_withdraw);
                    break;
                }
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
        debug!("Settled accumulator version {:?}", last_settled_version);
    }
}

impl AccountState {
    fn new(
        balance_read: &dyn AccountBalanceRead,
        object_id: ObjectID,
        last_settled_version: SequenceNumber,
    ) -> Self {
        let balance = balance_read.get_account_balance(&object_id, last_settled_version);
        debug!(
            last_settled_version =? last_settled_version.value(),
            account_id = ?object_id,
            "New account state tracked with initial balance {:?}",
            balance,
        );
        Self {
            object_id,
            reserved_balance: HashMap::new(),
            pending_reservations: VecDeque::new(),
            min_guaranteed_balance: balance,
        }
    }

    fn try_reserve(
        &mut self,
        last_settled_version: SequenceNumber,
        pending_withdraw: &Arc<PendingWithdraw>,
    ) -> bool {
        let to_reserve = pending_withdraw.pending_amount(&self.object_id);
        debug!(
            last_settled_version =? last_settled_version.value(),
            withdraw_version =? pending_withdraw.accumulator_version.value(),
            account_id = ?self.object_id,
            "Trying to reserve {}, min_guaranteed_balance: {}, pending_reservations size: {}", to_reserve, self.min_guaranteed_balance, self.pending_reservations.len(),
        );
        if !self.pending_reservations.is_empty() || to_reserve > self.min_guaranteed_balance {
            if last_settled_version == pending_withdraw.accumulator_version {
                assert!(self.pending_reservations.is_empty());
                pending_withdraw.notify_insufficient_balance();
            } else {
                debug!(
                    tx_digest = ?pending_withdraw.tx_digest,
                    account_id = ?self.object_id,
                    "Adding to pending reservations since we cannot schedule it yet.",
                );
                self.pending_reservations
                    .push_back(pending_withdraw.clone());
            }
            false
        } else {
            self.commit_reservation(pending_withdraw);
            true
        }
    }

    fn commit_reservation(&mut self, pending_withdraw: &Arc<PendingWithdraw>) {
        let mut pending = pending_withdraw.pending.lock();
        let to_reserve = pending.remove(&self.object_id).unwrap() as u128;
        assert!(self.min_guaranteed_balance >= to_reserve);
        self.min_guaranteed_balance -= to_reserve;
        self.reserved_balance
            .entry(pending_withdraw.accumulator_version)
            .and_modify(|v| *v += to_reserve)
            .or_insert(to_reserve);
        debug!(
            account_id = ?self.object_id,
            withdraw_version =? pending_withdraw.accumulator_version.value(),
            tx_digest = ?pending_withdraw.tx_digest,
            "Successfully reserved {} for account. New min guaranteed balance: {}",
            to_reserve,
            self.min_guaranteed_balance
        );
        if pending.is_empty() {
            debug!(
                tx_digest = ?pending_withdraw.tx_digest,
                "Successfully reserved all accounts for withdraw transaction",
            );
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

    fn pending_amount(&self, object_id: &ObjectID) -> u128 {
        self.pending.lock().get(object_id).copied().unwrap() as u128
    }

    fn notify_insufficient_balance(&self) {
        let mut sender_guard = self.sender.lock();
        // sender may be None because this pending withdraw may have multiple
        // insufficient accounts, and when processing the first one, the sender
        // is already taken.
        if let Some(sender) = sender_guard.take() {
            debug!(
                tx_digest = ?self.tx_digest,
                "Insufficient balance for accounts {:?}",
                self.pending.lock().keys().collect::<Vec<_>>()
            );
            let _ = sender.send(ScheduleResult {
                tx_digest: self.tx_digest,
                status: ScheduleStatus::InsufficientBalance,
            });
        }
    }
}
