// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for the implementation of the object funds withdraw scheduler.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use sui_types::{
    accumulator_root::AccumulatorObjId,
    base_types::{ObjectID, SequenceNumber},
    execution_params::FundsWithdrawStatus,
};

use crate::execution_scheduler::funds_withdraw_scheduler::{
    ObjectFundsWithdrawSchedulerTrait, ObjectFundsWithdrawStatus, mock_funds_read::MockFundsRead,
    naive_scheduler::NaiveObjectFundsWithdrawScheduler,
};

#[tokio::test]
async fn test_sufficient_balance() {
    let account = ObjectID::random();
    let scheduler = NaiveObjectFundsWithdrawScheduler::new(
        Arc::new(MockFundsRead::new(
            SequenceNumber::from_u64(0),
            BTreeMap::from([(account, 100)]),
        )),
        SequenceNumber::from_u64(0),
    );
    let status = scheduler.schedule(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
        SequenceNumber::from_u64(0),
    );
    assert!(matches!(status, ObjectFundsWithdrawStatus::SufficientFunds));
}

#[tokio::test]
async fn test_insufficient_balance() {
    let account = ObjectID::random();
    let scheduler = NaiveObjectFundsWithdrawScheduler::new(
        Arc::new(MockFundsRead::new(
            SequenceNumber::from_u64(0),
            BTreeMap::from([(account, 100)]),
        )),
        SequenceNumber::from_u64(0),
    );
    let status = scheduler.schedule(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(0),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    // Since the withdraw version is the same as the last settled version, the scheduler can immediately
    // decide that the balance is insufficient.
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);
}

#[tokio::test]
async fn test_pending_wait() {
    let account = ObjectID::random();
    let scheduler = NaiveObjectFundsWithdrawScheduler::new(
        Arc::new(MockFundsRead::new(
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
    let ObjectFundsWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
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
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let scheduler =
        NaiveObjectFundsWithdrawScheduler::new(funds_read.clone(), SequenceNumber::from_u64(0));
    let status = scheduler.schedule(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(1),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    funds_read.settle_funds_changes(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 1)]),
        SequenceNumber::from_u64(1),
    );
    scheduler.settle_accumulator_version(SequenceNumber::from_u64(1));
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::MaybeSufficient);

    let status = scheduler.schedule(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(1),
    );
    assert!(matches!(status, ObjectFundsWithdrawStatus::SufficientFunds));
}

#[tokio::test]
async fn test_pending_then_insufficient() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let scheduler =
        NaiveObjectFundsWithdrawScheduler::new(funds_read.clone(), SequenceNumber::from_u64(0));
    let status = scheduler.schedule(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 102)]),
        SequenceNumber::from_u64(1),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    funds_read.settle_funds_changes(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 1)]),
        SequenceNumber::from_u64(1),
    );
    scheduler.settle_accumulator_version(SequenceNumber::from_u64(1));
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::MaybeSufficient);

    let status = scheduler.schedule(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 102)]),
        SequenceNumber::from_u64(1),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);
}

#[tokio::test]
async fn test_pending_cancels_on_close_epoch() {
    let account = ObjectID::random();
    let scheduler = NaiveObjectFundsWithdrawScheduler::new(
        Arc::new(MockFundsRead::new(
            SequenceNumber::from_u64(0),
            BTreeMap::from([(account, 100)]),
        )),
        SequenceNumber::from_u64(0),
    );
    let status = scheduler.schedule(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(1),
    );
    let ObjectFundsWithdrawStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };

    scheduler.close_epoch();
    assert!(receiver.await.is_err());
}

#[tokio::test]
async fn test_multiple_withdraws() {
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account1, 100), (account2, 100)]),
    ));
    let scheduler =
        NaiveObjectFundsWithdrawScheduler::new(funds_read.clone(), SequenceNumber::from_u64(0));
    let status = scheduler.schedule(
        BTreeMap::from([
            (AccumulatorObjId::new_unchecked(account1), 100),
            (AccumulatorObjId::new_unchecked(account2), 50),
        ]),
        SequenceNumber::from_u64(0),
    );
    assert!(matches!(status, ObjectFundsWithdrawStatus::SufficientFunds));
}

#[tokio::test]
async fn test_settle_accumulator_version() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let scheduler =
        NaiveObjectFundsWithdrawScheduler::new(funds_read.clone(), SequenceNumber::from_u64(0));
    scheduler.schedule(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
        SequenceNumber::from_u64(0),
    );
    scheduler.settle_accumulator_version(SequenceNumber::from_u64(1));
    assert_eq!(
        scheduler.get_current_accumulator_version(),
        SequenceNumber::from_u64(1)
    );
}
