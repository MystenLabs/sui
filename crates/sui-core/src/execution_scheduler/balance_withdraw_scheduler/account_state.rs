// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use tokio::sync::{watch, Notify};

use sui_types::base_types::{ObjectID, SequenceNumber};

use crate::execution_scheduler::balance_withdraw_scheduler::{
    balance_read::AccountBalanceRead, ScheduleResult,
};

/// Represents the state of an account that we are currently tracking in memory.
/// We start tracking an account state whenever we see a transaction that has withdraw reservations on this account,
/// and the accumulator version that transaction depends on has not been settled yet.
/// We stop tracking an account state when the last transaction that depends on this account is settled.
///
/// The most critical piece of information we maintain in this account state is the guaranteed minimum balance of the account.
/// This is the minimum balance that the account may have considering all the pending reservations.
/// It is calculated by taking the last known settled balance, and subtracting all the reserved amounts.
/// We use this to optimistically schedule withdraw reservations as soon as possible.
///
/// When a new transaction is added to the account state, it is first put into a queue, and its withdraw
/// reservations are not processed yet.
/// Whenever the above happens, or a new accumulator version is settled,
/// we will try to schedule as many reservations as possible.
///
/// When we schedule a reservation, we will update the guaranteed minimum balance,
/// and we will remove the reservation from the queue and add it to the reserved map.
///
/// When we settle an accumulator version, we will clear all the reservations that are older than the settled version,
/// since they are now all settled.
#[allow(dead_code)]
pub(crate) struct AccountState {
    account_id: ObjectID,
    back_pointer: Arc<DashMap<ObjectID, Arc<AccountState>>>,
    updated_notify: Arc<Notify>,
    inner: Arc<Mutex<AccountStateInner>>,
}

struct AccountStateInner {
    /// Transactions that contain withdraw reservations in this account.
    /// We have not yet processed these withdraw reservations because we cannot
    /// yet guarantee that there is enough balance for them, and we have
    /// not yet settled the dependent accumulator version.
    /// The key is the accumulator version at which the transaction was scheduled on.
    enqueued: BTreeMap<SequenceNumber, VecDeque<Arc<TxWithdrawRegistration>>>,
    /// Map from each accumulator version to the total amount of withdraws that
    /// have been currently reserved at that accumulator version.
    /// Each entry will be removed when the accumulator version is settled,
    /// and the amount will be added back to the last_known_min_balance.
    reserved: BTreeMap<SequenceNumber, u64>,
    /// The guaranteed minimum balance of the account based on the most recent settlement,
    /// as well as all reserved withdraws.
    /// We use i128 because it is possible for this value to become negative. This can happen
    /// when we have just settled accumulator version V, and hence scheduled all withdraws
    /// that depends on V. The current settled balance at V may not be enough to satisfy
    /// all the withdraws that were scheduled at V, and hence the guaranteed minimum balance
    /// may be negative.
    last_known_min_balance: i128,
    /// The last known settled accumulator version.
    last_known_settled_version: SequenceNumber,
}

/// Represents an active registration of a transaction waiting for its withdraw
/// reservations to be deterministically satisfied.
#[allow(dead_code)]
pub(crate) struct TxWithdrawRegistration {
    /// The set of accounts and the amount of withdraw reservations in this transaction
    /// that are not yet deterministic where we know for sure
    /// if there is enough balance for the reservation on this account.
    /// When this becomes empty, the transaction is ready to be executed
    /// at least from balance withdraw perspective.
    pub pending_accounts: RwLock<BTreeMap<ObjectID, u64>>,
    /// The channel to notify the transaction when it is ready to be executed.
    pub notify: Mutex<Option<watch::Sender<ScheduleResult>>>,
}

impl AccountState {
    #[allow(dead_code)]
    pub fn new(
        balance_read: &dyn AccountBalanceRead,
        account_id: ObjectID,
        back_pointer: Arc<DashMap<ObjectID, Arc<Self>>>,
        last_known_settled_version: SequenceNumber,
    ) -> Arc<Self> {
        let cur_balance = balance_read.get_account_balance(&account_id, last_known_settled_version);
        let updated_notify = Arc::new(Notify::new());

        let account_state = Arc::new(Self {
            account_id,
            back_pointer,
            updated_notify,
            inner: Arc::new(Mutex::new(AccountStateInner {
                enqueued: BTreeMap::new(),
                reserved: BTreeMap::new(),
                last_known_min_balance: cur_balance as i128,
                last_known_settled_version,
            })),
        });
        // TODO: Handle cancellation.
        tokio::spawn(account_state.clone().scheduling_task());
        account_state
    }

    /// Whenever the account state is updated, this task will be notified and try to schedule as many reservations as possible.
    /// A reservation can be scheduled if either:
    /// 1. The accumulator version of the transaction is the same or older than the last known settled version,
    ///    meaning that the previous accumulator version has been fully settled, and there is no need to wait.
    /// 2. We can guarantee that the account has enough balance to satisfy the reservation.
    async fn scheduling_task(self: Arc<Self>) {
        // TODO: Handle cancellation.
        loop {
            self.updated_notify.notified().await;
            let mut inner = self.inner.lock();
            let mut reserved_updates: BTreeMap<SequenceNumber, u64> = BTreeMap::new();
            let last_known_settled_version = inner.last_known_settled_version;
            let mut last_known_min_balance = inner.last_known_min_balance;
            while !inner.enqueued.is_empty() {
                // Start with the smallest accumulator version.
                let mut first_entry = inner.enqueued.first_entry().unwrap();
                let accumulator_version = *first_entry.key();
                while !first_entry.get().is_empty() {
                    let registration = first_entry.get().front().unwrap();
                    let amount = *registration
                        .pending_accounts
                        .read()
                        .get(&self.account_id)
                        .unwrap();
                    if accumulator_version > last_known_settled_version
                        && amount as i128 > last_known_min_balance
                    {
                        break;
                    }
                    let registration = first_entry.get_mut().pop_front().unwrap();
                    if accumulator_version < last_known_settled_version {
                        registration.transaction_ready();
                    } else {
                        // Process the reservation, and take off the amount from the last_known_min_balance,
                        // since it is now reserved. This reservation will be cleared when we settle this accumulator version.
                        *reserved_updates.entry(accumulator_version).or_default() += amount;
                        last_known_min_balance -= amount as i128;
                        if accumulator_version == last_known_settled_version {
                            // If the accumulator it depends on was already settled,
                            // the transaction is ready to be executed, regardless of other accounts.
                            // This is an optimization to allow the transaction to be executed as soon as possible.
                            // Other accounts will still be processed when we get to them in the caller.
                            registration.transaction_ready();
                            // TODO: But when will these reservations be cleared?
                        } else {
                            registration.account_ready(&self.account_id);
                        }
                    }
                }
                if first_entry.get().is_empty() {
                    first_entry.remove();
                } else {
                    // If there are more transactions with the same accumulator version,
                    // we can stop. We won't be able to schedule any more reservations for this accumulator version,
                    // because reservations on the same account accumulate.
                    break;
                }
            }
            for (accumulator_version, amount) in reserved_updates {
                *inner.reserved.entry(accumulator_version).or_default() += amount;
            }
            inner.last_known_min_balance = last_known_min_balance;
            inner.last_known_settled_version = last_known_settled_version;
        }
    }

    #[allow(dead_code)]
    pub fn add_registrations(
        self: &Arc<Self>,
        accumulator_version: SequenceNumber,
        registrations: Vec<Arc<TxWithdrawRegistration>>,
    ) {
        let mut inner = self.inner.lock();
        assert!(accumulator_version >= inner.last_known_settled_version);
        assert!(registrations
            .iter()
            .all(|r| r.pending_accounts.read().contains_key(&self.account_id)));
        inner
            .enqueued
            .entry(accumulator_version)
            .or_default()
            .extend(registrations);
        self.updated_notify.notify_one();
    }

    #[allow(dead_code)]
    pub fn settle_accumulator_version(
        self: &Arc<Self>,
        settled_accumulator_version: SequenceNumber,
        balance_change: i128,
    ) {
        let mut inner = self.inner.lock();
        while !inner.reserved.is_empty() {
            let accumulator_version = *inner.reserved.keys().next().unwrap();
            if accumulator_version >= settled_accumulator_version {
                break;
            }
            let amount = inner.reserved.remove(&accumulator_version).unwrap();
            inner.last_known_min_balance += amount as i128 + balance_change;
        }
        inner.last_known_settled_version = settled_accumulator_version;
        self.updated_notify.notify_one();
    }

    #[allow(dead_code)]
    pub fn is_empty(self: &Arc<Self>) -> bool {
        let inner = self.inner.lock();
        inner.enqueued.is_empty() && inner.reserved.is_empty()
    }
}

impl TxWithdrawRegistration {
    #[allow(dead_code)]
    pub fn new(
        pending_accounts: BTreeMap<ObjectID, u64>,
        notify_sender: watch::Sender<ScheduleResult>,
    ) -> Arc<Self> {
        Arc::new(Self {
            pending_accounts: RwLock::new(pending_accounts),
            notify: Mutex::new(Some(notify_sender)),
        })
    }

    /// Signals a specific account has reached a deterministic state where
    /// we know for sure if there is enough balance for the reservation on this account.
    /// If all accounts are ready, the transaction is ready to be executed.
    fn account_ready(&self, account_id: &ObjectID) {
        // Because we register each transaction exactly once on each account,
        // and we pop that registration when we process it, it is guaranteed
        // that the account is in the pending accounts map and gets removed
        // exactly once.
        assert!(self.pending_accounts.write().remove(account_id).is_some());
        if self.pending_accounts.read().is_empty() {
            self.transaction_ready();
        }
    }

    fn transaction_ready(&self) {
        if let Some(notify) = self.notify.lock().take() {
            let _ = notify.send(ScheduleResult::ReadyForExecution);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_scheduler::balance_withdraw_scheduler::balance_read::MockBalanceRead;

    #[tokio::test]
    async fn test_account_state_new() {
        let account_id = ObjectID::random();
        let balance = 1000;
        let version = SequenceNumber::from_u64(1);

        let mut mock_balance_read = MockBalanceRead::new(version);
        mock_balance_read.settle_balance_changes(version, BTreeMap::from([(account_id, balance)]));

        let account_state = AccountState::new(&mock_balance_read, &account_id, version);
        assert_eq!(account_state.last_known_min_balance, balance);
        assert!(account_state.enqueued.is_empty());
        assert!(account_state.reserved.is_empty());
    }

    #[test]
    fn test_non_existent_account() {
        let account_id = ObjectID::random();
        let version = SequenceNumber::from_u64(1);
        let mock_balance_read = MockBalanceRead::new(version);
        let account_state = AccountState::new(&mock_balance_read, &account_id, version);
        assert_eq!(account_state.last_known_min_balance, 0);
    }

    #[tokio::test]
    async fn test_account_state_add_registrations() {
        let account_id = ObjectID::random();
        let version = SequenceNumber::from_u64(1);
        let mock_balance_read = MockBalanceRead::new(version);

        let mut account_state = AccountState::new(&mock_balance_read, &account_id, version);

        let mut accounts = BTreeMap::new();
        accounts.insert(account_id, 100);
        let (registration, _) = TxWithdrawRegistration::new(version, accounts);

        account_state.add_registrations(vec![registration]);
        assert_eq!(account_state.enqueued.len(), 1);
    }

    #[tokio::test]
    async fn test_account_state_try_schedule_next_reservation() {
        let account_id = ObjectID::random();
        let version = SequenceNumber::from_u64(1);
        let mut mock_balance_read = MockBalanceRead::new(version);
        mock_balance_read.set_balance(account_id, 1000);

        let mut account_state = AccountState::new(&mock_balance_read, &account_id, version);

        // Add a registration with withdraw amount less than balance
        let mut accounts = BTreeMap::new();
        accounts.insert(account_id, 500);
        let (registration, receiver) = TxWithdrawRegistration::new(version, accounts);
        account_state.add_registrations(vec![registration]);

        // Should be able to schedule the reservation
        let result = account_state.try_schedule_next_reservation(&account_id, version);
        assert!(result.is_some());
        assert_eq!(account_state.last_known_min_balance, 500);
        assert_eq!(*account_state.reserved.get(&version).unwrap(), 500);

        // The registration should be completed
        assert_eq!(*receiver.borrow(), ScheduleResult::ReadyForExecution);
    }

    #[tokio::test]
    async fn test_account_state_insufficient_balance() {
        let account_id = ObjectID::random();
        let version = SequenceNumber::from_u64(1);
        let mut mock_balance_read = MockBalanceRead::new(version);
        mock_balance_read.set_balance(account_id, 100);

        let mut account_state = AccountState::new(&mock_balance_read, &account_id, version);

        // Add a registration with withdraw amount more than balance
        let mut accounts = BTreeMap::new();
        accounts.insert(account_id, 500);
        let (registration, receiver) = TxWithdrawRegistration::new(version, accounts);
        account_state.add_registrations(vec![registration]);

        // Should not be able to schedule the reservation
        let result = account_state.try_schedule_next_reservation(&account_id, version);
        assert!(result.is_none());
        assert_eq!(account_state.last_known_min_balance, 100);
        assert!(account_state.reserved.is_empty());

        // The registration should still be pending
        assert_eq!(*receiver.borrow(), ScheduleResult::Init);
    }

    #[tokio::test]
    async fn test_account_state_multi_account_registration() {
        let version = SequenceNumber::from_u64(1);
        let mut mock_balance_read = MockBalanceRead::new(version);

        // Set up multiple accounts
        let account1 = ObjectID::random();
        let account2 = ObjectID::random();
        mock_balance_read.set_balance(account1, 1000);
        mock_balance_read.set_balance(account2, 2000);

        // Create a registration that involves both accounts
        let mut accounts = BTreeMap::new();
        accounts.insert(account1, 500);
        accounts.insert(account2, 1500);
        let (registration, receiver) = TxWithdrawRegistration::new(version, accounts);

        // Test account1's state
        let mut account1_state = AccountState::new(&mock_balance_read, &account1, version);
        account1_state.add_registrations(vec![registration.clone()]);

        // Should be able to schedule the reservation for account1
        let result = account1_state.try_schedule_next_reservation(&account1, version);
        assert!(result.is_some());
        assert_eq!(account1_state.last_known_min_balance, 500); // 1000 - 500
        assert_eq!(*account1_state.reserved.get(&version).unwrap(), 500);

        // The registration should still be pending because account2 hasn't been processed
        assert_eq!(*receiver.borrow(), ScheduleResult::Init);

        // Test account2's state
        let mut account2_state = AccountState::new(&mock_balance_read, &account2, version);
        account2_state.add_registrations(vec![registration.clone()]);

        // Should be able to schedule the reservation for account2
        let result = account2_state.try_schedule_next_reservation(&account2, version);
        assert!(result.is_some());
        assert_eq!(account2_state.last_known_min_balance, 500); // 2000 - 1500
        assert_eq!(*account2_state.reserved.get(&version).unwrap(), 1500);

        // Now the registration should be complete since both accounts are processed
        assert_eq!(*receiver.borrow(), ScheduleResult::ReadyForExecution);

        // Test settlement
        account1_state.settle_accumulator_version(version);
        assert_eq!(account1_state.last_known_min_balance, 1000);
        assert!(account1_state.reserved.is_empty());

        account2_state.settle_accumulator_version(version);
        assert_eq!(account2_state.last_known_min_balance, 2000);
        assert!(account2_state.reserved.is_empty());
    }
}
