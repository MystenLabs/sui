// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use sui_types::{
    accumulator_root::AccumulatorObjId,
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
};

use crate::execution_scheduler::funds_withdraw_scheduler::{
    FundsSettlement, ScheduleStatus, TxFundsWithdraw, WithdrawReservations,
    address_funds::{ScheduleResult, eager_scheduler::EagerFundsWithdrawScheduler},
    mock_funds_read::MockFundsRead,
    scheduler::FundsWithdrawSchedulerTrait,
};

#[derive(Clone)]
struct TestScheduler {
    mock_read: Arc<MockFundsRead>,
    scheduler: Arc<EagerFundsWithdrawScheduler>,
}

impl TestScheduler {
    fn new(init_version: SequenceNumber, init_funds: BTreeMap<ObjectID, u128>) -> Self {
        let mock_read = Arc::new(MockFundsRead::new(init_version, init_funds));
        let scheduler = EagerFundsWithdrawScheduler::new(mock_read.clone(), init_version);
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
async fn test_schedule_early_sufficient_funds() {
    // When an account has sufficient funds, even when the accumulator version is not yet at the version that the withdraw is scheduled for,
    // the withdraw should be scheduled immediately.
    let init_version = SequenceNumber::from_u64(0);
    let v1 = init_version.next();
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 100)]));
    let withdraw = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 100)]),
    };
    let results = test.schedule_withdraws(v1, vec![withdraw.clone()]);
    assert!(matches!(
        results.get(&withdraw.tx_digest).unwrap(),
        ScheduleResult::ScheduleResult(ScheduleStatus::SufficientFunds)
    ));
}

#[tokio::test]
async fn test_schedule_multiple_accounts() {
    // Multiple accounts are reserved in the same transaction, one has sufficient funds, one does not.
    // The scheduler should reserve for one account and mark pending for the other.
    let init_version = SequenceNumber::from_u64(0);
    let v1 = init_version.next();
    let account1 = ObjectID::random();
    let account2 = ObjectID::random();
    let test = TestScheduler::new(
        init_version,
        BTreeMap::from([(account1, 100), (account2, 100)]),
    );
    let withdraw1 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([
            (AccumulatorObjId::new_unchecked(account1), 50),
            (AccumulatorObjId::new_unchecked(account2), 101),
        ]),
    };
    let withdraw2 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account1), 50)]),
    };
    let withdraw3 = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account1), 1)]),
    };
    let results = test.schedule_withdraws(
        v1,
        vec![withdraw1.clone(), withdraw2.clone(), withdraw3.clone()],
    );
    assert!(matches!(
        results.get(&withdraw1.tx_digest).unwrap(),
        ScheduleResult::Pending(_)
    ));
    assert!(matches!(
        results.get(&withdraw2.tx_digest).unwrap(),
        ScheduleResult::ScheduleResult(ScheduleStatus::SufficientFunds)
    ));
    assert!(matches!(
        results.get(&withdraw3.tx_digest).unwrap(),
        ScheduleResult::Pending(_)
    ));
}

#[tokio::test]
async fn test_schedule_settle() {
    let init_version = SequenceNumber::from_u64(0);
    let v1 = init_version.next();
    let account = ObjectID::random();
    let test = TestScheduler::new(init_version, BTreeMap::from([(account, 100)]));
    let withdraw = TxFundsWithdraw {
        tx_digest: TransactionDigest::random(),
        reservations: BTreeMap::from([(AccumulatorObjId::new_unchecked(account), 101)]),
    };
    let mut results = test.schedule_withdraws(v1, vec![withdraw.clone()]);
    let ScheduleResult::Pending(receiver) = results.remove(&withdraw.tx_digest).unwrap() else {
        panic!("Expected a pending result");
    };
    test.settle_funds_changes(v1, BTreeMap::from([(account, 1)]))
        .await;
    let result = receiver.await.unwrap();
    assert_eq!(result, ScheduleStatus::SufficientFunds);
}
