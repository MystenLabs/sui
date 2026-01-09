// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use super::{
    FundsSettlement, ScheduleResult, ScheduleStatus, eager_scheduler::EagerFundsWithdrawScheduler,
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
    #[cfg(test)]
    fn get_current_accumulator_version(&self) -> SequenceNumber;
}

struct WithdrawEvent {
    pub reservations: WithdrawReservations,
    pub senders: BTreeMap<TransactionDigest, oneshot::Sender<(TransactionDigest, ScheduleStatus)>>,
}

#[derive(Clone)]
pub(crate) struct FundsWithdrawScheduler {
    innards: BTreeMap<String, Arc<dyn FundsWithdrawSchedulerTrait>>,
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
    ) -> Self {
        // TODO: Currently, scheduling will be as slow as the slowest scheduler (i.e., the naive scheduler).
        // Once we ensure that the eager scheduler is correct, it will become the only scheduler needed here,
        // and we would only run the naive scheduler in tests for correctness verification.
        let innards = BTreeMap::from([
            (
                "naive".to_string(),
                NaiveFundsWithdrawScheduler::new(funds_read.clone(), starting_accumulator_version)
                    as Arc<dyn FundsWithdrawSchedulerTrait>,
            ),
            (
                "eager".to_string(),
                EagerFundsWithdrawScheduler::new(funds_read, starting_accumulator_version)
                    as Arc<dyn FundsWithdrawSchedulerTrait>,
            ),
        ]);
        let (withdraw_sender, withdraw_receiver) =
            unbounded_channel("withdraw_scheduler_withdraws");
        let (settlement_sender, settlement_receiver) =
            unbounded_channel("withdraw_scheduler_settlements");
        let scheduler = Self {
            innards,
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
        for (name, scheduler) in &self.innards {
            debug!("Closing epoch for scheduler: {}", name);
            scheduler.close_epoch();
        }
    }

    #[cfg(test)]
    pub fn get_current_accumulator_version(&self) -> SequenceNumber {
        let versions: Vec<_> = self
            .innards
            .iter()
            .map(|(name, scheduler)| (name, scheduler.get_current_accumulator_version()))
            .collect();

        let first_version = versions[0].1;
        for (name, version) in versions.iter().skip(1) {
            assert_eq!(
                *version, first_version,
                "Scheduler '{}' has version {:?}, but expected {:?}",
                name, version, first_version
            );
        }

        first_version
    }

    async fn process_withdraw_task(self, mut withdraw_receiver: UnboundedReceiver<WithdrawEvent>) {
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

            // Create intermediate channels for each scheduler so that we could aggregate the results from all schedulers.
            let mut scheduler_receivers = BTreeMap::new();
            for (name, scheduler) in &self.innards {
                let (mut intermediate_senders, receivers): (BTreeMap<_, _>, BTreeMap<_, _>) =
                    reservations
                        .withdraws
                        .iter()
                        .map(|withdraw| {
                            let (s, r) = oneshot::channel();
                            ((withdraw.tx_digest, s), (withdraw.tx_digest, r))
                        })
                        .unzip();
                for (tx_digest, receiver) in receivers {
                    scheduler_receivers
                        .entry(tx_digest)
                        .or_insert_with(BTreeMap::new)
                        .insert(name.clone(), receiver);
                }
                let results = scheduler.schedule_withdraws(reservations.clone());
                for (tx_digest, result) in results {
                    let intermediate_sender = intermediate_senders.remove(&tx_digest).unwrap();
                    match result {
                        ScheduleResult::ScheduleResult(status) => {
                            intermediate_sender.send((tx_digest, status)).unwrap();
                        }
                        ScheduleResult::Pending(receiver) => {
                            let name = name.clone();
                            tokio::spawn(async move {
                                let Ok(status) = receiver.await else {
                                    debug!(
                                        ?tx_digest,
                                        "Failed to receive result from scheduler: {}", name
                                    );
                                    return;
                                };
                                intermediate_sender.send((tx_digest, status)).unwrap();
                            });
                        }
                    }
                }
            }

            for (tx_digest, receivers) in scheduler_receivers {
                let mut result = None;
                for (name, receiver) in receivers {
                    match receiver.await {
                        Ok((_, status)) => {
                            debug!(
                                scheduler = %name,
                                ?tx_digest,
                                ?status,
                                "Received scheduling result"
                            );
                            if status == ScheduleStatus::SkipSchedule {
                                continue;
                            }
                            if let Some(result) = result {
                                assert_eq!(
                                    result, status,
                                    "Scheduler {} returned different results for tx {:?}: expected {:?}, got {:?}",
                                    name, tx_digest, result, status
                                );
                            } else {
                                result = Some(status);
                            }
                        }
                        Err(_) => {
                            tracing::error!("Failed to receive result from scheduler: {}", name);
                        }
                    }
                }
                let original_sender = senders.remove(&tx_digest).unwrap();
                if let Some(result) = result {
                    let _ = original_sender.send((tx_digest, result));
                } else {
                    let _ = original_sender.send((tx_digest, ScheduleStatus::SkipSchedule));
                }
            }
        }
        tracing::info!("Funds withdraw receiver closed");
    }

    async fn process_settlement_task(
        self,
        mut settlement_receiver: UnboundedReceiver<FundsSettlement>,
    ) {
        while let Some(settlement) = settlement_receiver.recv().await {
            debug!(
                next_accumulator_version =? settlement.next_accumulator_version.value(),
                "Settling funds changes: {:?}",
                settlement.funds_changes,
            );
            for scheduler in self.innards.values() {
                scheduler.settle_funds(settlement.clone());
            }
        }
        tracing::info!("Funds settlement receiver closed");
    }
}
