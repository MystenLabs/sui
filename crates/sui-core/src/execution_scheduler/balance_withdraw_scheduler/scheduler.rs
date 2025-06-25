// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use crate::execution_scheduler::balance_withdraw_scheduler::{
    balance_read::AccountBalanceRead, naive_scheduler::NaiveBalanceWithdrawScheduler,
    BalanceSettlement, ScheduleResult, ScheduleStatus, TxBalanceWithdraw,
};
use futures::stream::FuturesUnordered;
use mysten_metrics::monitored_mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use sui_types::base_types::SequenceNumber;
use tokio::sync::oneshot;
use tracing::debug;

#[async_trait::async_trait]
pub(crate) trait BalanceWithdrawSchedulerTrait: Send + Sync {
    async fn schedule_withdraws(&self, withdraws: WithdrawReservations);
    async fn settle_balances(&self, settlement: BalanceSettlement);
}

pub(crate) struct WithdrawReservations {
    pub accumulator_version: SequenceNumber,
    pub withdraws: Vec<TxBalanceWithdraw>,
    pub senders: Vec<oneshot::Sender<ScheduleResult>>,
}

#[derive(Clone)]
pub(crate) struct BalanceWithdrawScheduler {
    inner: Arc<dyn BalanceWithdrawSchedulerTrait>,
    /// Use channels to process withdraws and settlements asynchronously without blocking the caller.
    withdraw_sender: UnboundedSender<WithdrawReservations>,
    settlement_sender: UnboundedSender<BalanceSettlement>,
}

impl WithdrawReservations {
    pub fn new(
        accumulator_version: SequenceNumber,
        withdraws: Vec<TxBalanceWithdraw>,
    ) -> (Self, FuturesUnordered<oneshot::Receiver<ScheduleResult>>) {
        let (senders, receivers) = (0..withdraws.len())
            .map(|_| {
                let (sender, receiver) = oneshot::channel();
                (sender, receiver)
            })
            .unzip();
        (
            Self {
                accumulator_version,
                withdraws,
                senders,
            },
            receivers,
        )
    }
}

impl BalanceWithdrawScheduler {
    pub fn new(
        balance_read: Arc<dyn AccountBalanceRead>,
        starting_accumulator_version: SequenceNumber,
    ) -> Arc<Self> {
        let inner = NaiveBalanceWithdrawScheduler::new(balance_read, starting_accumulator_version);
        let (withdraw_sender, withdraw_receiver) =
            unbounded_channel("withdraw_scheduler_withdraws");
        let (settlement_sender, settlement_receiver) =
            unbounded_channel("withdraw_scheduler_settlements");
        let scheduler = Arc::new(Self {
            inner,
            withdraw_sender,
            settlement_sender,
        });
        tokio::spawn(scheduler.clone().process_withdraw_task(withdraw_receiver));
        tokio::spawn(
            scheduler
                .clone()
                .process_settlement_task(settlement_receiver, starting_accumulator_version),
        );
        scheduler
    }

    /// This function will be called either by ConsensusHandler or the CheckpointExecutor.
    /// It will be called at most once per consensus commit batch that all reads the same root accumulator version.
    /// If a consensus commit batch does not contain any withdraw reservations, it can skip calling this function.
    /// It must be called sequentially in order to correctly schedule withdraws.
    /// It is OK to call this function multiple times for the same accumulator version (which will happen between
    /// the calls to the function by ConsensusHandler and the CheckpointExecutor).
    pub fn schedule_withdraws(
        &self,
        accumulator_version: SequenceNumber,
        withdraws: Vec<TxBalanceWithdraw>,
    ) -> FuturesUnordered<oneshot::Receiver<ScheduleResult>> {
        debug!(
            "schedule_withdraws: {:?}, {:?}",
            accumulator_version, withdraws
        );
        let (reservations, receivers) = WithdrawReservations::new(accumulator_version, withdraws);
        if let Err(err) = self.withdraw_sender.send(reservations) {
            tracing::error!("Failed to send withdraw reservations: {:?}", err);
        }
        receivers
    }

    /// This function is called whenever a new version of the accumulator root object is committed
    /// in the writeback cache.
    /// It is OK to call this function out of order, as the implementation will handle the out of order calls.
    /// It must be called once for each version of the accumulator root object.
    pub fn settle_balances(&self, settlement: BalanceSettlement) {
        if let Err(err) = self.settlement_sender.send(settlement) {
            tracing::error!("Failed to send balance settlement: {:?}", err);
        }
    }

    async fn process_withdraw_task(
        self: Arc<Self>,
        mut withdraw_receiver: UnboundedReceiver<WithdrawReservations>,
    ) {
        let mut last_scheduled_version = None;
        while let Some(event) = withdraw_receiver.recv().await {
            if let Some(last_scheduled_version) = last_scheduled_version {
                if event.accumulator_version <= last_scheduled_version {
                    // It is possible to receive withdraw reservations for the same accumulator version
                    // multiple times due to the race between consensus and checkpoint execution.
                    // Hence we may receive a version from the past after the version is updated.
                    for (withdraw, sender) in event.withdraws.into_iter().zip(event.senders) {
                        let _ = sender.send(ScheduleResult {
                            tx_digest: withdraw.tx_digest,
                            status: ScheduleStatus::AlreadyScheduled,
                        });
                    }
                    continue;
                }
            }
            last_scheduled_version = Some(event.accumulator_version);
            self.inner.schedule_withdraws(event).await;
        }
    }

    async fn process_settlement_task(
        self: Arc<Self>,
        mut settlement_receiver: UnboundedReceiver<BalanceSettlement>,
        starting_accumulator_version: SequenceNumber,
    ) {
        let mut expected_version = starting_accumulator_version.next();
        let mut pending_settlements = BTreeMap::new();
        while let Some(settlement) = settlement_receiver.recv().await {
            debug!(
                "process_settlement_task received version: {:?}, expected version: {:?}",
                settlement.accumulator_version, expected_version
            );
            pending_settlements.insert(settlement.accumulator_version, settlement);
            while let Some(settlement) = pending_settlements.remove(&expected_version) {
                expected_version = settlement.accumulator_version.next();
                self.inner.settle_balances(settlement).await;
            }
        }
    }
}
