// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use sui_types::base_types::SequenceNumber;
use tokio::sync::watch;
use tracing::{debug, instrument};

use super::{
    FundsSettlement, ScheduleResult, ScheduleStatus,
    scheduler::{FundsWithdrawSchedulerTrait, WithdrawReservations},
};
use crate::accumulators::funds_read::AccountFundsRead;

/// A naive implementation of the funds withdraw scheduler that does not attempt to optimize the scheduling.
/// For each withdraw reservation, it will always wait until the dependent accumulator object is available,
/// and then check if the funds are sufficient.
/// This implementation is simple and easy to understand, but it is not efficient.
/// It is only used to unblock further development of the funds withdraw scheduler.
pub(crate) struct NaiveFundsWithdrawScheduler {
    funds_read: Arc<dyn AccountFundsRead>,
    accumulator_version_sender: watch::Sender<SequenceNumber>,
    // We must keep a receiver alive to make sure sends go through and can update the last settled version.
    accumulator_version_receiver: watch::Receiver<SequenceNumber>,
}

impl NaiveFundsWithdrawScheduler {
    pub fn new(
        funds_read: Arc<dyn AccountFundsRead>,
        cur_accumulator_version: SequenceNumber,
    ) -> Arc<Self> {
        let (accumulator_version_sender, accumulator_version_receiver) =
            watch::channel(cur_accumulator_version);
        Arc::new(Self {
            funds_read,
            accumulator_version_sender,
            accumulator_version_receiver,
        })
    }
}

#[async_trait::async_trait]
impl FundsWithdrawSchedulerTrait for NaiveFundsWithdrawScheduler {
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
        // remaining funds for reservation.
        let mut cur_funds = BTreeMap::new();
        for (withdraw, sender) in withdraws.withdraws.into_iter().zip(withdraws.senders) {
            // Try to reserve all withdraws in this transaction.
            // Note that this is not atomic, so it is possible that we reserve some withdraws and not others.
            // This is intentional to be semantically consistent with the eager scheduler.
            let mut success = true;
            for (object_id, reservation) in &withdraw.reservations {
                let entry = cur_funds.entry(*object_id).or_insert_with(|| {
                    self.funds_read
                        .get_account_amount(object_id, withdraws.accumulator_version)
                });
                if *entry < *reservation as u128 {
                    debug!(
                        tx_digest =? withdraw.tx_digest,
                        "Insufficient funds for {:?}. Requested: {:?}, Available: {:?}",
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
                    status: ScheduleStatus::SufficientFunds,
                });
            } else {
                let _ = sender.send(ScheduleResult {
                    tx_digest: withdraw.tx_digest,
                    status: ScheduleStatus::InsufficientFunds,
                });
            }
        }
    }

    // We don't use the funds changes information in the naive scheduler.
    // Instead, the withdraw scheduling always reads the funds state from storage.
    async fn settle_funds(&self, settlement: FundsSettlement) {
        let cur_accumulator_version = *self.accumulator_version_receiver.borrow();
        let next_version = cur_accumulator_version.next();

        if settlement.next_accumulator_version < next_version {
            // This accumulator version is already settled.
            // There is no need to settle the funds.
            debug!(
                next_accumulator_version =? settlement.next_accumulator_version.value(),
                "Skipping settlement since it is already settled",
            );
            return;
        }

        debug!(
            settled_accumulator_version =? cur_accumulator_version.value(),
            next_accumulator_version =? next_version.value(),
            "Settling funds",
        );
        assert_eq!(next_version, settlement.next_accumulator_version);
        let _ = self.accumulator_version_sender.send(next_version);
    }

    fn close_epoch(&self) {
        debug!("Closing epoch in NaiveFundsWithdrawScheduler");
    }

    #[cfg(test)]
    fn get_current_accumulator_version(&self) -> SequenceNumber {
        *self.accumulator_version_receiver.borrow()
    }
}
