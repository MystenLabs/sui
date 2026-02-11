// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use sui_types::{base_types::SequenceNumber, digests::TransactionDigest};
use tokio::sync::{oneshot, watch};
use tracing::{debug, instrument};

use super::{
    FundsSettlement, ScheduleResult, ScheduleStatus, scheduler::FundsWithdrawSchedulerTrait,
};
use crate::{
    accumulators::funds_read::AccountFundsRead,
    execution_scheduler::funds_withdraw_scheduler::WithdrawReservations,
};

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

    fn process_withdraws(
        funds_read: &dyn AccountFundsRead,
        reservations: WithdrawReservations,
    ) -> BTreeMap<TransactionDigest, ScheduleResult> {
        // Map from each account ID that we have seen so far to the current
        // remaining funds for reservation.
        let mut cur_funds = BTreeMap::new();
        let all_accounts = reservations.all_accounts();
        for account_id in all_accounts {
            // TODO: We can warm up the cache prior to holding the lock.
            let (balance, version) = funds_read.get_latest_account_amount(&account_id);
            if version > reservations.accumulator_version {
                return reservations.notify_skip_schedule();
            }
            cur_funds.insert(account_id, balance);
        }
        reservations
            .withdraws
            .into_iter()
            .map(|withdraw| {
                // Try to reserve all withdraws in this transaction.
                // Note that this is not atomic, so it is possible that we reserve some withdraws and not others.
                // This is intentional to be semantically consistent with the eager scheduler.
                let mut success = true;
                for (object_id, reservation) in &withdraw.reservations {
                    let entry = cur_funds.get_mut(object_id).unwrap();
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
                let status = if success {
                    debug!(
                        tx_digest =? withdraw.tx_digest,
                        "Successfully reserved all withdraws"
                    );
                    ScheduleStatus::SufficientFunds
                } else {
                    ScheduleStatus::InsufficientFunds
                };
                (withdraw.tx_digest, ScheduleResult::ScheduleResult(status))
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl FundsWithdrawSchedulerTrait for NaiveFundsWithdrawScheduler {
    #[instrument(level = "debug", skip_all, fields(withdraw_accumulator_version = ?reservations.accumulator_version.value()))]
    fn schedule_withdraws(
        &self,
        reservations: WithdrawReservations,
    ) -> BTreeMap<TransactionDigest, ScheduleResult> {
        let mut receiver = self.accumulator_version_sender.subscribe();
        let cur_version = *receiver.borrow_and_update();
        if cur_version > reservations.accumulator_version {
            return reservations.notify_skip_schedule();
        }
        if cur_version == reservations.accumulator_version {
            return Self::process_withdraws(self.funds_read.as_ref(), reservations);
        }

        let (mut senders, receivers): (BTreeMap<_, _>, BTreeMap<_, _>) = reservations
            .withdraws
            .iter()
            .map(|withdraw| {
                let (sender, receiver) = oneshot::channel();
                (
                    (withdraw.tx_digest, sender),
                    (withdraw.tx_digest, ScheduleResult::Pending(receiver)),
                )
            })
            .unzip();

        let funds_read = self.funds_read.clone();
        tokio::spawn(async move {
            while *receiver.borrow_and_update() < reservations.accumulator_version {
                debug!("Waiting for the dependent accumulator version to be settled");
                if receiver.changed().await.is_err() {
                    tracing::error!(
                        "Accumulator version receiver channel closed while waiting for accumulator version"
                    );
                    return;
                }
            }
            if *receiver.borrow() > reservations.accumulator_version {
                for sender in senders.into_values() {
                    let _ = sender.send(ScheduleStatus::SkipSchedule);
                }
                return;
            }
            let results = Self::process_withdraws(funds_read.as_ref(), reservations);
            for (tx_digest, result) in results {
                let sender = senders.remove(&tx_digest).unwrap();
                // unwrap is safe because process_withdraws always return a finalized result.
                let _ = sender.send(result.unwrap_status());
            }
        });

        receivers
    }

    // We don't use the funds changes information in the naive scheduler.
    // Instead, the withdraw scheduling always reads the funds state from storage.
    fn settle_funds(&self, settlement: FundsSettlement) {
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

    fn num_tracked_accounts(&self) -> usize {
        0
    }

    #[cfg(test)]
    fn get_current_accumulator_version(&self) -> SequenceNumber {
        *self.accumulator_version_receiver.borrow()
    }
}
