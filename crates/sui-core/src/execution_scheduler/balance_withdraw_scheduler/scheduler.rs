// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::execution_scheduler::balance_withdraw_scheduler::{
    balance_read::AccountBalanceRead, naive_scheduler::NaiveBalanceWithdrawScheduler,
    BalanceSettlement, ScheduleResult, TxBalanceWithdraw,
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
    ) -> Self {
        let inner = NaiveBalanceWithdrawScheduler::new(balance_read, starting_accumulator_version);
        let (withdraw_sender, withdraw_receiver) =
            unbounded_channel("withdraw_scheduler_withdraws");
        let (settlement_sender, settlement_receiver) =
            unbounded_channel("withdraw_scheduler_settlements");
        let scheduler = Self {
            inner,
            withdraw_sender,
            settlement_sender,
        };
        tokio::spawn(scheduler.clone().process_withdraw_task(withdraw_receiver));
        tokio::spawn(
            scheduler
                .clone()
                .process_settlement_task(settlement_receiver),
        );
        scheduler
    }

    /// This function will be called at most once per consensus commit batch that all reads the same root accumulator version.
    /// If a consensus commit batch does not contain any withdraw reservations, it can skip calling this function.
    /// It must be called sequentially in order to correctly schedule withdraws.
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

    /// This function is called whenever a settlement transaction is executed.
    /// It is only called from checkpoint builder, once for each accumulator version, in order.
    pub fn settle_balances(&self, settlement: BalanceSettlement) {
        if let Err(err) = self.settlement_sender.send(settlement) {
            tracing::error!("Failed to send balance settlement: {:?}", err);
        }
    }

    async fn process_withdraw_task(
        self,
        mut withdraw_receiver: UnboundedReceiver<WithdrawReservations>,
    ) {
        while let Some(event) = withdraw_receiver.recv().await {
            self.inner.schedule_withdraws(event).await;
        }
        tracing::info!("Balance withdraw receiver closed");
    }

    async fn process_settlement_task(
        self,
        mut settlement_receiver: UnboundedReceiver<BalanceSettlement>,
    ) {
        while let Some(settlement) = settlement_receiver.recv().await {
            self.inner.settle_balances(settlement).await;
        }
        tracing::info!("Balance settlement receiver closed");
    }
}
