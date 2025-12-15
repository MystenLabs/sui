// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use sui_types::base_types::SequenceNumber;
use tokio::sync::watch;
use tracing::{debug, instrument};

use super::{
    BalanceSettlement, ScheduleResult, ScheduleStatus,
    scheduler::{BalanceWithdrawSchedulerTrait, WithdrawReservations},
};
use crate::accumulators::balance_read::AccountBalanceRead;

/// A naive implementation of the balance withdraw scheduler that does not attempt to optimize the scheduling.
/// For each withdraw reservation, it will always wait until the dependent accumulator object is available,
/// and then check if the balance is sufficient.
/// This implementation is simple and easy to understand, but it is not efficient.
/// It is only used to unblock further development of the balance withdraw scheduler.
pub(crate) struct NaiveBalanceWithdrawScheduler {
    balance_read: Arc<dyn AccountBalanceRead>,
    accumulator_version_sender: watch::Sender<SequenceNumber>,
    // We must keep a receiver alive to make sure sends go through and can update the last settled version.
    accumulator_version_receiver: watch::Receiver<SequenceNumber>,
}

impl NaiveBalanceWithdrawScheduler {
    pub fn new(
        balance_read: Arc<dyn AccountBalanceRead>,
        cur_accumulator_version: SequenceNumber,
    ) -> Arc<Self> {
        let (accumulator_version_sender, accumulator_version_receiver) =
            watch::channel(cur_accumulator_version);
        Arc::new(Self {
            balance_read,
            accumulator_version_sender,
            accumulator_version_receiver,
        })
    }
}

#[async_trait::async_trait]
impl BalanceWithdrawSchedulerTrait for NaiveBalanceWithdrawScheduler {
    #[instrument(level = "debug", skip_all, fields(withdraw_accumulator_version = ?withdraws.accumulator_version.value()))]
    async fn schedule_withdraws(&self, withdraws: WithdrawReservations) {
        let mut receiver = self.accumulator_version_sender.subscribe();
        while *receiver.borrow_and_update() < withdraws.accumulator_version {
            debug!("Waiting for the dependent accumulator version to be settled");
            if receiver.changed().await.is_err() {
                return;
            }
        }
        if *receiver.borrow() > withdraws.accumulator_version {
            // This accumulator version is already settled.
            // There is no need to schedule the withdraws.
            withdraws.notify_skip_schedule();
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
        let cur_accumulator_version = *self.accumulator_version_receiver.borrow();
        let next_version = cur_accumulator_version.next();

        if settlement.next_accumulator_version < next_version {
            // This accumulator version is already settled.
            // There is no need to settle the balances.
            debug!(
                next_accumulator_version =? settlement.next_accumulator_version.value(),
                "Skipping settlement since it is already settled",
            );
            return;
        }

        debug!(
            settled_accumulator_version =? cur_accumulator_version.value(),
            next_accumulator_version =? next_version.value(),
            "Settling balances",
        );
        assert_eq!(next_version, settlement.next_accumulator_version);
        let _ = self.accumulator_version_sender.send(next_version);
    }

    fn close_epoch(&self) {
        debug!("Closing epoch in NaiveBalanceWithdrawScheduler");
    }

    #[cfg(test)]
    fn get_current_accumulator_version(&self) -> SequenceNumber {
        *self.accumulator_version_receiver.borrow()
    }
}
