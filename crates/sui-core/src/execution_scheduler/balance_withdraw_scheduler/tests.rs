// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_scheduler::balance_withdraw_scheduler::{
    account_state::{AccountState, TxWithdrawRegistration},
    balance_read::AccountBalanceRead,
    scheduler::WithdrawScheduler,
    BalanceSettlement, ScheduleResult, TxBalanceWithdraw,
};
use std::collections::BTreeMap;
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
};

// Mock implementation of AccountBalanceRead for testing
#[derive(Default)]
struct MockBalanceRead {
    balances: BTreeMap<ObjectID, u64>,
    accumulator_version: SequenceNumber,
}

impl MockBalanceRead {
    fn new(accumulator_version: SequenceNumber) -> Self {
        Self {
            balances: BTreeMap::new(),
            accumulator_version,
        }
    }

    fn set_balance(&mut self, account_id: ObjectID, balance: u64) {
        self.balances.insert(account_id, balance);
    }
}

impl AccountBalanceRead for MockBalanceRead {
    fn get_accumulator_version(&self) -> SequenceNumber {
        self.accumulator_version
    }

    fn get_account_balance(
        &self,
        account_id: &ObjectID,
        _accumulator_version: SequenceNumber,
    ) -> u64 {
        *self.balances.get(account_id).unwrap_or(&0)
    }
}

#[tokio::test]
async fn test_account_state_new() {
    let account_id = ObjectID::random();
    let balance = 1000;
    let version = SequenceNumber::from_u64(1);

    let mut mock_balance_read = MockBalanceRead::new(version);
    mock_balance_read.set_balance(account_id, balance);

    let account_state = AccountState::new(&mock_balance_read, &account_id, version);
    assert_eq!(account_state.last_known_min_balance, balance as i128);
    assert!(account_state.enqueued.is_empty());
    assert!(account_state.reserved.is_empty());
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

#[tokio::test]
async fn test_withdraw_scheduler() {
    let version = SequenceNumber::from_u64(1);
    let mut mock_balance_read = MockBalanceRead::new(version);

    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    mock_balance_read.set_balance(account1, 1000);
    mock_balance_read.set_balance(account2, 2000);

    let scheduler = WithdrawScheduler::new(&mock_balance_read);

    // Create two transactions with withdraws
    let mut tx1_withdraws = BTreeMap::new();
    tx1_withdraws.insert(account1, 500);
    let tx1 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: tx1_withdraws,
    };

    let mut tx2_withdraws = BTreeMap::new();
    tx2_withdraws.insert(account2, 1500);
    let tx2 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: tx2_withdraws,
    };

    // Schedule the withdraws
    let receivers = scheduler
        .enqueue_withdraw_reservations(version, vec![tx1, tx2], &mock_balance_read)
        .await;

    // Both transactions should be scheduled successfully
    for receiver in receivers.values() {
        assert_eq!(*receiver.borrow(), ScheduleResult::ReadyForExecution);
    }
}

#[tokio::test]
async fn test_withdraw_scheduler_settlement() {
    let version = SequenceNumber::from_u64(1);
    let mut mock_balance_read = MockBalanceRead::new(version);

    let account = ObjectID::random();
    mock_balance_read.set_balance(account, 1000);

    let scheduler = WithdrawScheduler::new(&mock_balance_read);

    // Create a transaction that withdraws more than available balance
    let mut tx_withdraws = BTreeMap::new();
    tx_withdraws.insert(account, 1500);
    let tx = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: tx_withdraws,
    };

    // Schedule the withdraw - should not be ready due to insufficient balance
    let mut receivers = scheduler
        .enqueue_withdraw_reservations(version, vec![tx], &mock_balance_read)
        .await;
    assert_eq!(
        *receivers.values().next().unwrap().borrow(),
        ScheduleResult::Init
    );

    // Process a settlement that increases the balance
    let mut balance_changes = BTreeMap::new();
    balance_changes.insert(account, 1000);
    let settlement = BalanceSettlement {
        old_accumulator_version: version,
        new_accumulator_version: SequenceNumber::from_u64(2),
        balance_changes,
    };
    scheduler.handle_balance_settlement(settlement);

    // The transaction should now be ready for execution
    let receiver = receivers.values_mut().next().unwrap();
    receiver.changed().await.unwrap();
    assert_eq!(*receiver.borrow(), ScheduleResult::ReadyForExecution);
}

#[tokio::test]
async fn test_withdraw_scheduler_multi_account() {
    let version = SequenceNumber::from_u64(1);
    let mut mock_balance_read = MockBalanceRead::new(version);

    // Set up multiple accounts with different balances
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let account3 = ObjectID::random();
    mock_balance_read.set_balance(account1, 1000);
    mock_balance_read.set_balance(account2, 2000);
    mock_balance_read.set_balance(account3, 500);

    let scheduler = WithdrawScheduler::new(&mock_balance_read);

    // Create transactions that withdraw from multiple accounts
    let mut tx1_withdraws = BTreeMap::new();
    tx1_withdraws.insert(account1, 500); // Should succeed (1000 available)
    tx1_withdraws.insert(account2, 1500); // Should succeed (2000 available)
    let tx1 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: tx1_withdraws,
    };

    let mut tx2_withdraws = BTreeMap::new();
    tx2_withdraws.insert(account2, 400); // Should succeed (500 remaining after tx1)
    tx2_withdraws.insert(account3, 600); // Should fail (only 500 available)
    let tx2 = TxBalanceWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: tx2_withdraws,
    };

    // Schedule both transactions
    let receivers = scheduler
        .enqueue_withdraw_reservations(version, vec![tx1, tx2], &mock_balance_read)
        .await;

    let mut receivers_iter = receivers.values();

    // First transaction should succeed as both accounts have sufficient balance
    assert_eq!(
        *receivers_iter.next().unwrap().borrow(),
        ScheduleResult::ReadyForExecution
    );

    // Second transaction should remain pending due to insufficient balance in account3
    assert_eq!(
        *receivers_iter.next().unwrap().borrow(),
        ScheduleResult::Init
    );

    // Process a settlement that increases account3's balance
    let mut balance_changes = BTreeMap::new();
    balance_changes.insert(account3, 200);
    let settlement = BalanceSettlement {
        old_accumulator_version: version,
        new_accumulator_version: SequenceNumber::from_u64(2),
        balance_changes,
    };
    scheduler.handle_balance_settlement(settlement);

    // Now the second transaction should be ready for execution
    assert_eq!(
        *receivers_iter.next().unwrap().borrow(),
        ScheduleResult::ReadyForExecution
    );
}

#[tokio::test]
async fn test_account_state_version_based_scheduling() {
    let account_id = ObjectID::random();
    let base_version = SequenceNumber::from_u64(1);
    let mut mock_balance_read = MockBalanceRead::new(base_version);
    mock_balance_read.set_balance(account_id, 1000);

    let mut account_state = AccountState::new(&mock_balance_read, &account_id, base_version);

    // Create registrations at different versions
    let mut accounts1 = BTreeMap::new();
    accounts1.insert(account_id, 400);
    let (registration1, receiver1) = TxWithdrawRegistration::new(base_version, accounts1);

    let mut accounts2 = BTreeMap::new();
    accounts2.insert(account_id, 300);
    let (registration2, receiver2) =
        TxWithdrawRegistration::new(SequenceNumber::from_u64(2), accounts2);

    let mut accounts3 = BTreeMap::new();
    accounts3.insert(account_id, 200);
    let (registration3, receiver3) = TxWithdrawRegistration::new(base_version, accounts3);

    // Add all registrations
    account_state.add_registrations(vec![
        registration1.clone(),
        registration2.clone(),
        registration3.clone(),
    ]);

    // Case 1: Try scheduling with version matching first registration
    let result = account_state.try_schedule_next_reservation(&account_id, base_version);
    assert!(result.is_some());
    assert_eq!(account_state.last_known_min_balance, 600); // 1000 - 400
    assert_eq!(*account_state.reserved.get(&base_version).unwrap(), 400);
    assert_eq!(*receiver1.borrow(), ScheduleResult::ReadyForExecution);

    // Case 2: Try scheduling with version matching second registration
    // Should fail because version 2 > base_version and balance would go below 0
    let result =
        account_state.try_schedule_next_reservation(&account_id, SequenceNumber::from_u64(2));
    assert!(result.is_none());
    assert_eq!(*receiver2.borrow(), ScheduleResult::Init);

    // Case 3: Try scheduling with version matching third registration (same as base)
    let result = account_state.try_schedule_next_reservation(&account_id, base_version);
    assert!(result.is_some());
    assert_eq!(account_state.last_known_min_balance, 400); // 600 - 200
    assert_eq!(*account_state.reserved.get(&base_version).unwrap(), 600); // 400 + 200
    assert_eq!(*receiver3.borrow(), ScheduleResult::ReadyForExecution);

    // Settle base version
    account_state.settle_accumulator_version(base_version);
    assert_eq!(account_state.last_known_min_balance, 1000);
    assert!(account_state.reserved.is_empty());

    // Now try scheduling version 2 again - should succeed with restored balance
    let result =
        account_state.try_schedule_next_reservation(&account_id, SequenceNumber::from_u64(2));
    assert!(result.is_some());
    assert_eq!(account_state.last_known_min_balance, 700); // 1000 - 300
    assert_eq!(
        *account_state
            .reserved
            .get(&SequenceNumber::from_u64(2))
            .unwrap(),
        300
    );
    assert_eq!(*receiver2.borrow(), ScheduleResult::ReadyForExecution);
}
