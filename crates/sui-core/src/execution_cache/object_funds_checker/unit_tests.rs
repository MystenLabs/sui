// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Unit tests for the pending object funds withdraws tracker.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use sui_types::{
    accumulator_root::AccumulatorObjId,
    base_types::{ObjectID, SequenceNumber},
    execution_params::FundsWithdrawStatus,
};

use crate::execution_cache::object_funds_checker::{
    ObjectFundsCheckStatus, PendingObjectFundsWithdraws,
};
use crate::execution_scheduler::funds_withdraw_scheduler::mock_funds_read::MockFundsRead;

#[tokio::test]
async fn test_sufficient_balance() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let tracker = PendingObjectFundsWithdraws::new(SequenceNumber::from_u64(0));
    let status = tracker.check(
        funds_read.as_ref(),
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
        SequenceNumber::from_u64(0),
    );
    assert!(matches!(status, ObjectFundsCheckStatus::SufficientFunds));
}

#[tokio::test]
async fn test_insufficient_balance() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let tracker = PendingObjectFundsWithdraws::new(SequenceNumber::from_u64(0));
    let status = tracker.check(
        funds_read.as_ref(),
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(0),
    );
    let ObjectFundsCheckStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    // Since the withdraw version is the same as the last settled version, the tracker can immediately
    // decide that the balance is insufficient.
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);
}

#[tokio::test]
async fn test_pending_wait() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let tracker = PendingObjectFundsWithdraws::new(SequenceNumber::from_u64(0));
    // Attempt to withdraw 101 at version 2.
    let status = tracker.check(
        funds_read.as_ref(),
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(2),
    );
    let ObjectFundsCheckStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    tracker.settle_accumulator_version(SequenceNumber::from_u64(1));
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
    let tracker = PendingObjectFundsWithdraws::new(SequenceNumber::from_u64(0));
    let status = tracker.check(
        funds_read.as_ref(),
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(1),
    );
    let ObjectFundsCheckStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    funds_read.settle_funds_changes(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 1)]),
        SequenceNumber::from_u64(1),
    );
    tracker.settle_accumulator_version(SequenceNumber::from_u64(1));
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::MaybeSufficient);

    let status = tracker.check(
        funds_read.as_ref(),
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(1),
    );
    assert!(matches!(status, ObjectFundsCheckStatus::SufficientFunds));
}

#[tokio::test]
async fn test_pending_then_insufficient() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let tracker = PendingObjectFundsWithdraws::new(SequenceNumber::from_u64(0));
    let status = tracker.check(
        funds_read.as_ref(),
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 102)]),
        SequenceNumber::from_u64(1),
    );
    let ObjectFundsCheckStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    funds_read.settle_funds_changes(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 1)]),
        SequenceNumber::from_u64(1),
    );
    tracker.settle_accumulator_version(SequenceNumber::from_u64(1));
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::MaybeSufficient);

    let status = tracker.check(
        funds_read.as_ref(),
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 102)]),
        SequenceNumber::from_u64(1),
    );
    let ObjectFundsCheckStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);
}

#[tokio::test]
async fn test_multiple_withdraws() {
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account1, 100), (account2, 100)]),
    ));
    let tracker = PendingObjectFundsWithdraws::new(SequenceNumber::from_u64(0));
    let status = tracker.check(
        funds_read.as_ref(),
        BTreeMap::from([
            (AccumulatorObjId::new_unchecked(account1), 100),
            (AccumulatorObjId::new_unchecked(account2), 50),
        ]),
        SequenceNumber::from_u64(0),
    );
    assert!(matches!(status, ObjectFundsCheckStatus::SufficientFunds));
}

#[tokio::test]
async fn test_settle_accumulator_version() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let tracker = PendingObjectFundsWithdraws::new(SequenceNumber::from_u64(0));
    tracker.check(
        funds_read.as_ref(),
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
        SequenceNumber::from_u64(0),
    );
    tracker.settle_accumulator_version(SequenceNumber::from_u64(1));
    assert_eq!(
        tracker.get_current_accumulator_version(),
        SequenceNumber::from_u64(1)
    );
}

#[tokio::test]
async fn test_account_version_ahead_of_check() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    funds_read.settle_funds_changes(
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 1)]),
        SequenceNumber::from_u64(1),
    );
    let tracker = PendingObjectFundsWithdraws::new(SequenceNumber::from_u64(0));
    let result = tracker.check(
        funds_read.as_ref(),
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(0),
    );
    let ObjectFundsCheckStatus::Pending(receiver) = result else {
        panic!("Expected pending status");
    };
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);

    let result = tracker.check(
        funds_read.as_ref(),
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
        SequenceNumber::from_u64(0),
    );
    assert!(matches!(result, ObjectFundsCheckStatus::SufficientFunds));
}

#[tokio::test]
async fn test_settle_ahead_of_check() {
    let account = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account, 100)]),
    ));
    let tracker = PendingObjectFundsWithdraws::new(SequenceNumber::from_u64(0));
    tracker.settle_accumulator_version(SequenceNumber::from_u64(1));
    let result = tracker.check(
        funds_read.as_ref(),
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
        SequenceNumber::from_u64(0),
    );
    let ObjectFundsCheckStatus::Pending(receiver) = result else {
        panic!("Expected pending status");
    };
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);
}

#[tokio::test]
async fn test_check_out_of_order() {
    // Check a withdraw on account A at version 0,
    // then a withdraw on account B at version 1,
    // then a withdraw on account A at version 0 again.
    // This is valid since transactions touching different accounts can be executed in any order.
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let funds_read = Arc::new(MockFundsRead::new(
        SequenceNumber::from_u64(0),
        BTreeMap::from([(account1, 100), (account2, 100)]),
    ));
    let tracker = PendingObjectFundsWithdraws::new(SequenceNumber::from_u64(0));
    let status = tracker.check(
        funds_read.as_ref(),
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account1), 100)]),
        SequenceNumber::from_u64(0),
    );
    assert!(matches!(status, ObjectFundsCheckStatus::SufficientFunds));
    let status = tracker.check(
        funds_read.as_ref(),
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account2), 100)]),
        SequenceNumber::from_u64(1),
    );
    let ObjectFundsCheckStatus::Pending(_) = status else {
        panic!("Expected pending status");
    };
    let status = tracker.check(
        funds_read.as_ref(),
        BTreeMap::from([(AccumulatorObjId::new_unchecked(account1), 100)]),
        SequenceNumber::from_u64(0),
    );
    let ObjectFundsCheckStatus::Pending(receiver) = status else {
        panic!("Expected pending status");
    };
    let result = receiver.await.unwrap();
    assert_eq!(result, FundsWithdrawStatus::Insufficient);
}
