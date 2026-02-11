// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use super::{
    FundsSettlement, FundsWithdrawSchedulerType, ScheduleResult, ScheduleStatus,
    eager_scheduler::EagerFundsWithdrawScheduler, metrics::AddressFundsSchedulerMetrics,
    naive_scheduler::NaiveFundsWithdrawScheduler,
};
use crate::{
    accumulators::funds_read::AccountFundsRead,
    execution_scheduler::funds_withdraw_scheduler::WithdrawReservations,
};
use futures::stream::FuturesUnordered;
use mysten_metrics::monitored_mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use sui_types::{base_types::SequenceNumber, digests::TransactionDigest};
use tokio::sync::oneshot;
use tracing::debug;

pub(crate) trait FundsWithdrawSchedulerTrait: Send + Sync {
    fn schedule_withdraws(
        &self,
        reservations: WithdrawReservations,
    ) -> BTreeMap<TransactionDigest, ScheduleResult>;
    fn settle_funds(&self, settlement: FundsSettlement);
    fn close_epoch(&self);
    fn num_tracked_accounts(&self) -> usize;
    #[cfg(test)]
    fn get_current_accumulator_version(&self) -> SequenceNumber;
}

struct WithdrawEvent {
    pub reservations: WithdrawReservations,
    pub senders: BTreeMap<TransactionDigest, oneshot::Sender<(TransactionDigest, ScheduleStatus)>>,
}

#[derive(Clone)]
pub(crate) struct FundsWithdrawScheduler {
    scheduler: Arc<dyn FundsWithdrawSchedulerTrait>,
    /// Use channels to process withdraws and settlements asynchronously without blocking the caller.
    withdraw_sender: UnboundedSender<WithdrawEvent>,
    settlement_sender: UnboundedSender<FundsSettlement>,
}

impl WithdrawEvent {
    fn new(
        reservations: WithdrawReservations,
    ) -> (
        Self,
        FuturesUnordered<oneshot::Receiver<(TransactionDigest, ScheduleStatus)>>,
    ) {
        let (senders, receivers) = reservations
            .withdraws
            .iter()
            .map(|withdraw| {
                let (sender, receiver) = oneshot::channel();
                ((withdraw.tx_digest, sender), receiver)
            })
            .unzip();
        (
            Self {
                reservations,
                senders,
            },
            receivers,
        )
    }
}

impl FundsWithdrawScheduler {
    pub fn new(
        funds_read: Arc<dyn AccountFundsRead>,
        starting_accumulator_version: SequenceNumber,
        scheduler_type: FundsWithdrawSchedulerType,
        metrics: Arc<AddressFundsSchedulerMetrics>,
    ) -> Self {
        let scheduler: Arc<dyn FundsWithdrawSchedulerTrait> = match scheduler_type {
            FundsWithdrawSchedulerType::Naive => {
                NaiveFundsWithdrawScheduler::new(funds_read, starting_accumulator_version)
            }
            FundsWithdrawSchedulerType::Eager => {
                EagerFundsWithdrawScheduler::new(funds_read, starting_accumulator_version)
            }
        };
        let (withdraw_sender, withdraw_receiver) =
            unbounded_channel("withdraw_scheduler_withdraws");
        let (settlement_sender, settlement_receiver) =
            unbounded_channel("withdraw_scheduler_settlements");
        // Pass only the scheduler to the spawned tasks, not the senders. This ensures that when
        // the FundsWithdrawScheduler is dropped (dropping the senders), the channels close and tasks exit.
        tokio::spawn(Self::process_withdraw_task(
            scheduler.clone(),
            withdraw_receiver,
            metrics.clone(),
        ));
        tokio::spawn(Self::process_settlement_task(
            scheduler.clone(),
            settlement_receiver,
            metrics,
        ));
        Self {
            scheduler,
            withdraw_sender,
            settlement_sender,
        }
    }

    /// This function will be called at most once per consensus commit batch that all reads the same root accumulator version.
    /// If a consensus commit batch does not contain any withdraw reservations, it can skip calling this function.
    /// It must be called sequentially in order to correctly schedule withdraws.
    pub fn schedule_withdraws(
        &self,
        withdraw_reservations: WithdrawReservations,
    ) -> FuturesUnordered<oneshot::Receiver<(TransactionDigest, ScheduleStatus)>> {
        // TODO: Add debug assertion that withdraws are scheduled in order.
        let (event, receivers) = WithdrawEvent::new(withdraw_reservations);
        if let Err(err) = self.withdraw_sender.send(event) {
            tracing::error!("Failed to send withdraw reservations: {:?}", err);
        }
        receivers
    }

    /// This function is called whenever a settlement transaction is executed.
    /// It is only called from checkpoint builder, once for each accumulator version, in order.
    pub fn settle_funds(&self, settlement: FundsSettlement) {
        if let Err(err) = self.settlement_sender.send(settlement) {
            tracing::error!("Failed to send funds settlement: {}", err);
        }
    }

    pub fn close_epoch(&self) {
        debug!("Closing epoch for funds withdraw scheduler");
        self.scheduler.close_epoch();
    }

    async fn process_withdraw_task(
        scheduler: Arc<dyn FundsWithdrawSchedulerTrait>,
        mut withdraw_receiver: UnboundedReceiver<WithdrawEvent>,
        metrics: Arc<AddressFundsSchedulerMetrics>,
    ) {
        while let Some(event) = withdraw_receiver.recv().await {
            let WithdrawEvent {
                reservations,
                mut senders,
            } = event;
            debug!(
                withdraw_accumulator_version =? reservations.accumulator_version.value(),
                "Processing withdraws: {:?}",
                reservations.withdraws,
            );

            let num_txns = reservations.withdraws.len();
            let accumulator_version = reservations.accumulator_version;
            let results = scheduler.schedule_withdraws(reservations);

            metrics
                .highest_scheduled_version
                .set(accumulator_version.value() as i64);
            metrics.total_scheduled.inc_by(num_txns as u64);
            metrics
                .tracked_accounts
                .set(scheduler.num_tracked_accounts() as i64);

            for (tx_digest, result) in results {
                let original_sender = senders.remove(&tx_digest).unwrap();
                match result {
                    ScheduleResult::ScheduleResult(status) => {
                        debug!(?tx_digest, ?status, "Scheduling result");
                        metrics
                            .schedule_result
                            .with_label_values(&[status_label(status)])
                            .inc();
                        let _ = original_sender.send((tx_digest, status));
                    }
                    ScheduleResult::Pending(receiver) => {
                        metrics.pending_schedules.inc();
                        let timer = metrics.pending_schedule_latency.start_timer();
                        let pending_metrics = metrics.clone();
                        tokio::spawn(async move {
                            match receiver.await {
                                Ok(status) => {
                                    debug!(?tx_digest, ?status, "Scheduling result (pending)");
                                    pending_metrics
                                        .schedule_result
                                        .with_label_values(&[status_label(status)])
                                        .inc();
                                    let _ = original_sender.send((tx_digest, status));
                                }
                                Err(_) => {
                                    debug!(?tx_digest, "Failed to receive scheduling result");
                                }
                            }
                            timer.observe_duration();
                            pending_metrics.pending_schedules.dec();
                        });
                    }
                }
            }
        }
        tracing::info!("Funds withdraw receiver closed");
    }

    async fn process_settlement_task(
        scheduler: Arc<dyn FundsWithdrawSchedulerTrait>,
        mut settlement_receiver: UnboundedReceiver<FundsSettlement>,
        metrics: Arc<AddressFundsSchedulerMetrics>,
    ) {
        while let Some(settlement) = settlement_receiver.recv().await {
            debug!(
                next_accumulator_version =? settlement.next_accumulator_version.value(),
                "Settling funds changes: {:?}",
                settlement.funds_changes,
            );
            let next_version = settlement.next_accumulator_version;
            scheduler.settle_funds(settlement);

            metrics
                .highest_settled_version
                .set(next_version.value() as i64);
            metrics.total_settled.inc();
            metrics
                .tracked_accounts
                .set(scheduler.num_tracked_accounts() as i64);
        }
        tracing::info!("Funds settlement receiver closed");
    }
}

fn status_label(status: ScheduleStatus) -> &'static str {
    match status {
        ScheduleStatus::SufficientFunds => "sufficient",
        ScheduleStatus::InsufficientFunds => "insufficient",
        ScheduleStatus::SkipSchedule => "skip",
    }
}
