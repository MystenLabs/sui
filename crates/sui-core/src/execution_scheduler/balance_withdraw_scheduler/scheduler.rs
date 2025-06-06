// Copyright (c) Mysten Labs, Inc.Add commentMore actions
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use crate::execution_scheduler::balance_withdraw_scheduler::{
    balance_read::AccountBalanceRead, naive_scheduler::NaiveBalanceWithdrawScheduler,
    BalanceSettlement, ScheduleResult, TxBalanceWithdraw, WithdrawReservations,
};
use mysten_metrics::monitored_mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use sui_types::{base_types::SequenceNumber, digests::TransactionDigest};
use tokio::sync::oneshot;

#[allow(dead_code)]
#[async_trait::async_trait]
pub(crate) trait BalanceWithdrawSchedulerTrait: Send + Sync {
    async fn schedule_withdraws(&self, withdraws: WithdrawReservations);
    async fn settle_balances(&self, settlement: BalanceSettlement);
}

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct BalanceWithdrawScheduler {
    inner: Arc<dyn BalanceWithdrawSchedulerTrait>,
    withdraw_sender: UnboundedSender<WithdrawReservations>,
    settlement_sender: UnboundedSender<BalanceSettlement>,
}

impl BalanceWithdrawScheduler {
    #[allow(dead_code)]
    pub fn new(
        balance_read: Arc<dyn AccountBalanceRead>,
        init_accumulator_version: SequenceNumber,
    ) -> Arc<Self> {
        let inner = NaiveBalanceWithdrawScheduler::new(balance_read, init_accumulator_version);
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
                .process_settlement_task(settlement_receiver, init_accumulator_version),
        );
        scheduler
    }

    #[allow(dead_code)]
    pub fn schedule_withdraws(
        &self,
        accumulator_version: SequenceNumber,
        withdraws: Vec<TxBalanceWithdraw>,
    ) -> BTreeMap<TransactionDigest, oneshot::Receiver<ScheduleResult>> {
        let (reservations, receivers) = WithdrawReservations::new(accumulator_version, withdraws);
        if let Err(err) = self.withdraw_sender.send(reservations) {
            tracing::error!("Failed to send withdraw reservations: {:?}", err);
        }
        receivers
    }

    #[allow(dead_code)]
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
                    for sender in event.senders {
                        let _ = sender.send(ScheduleResult::AlreadyScheduled);
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
        init_accumulator_version: SequenceNumber,
    ) {
        let mut expected_version = init_accumulator_version.next();
        let mut pending_settlements = BTreeMap::new();
        while let Some(settlement) = settlement_receiver.recv().await {
            pending_settlements.insert(settlement.accumulator_version, settlement);
            while let Some(settlement) = pending_settlements.remove(&expected_version) {
                expected_version = settlement.accumulator_version.next();
                self.inner.settle_balances(settlement).await;
            }
        }
    }
}
