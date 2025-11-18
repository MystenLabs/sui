// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for the implementation of the object balance withdraw scheduler.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use sui_types::{
    accumulator_root::AccumulatorObjId,
    base_types::{ObjectID, SequenceNumber},
    execution_params::BalanceWithdrawStatus,
};

use crate::execution_scheduler::balance_withdraw_scheduler::{
    mock_balance_read::MockBalanceRead,
    naive_scheduler::{ObjectBalanceWithdrawScheduler, ObjectBalanceWithdrawStatus},
};

#[tokio::test]
async fn test_sufficient_balance() {
    let account = ObjectID::random();
    let scheduler = ObjectBalanceWithdrawScheduler::new(
        Arc::new(MockBalanceRead::new(
            SequenceNumber::from_u64(0),
            BTreeMap::from([(account, 100)]),
        )),
        SequenceNumber::from_u64(0),
    );
    let status = scheduler.schedule(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
        SequenceNumber::from_u64(0),
    );
    assert!(matches!(
        status,
        ObjectBalanceWithdrawStatus::SufficientBalance
    ));
    assert_eq!(
        scheduler.get_unsettled_withdraw_amount(&AccumulatorObjId::new_unchecked(account)),
        100
    );
    assert_eq!(
        scheduler.get_tracked_versions(),
        BTreeSet::from([SequenceNumber::from_u64(0)])
    );
    assert_eq!(
        scheduler.get_tracked_accounts(SequenceNumber::from_u64(0)),
        BTreeSet::from([AccumulatorObjId::new_unchecked(account)])
    );
}

#[tokio::test]
async fn test_insufficient_balance() {
    let account = ObjectID::random();
    let scheduler = ObjectBalanceWithdrawScheduler::new(
        Arc::new(MockBalanceRead::new(
            SequenceNumber::from_u64(0),
            BTreeMap::from([(account, 100)]),
        )),
        SequenceNumber::from_u64(0),
    );
    let status = scheduler.schedule(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(0),
    );
    let ObjectBalanceWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    // Since the withdraw version is the same as the last settled version, the scheduler can immediately
    // decide that the balance is insufficient.
    let result = receiver.await.unwrap();
    assert_eq!(result, BalanceWithdrawStatus::InsufficientBalance);
    assert_eq!(
        scheduler.get_unsettled_withdraw_amount(&AccumulatorObjId::new_unchecked(account)),
        0
    );
    assert_eq!(scheduler.get_tracked_versions(), BTreeSet::new());
}

#[tokio::test]
async fn test_pending_wait() {
    let account = ObjectID::random();
    let scheduler = ObjectBalanceWithdrawScheduler::new(
        Arc::new(MockBalanceRead::new(
            SequenceNumber::from_u64(0),
            BTreeMap::from([(account, 100)]),
        )),
        SequenceNumber::from_u64(0),
    );
    // Attempt to withdraw 101 at version 2.
    let status = scheduler.schedule(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(2),
    );
    let ObjectBalanceWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    assert_eq!(
        scheduler.get_unsettled_withdraw_amount(&AccumulatorObjId::new_unchecked(account)),
        0
    );
    assert_eq!(scheduler.get_tracked_versions(), BTreeSet::new());

    scheduler.settle_accumulator_version(SequenceNumber::from_u64(1));
    // The wait won't finish until it observes version 2.
    assert!(
        tokio::time::timeout(Duration::from_secs(1), receiver)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn test_pending_then_sufficient() {
    let account = ObjectID::random();
    let balance_read = Arc::new(MockBalanceRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let scheduler =
        ObjectBalanceWithdrawScheduler::new(balance_read.clone(), SequenceNumber::from_u64(0));
    let status = scheduler.schedule(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(1),
    );
    let ObjectBalanceWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    balance_read.settle_balance_changes(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 1)]),
        SequenceNumber::from_u64(1),
    );
    scheduler.settle_accumulator_version(SequenceNumber::from_u64(1));
    let result = receiver.await.unwrap();
    assert_eq!(result, BalanceWithdrawStatus::Unknown);
    assert_eq!(
        scheduler.get_unsettled_withdraw_amount(&AccumulatorObjId::new_unchecked(account)),
        0
    );
    assert_eq!(scheduler.get_tracked_versions(), BTreeSet::new());

    let status = scheduler.schedule(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(1),
    );
    assert!(matches!(
        status,
        ObjectBalanceWithdrawStatus::SufficientBalance
    ));
    assert_eq!(
        scheduler.get_unsettled_withdraw_amount(&AccumulatorObjId::new_unchecked(account)),
        101
    );
    assert_eq!(
        scheduler.get_tracked_versions(),
        BTreeSet::from([SequenceNumber::from_u64(1)])
    );
    assert_eq!(
        scheduler.get_tracked_accounts(SequenceNumber::from_u64(1)),
        BTreeSet::from([AccumulatorObjId::new_unchecked(account)])
    );
}

#[tokio::test]
async fn test_pending_then_insufficient() {
    let account = ObjectID::random();
    let balance_read = Arc::new(MockBalanceRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let scheduler =
        ObjectBalanceWithdrawScheduler::new(balance_read.clone(), SequenceNumber::from_u64(0));
    let status = scheduler.schedule(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 102)]),
        SequenceNumber::from_u64(1),
    );
    let ObjectBalanceWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    balance_read.settle_balance_changes(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 1)]),
        SequenceNumber::from_u64(1),
    );
    scheduler.settle_accumulator_version(SequenceNumber::from_u64(1));
    let result = receiver.await.unwrap();
    assert_eq!(result, BalanceWithdrawStatus::InsufficientBalance);
    assert_eq!(
        scheduler.get_unsettled_withdraw_amount(&AccumulatorObjId::new_unchecked(account)),
        0
    );
    assert_eq!(scheduler.get_tracked_versions(), BTreeSet::new());
}

#[tokio::test]
async fn test_multiple_withdraws() {
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let balance_read = Arc::new(MockBalanceRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account1, 100), (account2, 100)]),
    ));
    let scheduler =
        ObjectBalanceWithdrawScheduler::new(balance_read.clone(), SequenceNumber::from_u64(0));
    let status = scheduler.schedule(
        BTreeMap::from([
            (AccumulatorObjId::new_unchecked(account1), 100),
            (AccumulatorObjId::new_unchecked(account2), 50),
        ]),
        SequenceNumber::from_u64(0),
    );
    assert!(matches!(
        status,
        ObjectBalanceWithdrawStatus::SufficientBalance
    ));
    assert_eq!(
        scheduler.get_unsettled_withdraw_amount(&AccumulatorObjId::new_unchecked(account1)),
        100
    );
    assert_eq!(
        scheduler.get_unsettled_withdraw_amount(&AccumulatorObjId::new_unchecked(account2)),
        50
    );
    assert_eq!(
        scheduler.get_tracked_versions(),
        BTreeSet::from([SequenceNumber::from_u64(0)])
    );
    assert_eq!(
        scheduler.get_tracked_accounts(SequenceNumber::from_u64(0)),
        BTreeSet::from([
            AccumulatorObjId::new_unchecked(account1),
            AccumulatorObjId::new_unchecked(account2)
        ])
    );
}

#[tokio::test]
async fn test_settle_accumulator_version() {
    let account = ObjectID::random();
    let balance_read = Arc::new(MockBalanceRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let scheduler =
        ObjectBalanceWithdrawScheduler::new(balance_read.clone(), SequenceNumber::from_u64(0));
    scheduler.schedule(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
        SequenceNumber::from_u64(0),
    );
    scheduler.settle_accumulator_version(SequenceNumber::from_u64(1));
    assert_eq!(
        scheduler.get_unsettled_withdraw_amount(&AccumulatorObjId::new_unchecked(account)),
        0
    );
    assert_eq!(scheduler.get_tracked_versions(), BTreeSet::new());
}
