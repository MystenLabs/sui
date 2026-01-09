// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_scheduler::funds_withdraw_scheduler::{
    WithdrawReservations, address_funds::naive_scheduler::NaiveFundsWithdrawScheduler,
    mock_funds_read::MockFundsRead, scheduler::FundsWithdrawSchedulerTrait,
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

#[derive(Clone)]
struct TestScheduler {
    mock_read: Arc<MockFundsRead>,
    scheduler: Arc<NaiveFundsWithdrawScheduler>,
}

impl TestScheduler {
    fn new(init_version: SequenceNumber, init_funds: BTreeMap<ObjectID, u128>) -> Self {
        let mock_read = Arc::new(MockFundsRead::new(init_version, init_funds));
        let scheduler = NaiveFundsWithdrawScheduler::new(mock_read.clone(), init_version);
        Self {
            mock_read,
            scheduler,
        }
    }

    fn schedule_withdraws(
        &self,
        accumulator_version: SequenceNumber,
        withdraws: Vec<TxFundsWithdraw>,
    ) -> BTreeMap<TransactionDigest, ScheduleResult> {
        let reservations = WithdrawReservations {
            accumulator_version,
            withdraws,
        };
        self.scheduler.schedule_withdraws(reservations)
    }

    async fn settle_funds_changes(
        &self,
        next_accumulator_version: SequenceNumber,
        changes: BTreeMap<ObjectID, i128>,
    ) {
        let accumulator_changes: BTreeMap<_, _> = changes
            .iter()
            .map(|(id, value)| (AccumulatorObjId::new_unchecked(*id), *value))
            .collect();
        self.mock_read
            .settle_funds_changes(accumulator_changes.clone(), next_accumulator_version)
            .await;
        self.scheduler.settle_funds(FundsSettlement {
            next_accumulator_version,
            funds_changes: accumulator_changes.clone(),
        });
    }
}

#[tokio::test]
async fn test_schedule_wait_for_settlement() {
    // This test checks that a withdraw cannot be scheduled until
    // a settlement if the version hasn't been settled yet.
    let init_version = SequenceNumber::from_u64(0);
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 100)]));

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
    let scheduler = TestScheduler::new(v0, BTreeMap::from([(account, 100u128)]));
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
