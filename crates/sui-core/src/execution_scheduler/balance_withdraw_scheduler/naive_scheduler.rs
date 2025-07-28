// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use sui_types::{base_types::SequenceNumber, transaction::Reservation};
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
    _receiver: watch::Receiver<SequenceNumber>,
}

impl NaiveBalanceWithdrawScheduler {
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
            // We need to first walk through all reservations in this transaction
            // to see if we can successfully reserve each of them.
            // If we can, we then update the current balances atomically.
            // If not, we leave the current balances unchanged for the next transaction.
            // We make sure to initialize each account we see in the cur_balances map.
            let mut success = true;
            for (object_id, reservation) in &withdraw.reservations {
                let entry = cur_balances.entry(*object_id).or_insert_with(|| {
                    self.balance_read
                        .get_account_balance(object_id, withdraws.accumulator_version)
                });
                debug!("Starting balance for {:?}: {:?}", object_id, entry);

                match reservation {
                    Reservation::MaxAmountU64(amount) => {
                        if *entry < *amount {
                            debug!(
                                "Insufficient balance for {:?}. Requested: {:?}, Available: {:?}",
                                object_id, amount, entry
                            );
                            success = false;
                            break;
                        }
                    }
                    Reservation::EntireBalance => {
                        // When we want to reserve the entire balance,
                        // we still need to ensure that the entire balance is not already reserved.
                        if *entry == 0 {
                            debug!("No more balance available for {:?}", object_id);
                            success = false;
                            break;
                        }
                    }
                }
            }
            if success {
                debug!("Successfully reserved all withdraws for {:?}", withdraw);
                for (object_id, reservation) in withdraw.reservations {
                    // unwrap safe because we always initialize each account in the above loop.
                    let balance = cur_balances.get_mut(&object_id).unwrap();
                    match reservation {
                        Reservation::MaxAmountU64(amount) => {
                            *balance -= amount;
                        }
                        Reservation::EntireBalance => {
                            // We use 0 remaining balance to indicate that the entire balance is reserved.
                            // This works because we require that all explicit withdraw reservations
                            // use a non-zero amount.
                            *balance = 0;
                        }
                    }
                }
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

    async fn settle_balances(&self, settlement: BalanceSettlement) {
        debug!(
            "Settling balances for version {:?}",
            settlement.accumulator_version
        );
        let _ = self
            .last_settled_version_sender
            .send(settlement.accumulator_version);
    }
}
