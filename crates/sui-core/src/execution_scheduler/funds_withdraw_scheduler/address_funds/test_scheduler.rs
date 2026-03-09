// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_scheduler::funds_withdraw_scheduler::{
    ScheduleStatus, WithdrawReservations,
    address_funds::{
        eager_scheduler::EagerFundsWithdrawScheduler, naive_scheduler::NaiveFundsWithdrawScheduler,
    },
    mock_funds_read::MockFundsRead,
    scheduler::FundsWithdrawSchedulerTrait,
};

use super::{FundsSettlement, ScheduleResult, TxFundsWithdraw};
use std::{collections::BTreeMap, sync::Arc};
use sui_types::{
    accumulator_root::AccumulatorObjId,
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
};

#[derive(Clone)]
pub struct TestScheduler {
    pub mock_read: Arc<MockFundsRead>,
    pub scheduler: Arc<dyn FundsWithdrawSchedulerTrait>,
}

impl TestScheduler {
    pub fn new_naive(init_version: SequenceNumber, init_funds: BTreeMap<ObjectID, u128>) -> Self {
        let mock_read = Arc::new(MockFundsRead::new(init_version, init_funds));
        let scheduler = NaiveFundsWithdrawScheduler::new(mock_read.clone(), init_version);
        Self {
            mock_read,
            scheduler,
        }
    }

    pub fn new_eager(init_version: SequenceNumber, init_funds: BTreeMap<ObjectID, u128>) -> Self {
        let mock_read = Arc::new(MockFundsRead::new(init_version, init_funds));
        let scheduler = EagerFundsWithdrawScheduler::new(mock_read.clone(), init_version);
        Self {
            mock_read,
            scheduler,
        }
    }

    pub fn schedule_withdraws(
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

    pub async fn settle_funds_changes(
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

pub fn expect_schedule_results(
    results: BTreeMap<TransactionDigest, ScheduleResult>,
    expected: BTreeMap<TransactionDigest, ScheduleStatus>,
) {
    for (tx_digest, result) in results {
        let expected_status = expected.get(&tx_digest).unwrap();
        assert_eq!(result.unwrap_status(), *expected_status);
    }
}
