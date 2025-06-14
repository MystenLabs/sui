// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use sui_types::base_types::SequenceNumber;
use tokio::sync::watch;

use crate::execution_scheduler::balance_withdraw_scheduler::{
    balance_read::AccountBalanceRead,
    scheduler::{BalanceWithdrawSchedulerTrait, WithdrawReservations},
    BalanceSettlement, ScheduleResult,
};

/// A naive implementation of the balance withdraw scheduler that does not attempt to optimize the scheduling.
/// For each withdraw reservation, it will always wait until the dependent accumulator object is available,
/// and then check if the balance is sufficient.
/// This implementation is simple and easy to understand, but it is not efficient.
/// It is only used to unblock further development of the balance withdraw scheduler.
#[allow(dead_code)]
pub(crate) struct NaiveBalanceWithdrawScheduler {
    balance_read: Arc<dyn AccountBalanceRead>,
    last_settled_version_sender: watch::Sender<SequenceNumber>,
    // We must keep a receiver alive to make sure sends go through and can update the last settled version.
    _receiver: watch::Receiver<SequenceNumber>,
}

impl NaiveBalanceWithdrawScheduler {
    #[allow(dead_code)]
    pub fn new(
        balance_read: Arc<dyn AccountBalanceRead>,
        last_settled_accumulator_version: SequenceNumber,
    ) -> Arc<Self> {
        let (last_settled_version_sender, _receiver) =
            watch::channel(last_settled_accumulator_version);
        Arc::new(Self {
            balance_read,
            last_settled_version_sender,
            _receiver,
        })
    }
}

#[async_trait::async_trait]
impl BalanceWithdrawSchedulerTrait for NaiveBalanceWithdrawScheduler {
    async fn schedule_withdraws(&self, withdraws: WithdrawReservations) {
        let mut receiver = self.last_settled_version_sender.subscribe();
        while *receiver.borrow_and_update() < withdraws.accumulator_version {
            if receiver.changed().await.is_err() {
                return;
            }
        }

        let mut cur_balances = BTreeMap::new();
        for (withdraw, sender) in withdraws.withdraws.into_iter().zip(withdraws.senders) {
            let mut success = true;
            for (object_id, amount) in &withdraw.reservations {
                let balance = cur_balances.entry(*object_id).or_insert_with(|| {
                    self.balance_read
                        .get_account_balance(object_id, withdraws.accumulator_version)
                });
                if *balance < *amount {
                    success = false;
                    break;
                }
            }
            if success {
                for (object_id, amount) in withdraw.reservations {
                    *cur_balances.get_mut(&object_id).unwrap() -= amount;
                }
                let _ = sender.send(ScheduleResult::SufficientBalance);
            } else {
                let _ = sender.send(ScheduleResult::InsufficientBalance);
            }
        }
    }

    async fn settle_balances(&self, settlement: BalanceSettlement) {
        let _ = self
            .last_settled_version_sender
            .send(settlement.accumulator_version);
    }
}
