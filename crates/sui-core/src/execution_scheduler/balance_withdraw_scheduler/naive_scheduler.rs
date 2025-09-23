// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use sui_types::base_types::SequenceNumber;
use tokio::sync::watch;
use tracing::debug;

use crate::execution_scheduler::balance_withdraw_scheduler::{
    balance_read::AccountBalanceRead,
    scheduler::{BalanceWithdrawSchedulerTrait, WithdrawReservations},
    BalanceSettlement, ScheduleResult, ScheduleStatus,
};

/// A naive implementation of the balance withdraw scheduler that does not attempt to optimize the scheduling.
/// For each withdraw reservation, it will always wait until the dependent accumulator object is available,
/// and then check if the balance is sufficient.
/// This implementation is simple and easy to understand, but it is not efficient.
/// It is only used to unblock further development of the balance withdraw scheduler.
pub(crate) struct NaiveBalanceWithdrawScheduler {
    balance_read: Arc<dyn AccountBalanceRead>,
    last_settled_version_sender: watch::Sender<SequenceNumber>,
    // We must keep a receiver alive to make sure sends go through and can update the last settled version.
    last_settled_version_receiver: watch::Receiver<SequenceNumber>,
}

impl NaiveBalanceWithdrawScheduler {
    pub fn new(
        balance_read: Arc<dyn AccountBalanceRead>,
        last_settled_accumulator_version: SequenceNumber,
    ) -> Arc<Self> {
        let (last_settled_version_sender, last_settled_version_receiver) =
            watch::channel(last_settled_accumulator_version);
        Arc::new(Self {
            balance_read,
            last_settled_version_sender,
            last_settled_version_receiver,
        })
    }
}

#[async_trait::async_trait]
impl BalanceWithdrawSchedulerTrait for NaiveBalanceWithdrawScheduler {
    async fn schedule_withdraws(&self, withdraws: WithdrawReservations) {
        let mut receiver = self.last_settled_version_sender.subscribe();
        while *receiver.borrow_and_update() < withdraws.accumulator_version {
            debug!(
                "Waiting for accumulator version {:?} to be settled",
                withdraws.accumulator_version
            );
            if receiver.changed().await.is_err() {
                return;
            }
        }
        if *receiver.borrow() > withdraws.accumulator_version {
            debug!(
                "Accumulator version {:?} is already settled",
                withdraws.accumulator_version
            );
            for (withdraw, sender) in withdraws.withdraws.into_iter().zip(withdraws.senders) {
                let _ = sender.send(ScheduleResult {
                    tx_digest: withdraw.tx_digest,
                    status: ScheduleStatus::AlreadyExecuted,
                });
            }
            return;
        }

        // Map from each account ID that we have seen so far to the current
        // remaining balance for reservation.
        let mut cur_balances = BTreeMap::new();
        for (withdraw, sender) in withdraws.withdraws.into_iter().zip(withdraws.senders) {
            // Try to reserve all withdraws in this transaction.
            // Note that this is not atomic, so it is possible that we reserve some withdraws and not others.
            // This is intentional to be semantically consistent with the eager scheduler.
            let mut success = true;
            for (object_id, reservation) in &withdraw.reservations {
                let entry = cur_balances.entry(*object_id).or_insert_with(|| {
                    self.balance_read
                        .get_account_balance(object_id, withdraws.accumulator_version)
                });

                if *entry < *reservation as u128 {
                    debug!(
                        tx_digest =? withdraw.tx_digest,
                        "Insufficient balance for {:?}. Requested: {:?}, Available: {:?}",
                        object_id, reservation, entry
                    );
                    success = false;
                } else {
                    debug!(
                        tx_digest =? withdraw.tx_digest,
                        "Successfully reserved {:?} for account {:?}",
                        reservation, object_id
                    );
                    *entry -= *reservation as u128;
                }
            }
            if success {
                debug!(
                    tx_digest =? withdraw.tx_digest,
                    "Successfully reserved all withdraws"
                );
                let _ = sender.send(ScheduleResult {
                    tx_digest: withdraw.tx_digest,
                    status: ScheduleStatus::SufficientBalance,
                });
            } else {
                let _ = sender.send(ScheduleResult {
                    tx_digest: withdraw.tx_digest,
                    status: ScheduleStatus::InsufficientBalance,
                });
            }
        }
    }

    // We don't use the balance changes information in the naive scheduler.
    // Instead, the withdraw scheduling always read the balance state fro storage.
    async fn settle_balances(&self, settlement: BalanceSettlement) {
        let next_version = self.last_settled_version_receiver.borrow().next();
        debug!("Settling balances for version {:?}", next_version);
        assert_eq!(next_version, settlement.next_accumulator_version);
        let _ = self.last_settled_version_sender.send(next_version);
    }
}
