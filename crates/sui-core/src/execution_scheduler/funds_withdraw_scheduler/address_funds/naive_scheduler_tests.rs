// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_scheduler::funds_withdraw_scheduler::address_funds::test_scheduler::{
    TestScheduler, expect_schedule_results,
};

use super::{FundsSettlement, ScheduleResult, ScheduleStatus, TxFundsWithdraw};
use parking_lot::Mutex;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use sui_types::{
    accumulator_root::AccumulatorObjId,
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
};
use tokio::time::sleep;

#[tokio::test]
async fn test_schedule_right_away() {
    // When we schedule withdraws at a version that is already settled,
    // we should immediately return the results.
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new_naive(init_version, BTreeMap::from([(account, 100)]));

    let withdraw1 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
    };
    let withdraw2 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 1)]),
    };

    let results = test.schedule_withdraws(init_version, vec![withdraw1.clone(), withdraw2.clone()]);
    expect_schedule_results(
        results,
        BTreeMap::from([
            (withdraw1.tx_digest, ScheduleStatus::SufficientFunds),
            (withdraw2.tx_digest, ScheduleStatus::InsufficientFunds),
        ]),
    );
}

#[tokio::test]
async fn test_already_settled() {
    let init_version = SequenceNumber::from_u64(0);
    let v1 = init_version.next();
    let account = ObjectID::random();
    let test = TestScheduler::new_naive(v1, BTreeMap::from([(account, 100)]));

    let withdraw = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
    };
    let results = test.schedule_withdraws(init_version, vec![withdraw.clone()]);
    expect_schedule_results(
        results,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SkipSchedule)]),
    );

    // Bump the underlying object version to v2.
    // Even though the scheduler itself is still at v0 as the last settled version,
    // withdrawing v1 is still considered as already settled since the object version is already at v2.
    let v2 = v1.next();
    test.mock_read
        .settle_funds_changes(
            BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 0)]),
            v2,
        )
        .await;
    let results = test.schedule_withdraws(v1, vec![withdraw.clone()]);
    expect_schedule_results(
        results,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SkipSchedule)]),
    );
}

#[tokio::test]
async fn test_withdraw_one_account_already_settled() {
    let init_version = SequenceNumber::from_u64(0);
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let test = TestScheduler::new_naive(
        init_version,
        BTreeMap::from([(account1, 100), (account2, 200)]),
    );

    // Advance one of the accounts to the next version, but not settle yet.
    test.mock_read
        .settle_funds_changes(
            BTreeMap::from([(AccumulatorObjId::new_unchecked(account1), 0)]),
            init_version.next(),
        )
        .await;

    let withdraw = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([
            (AccumulatorObjId::new_unchecked(account1), 50),
            (AccumulatorObjId::new_unchecked(account2), 100),
        ]),
    };

    let results = test.schedule_withdraws(init_version, vec![withdraw.clone()]);
    expect_schedule_results(
        results,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SkipSchedule)]),
    );
}

#[tokio::test]
async fn test_multiple_withdraws_same_version() {
    // This test checks that even though the second withdraw failed due to insufficient balance,
    // the third withdraw can still be scheduled since the second withdraw does not reserve any balance.
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new_naive(init_version, BTreeMap::from([(account, 90)]));

    let withdraw1 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 50)]),
    };
    let withdraw2 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 50)]),
    };
    let withdraw3 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 40)]),
    };

    let results = test.schedule_withdraws(
        init_version,
        vec![withdraw1.clone(), withdraw2.clone(), withdraw3.clone()],
    );
    expect_schedule_results(
        results,
        BTreeMap::from([
            (withdraw1.tx_digest, ScheduleStatus::SufficientFunds),
            (withdraw2.tx_digest, ScheduleStatus::InsufficientFunds),
            (withdraw3.tx_digest, ScheduleStatus::SufficientFunds),
        ]),
    );
}

#[tokio::test]
async fn test_multiple_withdraws_multiple_accounts_same_version() {
    let init_version = SequenceNumber::from_u64(0);
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let test = TestScheduler::new_naive(
        init_version,
        BTreeMap::from([(account1, 100), (account2, 100)]),
    );

    let withdraw1 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([
            (AccumulatorObjId::new_unchecked(account1), 100),
            (AccumulatorObjId::new_unchecked(account2), 200),
        ]),
    };
    let withdraw2 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account1), 1)]),
    };
    let withdraw3 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account2), 100)]),
    };

    let results = test.schedule_withdraws(
        init_version,
        vec![withdraw1.clone(), withdraw2.clone(), withdraw3.clone()],
    );
    expect_schedule_results(
        results,
        BTreeMap::from([
            (withdraw1.tx_digest, ScheduleStatus::InsufficientFunds),
            (withdraw2.tx_digest, ScheduleStatus::InsufficientFunds),
            (withdraw3.tx_digest, ScheduleStatus::SufficientFunds),
        ]),
    );
}

#[tokio::test]
async fn test_withdraw_settle_and_deleted_account() {
    telemetry_subscribers::init_for_testing();
    let v0 = SequenceNumber::from_u64(0);
    let v1 = v0.next();
    let account = ObjectID::random();
    let account_id = AccumulatorObjId::new_unchecked(account);
    let scheduler = TestScheduler::new_naive(v0, BTreeMap::from([(account, 100)]));

    // Only update the account balance, without calling the scheduler to settle the balances.
    // This means that the scheduler still thinks we are at v0.
    // The settlement of -100 should lead to 0 balance, causing the account to be deleted.
    scheduler
        .mock_read
        .settle_funds_changes(BTreeMap::from([(account_id, -100)]), v1)
        .await;

    let withdraw = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(account_id, 100)]),
    };

    let results = scheduler.schedule_withdraws(v0, vec![withdraw.clone()]);
    expect_schedule_results(
        results,
        BTreeMap::from([(withdraw.tx_digest, ScheduleStatus::SkipSchedule)]),
    );
}

#[tokio::test]
async fn test_schedule_wait_for_settlement() {
    // This test checks that a withdraw cannot be scheduled until
    // a settlement if the version hasn't been settled yet.
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new_naive(init_version, BTreeMap::from([(account, 100)]));

    let withdraw1 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
    };
    let withdraw2 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 1)]),
    };

    let mut results = test.schedule_withdraws(
        init_version.next(),
        vec![withdraw1.clone(), withdraw2.clone()],
    );
    let result1 = results.remove(&withdraw1.tx_digest).unwrap();
    let ScheduleResult::Pending(receiver1) = result1 else {
        panic!("Expected a pending result");
    };
    let result2 = results.remove(&withdraw2.tx_digest).unwrap();
    let ScheduleResult::Pending(receiver2) = result2 else {
        panic!("Expected a pending result");
    };
    test.settle_funds_changes(init_version.next(), BTreeMap::new())
        .await;

    let status1 = receiver1.await.unwrap();
    assert_eq!(status1, ScheduleStatus::SufficientFunds);
    let status2 = receiver2.await.unwrap();
    assert_eq!(status2, ScheduleStatus::InsufficientFunds);
}

#[tokio::test]
async fn test_settle_just_updated_account_object() {
    let v0 = SequenceNumber::from_u64(0);
    let v1 = v0.next();
    let v2 = v1.next();
    let account = ObjectID::random();
    let scheduler = TestScheduler::new_naive(v0, BTreeMap::from([(account, 100u128)]));
    // Bump underlying account object versions to v1.
    scheduler
        .mock_read
        .settle_funds_changes(
            BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 0)]),
            v1,
        )
        .await;
    // Scheduling at v2, with a reservation of 100.
    // Current balance is 100, at version v1.
    let withdraw = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
    };
    let mut results = scheduler.schedule_withdraws(v2, vec![withdraw.clone()]);
    let ScheduleResult::Pending(receiver) = results.remove(&withdraw.tx_digest).unwrap() else {
        panic!("Expected a pending result");
    };
    let status = Arc::new(Mutex::new(None));
    let status_clone = status.clone();
    tokio::spawn(async move {
        let s = receiver.await.unwrap();
        *status_clone.lock() = Some(s);
    });

    // Bring the scheduler to `v1`.
    // The pending withdraw is still pending since the withdraw version is v2.
    scheduler.scheduler.settle_funds(FundsSettlement {
        next_accumulator_version: v1,
        funds_changes: BTreeMap::new(),
    });
    sleep(Duration::from_secs(3)).await;
    assert!(status.lock().is_none());

    // Trigger the scheduler to process the pending withdraw.
    scheduler.settle_funds_changes(v2, BTreeMap::new()).await;
    while status.lock().is_none() {
        sleep(Duration::from_secs(1)).await;
    }

    assert_eq!(status.lock().unwrap(), ScheduleStatus::SufficientFunds);
}
