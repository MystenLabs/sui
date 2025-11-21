// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use crate::{
    accumulators::balance_read::AccountBalanceRead,
    execution_scheduler::balance_withdraw_scheduler::{
        BalanceSettlement, ScheduleResult, ScheduleStatus, TxBalanceWithdraw,
        eager_scheduler::EagerBalanceWithdrawScheduler,
        naive_scheduler::NaiveBalanceWithdrawScheduler,
    },
};
use futures::stream::FuturesUnordered;
use mysten_metrics::monitored_mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use sui_types::base_types::SequenceNumber;
use tokio::sync::oneshot;
use tracing::debug;

#[async_trait::async_trait]
pub(crate) trait BalanceWithdrawSchedulerTrait: Send + Sync {
    async fn schedule_withdraws(&self, withdraws: WithdrawReservations);
    async fn settle_balances(&self, settlement: BalanceSettlement);
    fn close_epoch(&self);
    #[cfg(test)]
    fn get_current_accumulator_version(&self) -> SequenceNumber;
}

pub(crate) struct WithdrawReservations {
    pub accumulator_version: SequenceNumber,
    pub withdraws: Vec<TxBalanceWithdraw>,
    pub senders: Vec<oneshot::Sender<ScheduleResult>>,
}

#[derive(Clone)]
pub(crate) struct BalanceWithdrawScheduler {
    innards: BTreeMap<String, Arc<dyn BalanceWithdrawSchedulerTrait>>,
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

    pub fn notify_skip_schedule(self) {
        debug!(
            "Withdraws at accumulator version {:?} are already settled",
            self.accumulator_version
        );
        for (withdraw, sender) in self.withdraws.into_iter().zip(self.senders) {
            let _ = sender.send(ScheduleResult {
                tx_digest: withdraw.tx_digest,
                status: ScheduleStatus::SkipSchedule,
            });
        }
    }
}

impl BalanceWithdrawScheduler {
    pub fn new(
        balance_read: Arc<dyn AccountBalanceRead>,
        starting_accumulator_version: SequenceNumber,
    ) -> Self {
        // TODO: Currently, scheduling will be as slow as the slowest scheduler (i.e., the naive scheduler).
        // Once we ensure that the eager scheduler is correct, it will become the only scheduler needed here,
        // and we would only run the naive scheduler in tests for correctness verification.
        let innards = BTreeMap::from([
            (
                "naive".to_string(),
                NaiveBalanceWithdrawScheduler::new(
                    balance_read.clone(),
                    starting_accumulator_version,
                ) as Arc<dyn BalanceWithdrawSchedulerTrait>,
            ),
            (
                "eager".to_string(),
                EagerBalanceWithdrawScheduler::new(balance_read, starting_accumulator_version)
                    as Arc<dyn BalanceWithdrawSchedulerTrait>,
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
        accumulator_version: SequenceNumber,
        withdraws: Vec<TxBalanceWithdraw>,
    ) -> FuturesUnordered<oneshot::Receiver<ScheduleResult>> {
        // TODO: Add debug assertion that withdraws are scheduled in order.
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
            tracing::error!("Failed to send balance settlement: {}", err);
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

    async fn process_withdraw_task(
        self,
        mut withdraw_receiver: UnboundedReceiver<WithdrawReservations>,
    ) {
        while let Some(event) = withdraw_receiver.recv().await {
            debug!(
                withdraw_accumulator_version =? event.accumulator_version.value(),
                "Processing withdraws: {:?}",
                event.withdraws,
            );

            let accumulator_version = event.accumulator_version;
            let withdraws = event.withdraws.clone();
            let original_senders = event.senders;

            // Create intermediate channels for each scheduler
            let mut scheduler_receivers = BTreeMap::new();
            for (name, scheduler) in &self.innards {
                let (intermediate_senders, receivers): (Vec<_>, Vec<_>) =
                    withdraws.iter().map(|_| oneshot::channel()).unzip();
                scheduler_receivers.insert(name.clone(), receivers);

                let reservations = WithdrawReservations {
                    accumulator_version,
                    withdraws: withdraws.clone(),
                    senders: intermediate_senders,
                };
                debug!("Scheduling withdraws on scheduler: {}", name);
                scheduler.schedule_withdraws(reservations).await;
            }

            // Collect results from all schedulers
            for (tx_idx, original_sender) in original_senders.into_iter().enumerate() {
                let mut results = Vec::new();
                for (name, receivers) in &mut scheduler_receivers {
                    if let Some(receiver) = receivers.get_mut(tx_idx) {
                        match receiver.await {
                            Ok(result) => {
                                debug!(
                                    scheduler = %name,
                                    tx_digest =? result.tx_digest,
                                    status =? result.status,
                                    "Received scheduling result"
                                );
                                results.push(result);
                            }
                            Err(_) => {
                                tracing::error!(
                                    "Failed to receive result from scheduler: {}",
                                    name
                                );
                            }
                        }
                    }
                }

                // Aggregate results according to the rules
                let final_result = Self::aggregate_results(results);
                let _ = original_sender.send(final_result);
            }
        }
        tracing::info!("Balance withdraw receiver closed");
    }

    fn aggregate_results(results: Vec<ScheduleResult>) -> ScheduleResult {
        assert!(!results.is_empty(), "Must have at least one result");

        let tx_digest = results[0].tx_digest;
        let non_skip_results: Vec<_> = results
            .iter()
            .filter(|r| r.status != ScheduleStatus::SkipSchedule)
            .collect();

        if non_skip_results.is_empty() {
            // All schedulers returned SkipSchedule
            return ScheduleResult {
                tx_digest,
                status: ScheduleStatus::SkipSchedule,
            };
        }

        // Verify all non-skip results agree
        let first_status = non_skip_results[0].status;
        for result in &non_skip_results {
            assert_eq!(
                result.status, first_status,
                "Schedulers returned different results for tx {:?}: expected {:?}, got {:?}",
                tx_digest, first_status, result.status
            );
        }

        ScheduleResult {
            tx_digest,
            status: first_status,
        }
    }

    async fn process_settlement_task(
        self,
        mut settlement_receiver: UnboundedReceiver<BalanceSettlement>,
    ) {
        while let Some(settlement) = settlement_receiver.recv().await {
            debug!(
                next_accumulator_version =? settlement.next_accumulator_version.value(),
                "Settling balance changes: {:?}",
                settlement.balance_changes,
            );
            for (name, scheduler) in &self.innards {
                debug!("Settling balances on scheduler: {}", name);
                scheduler.settle_balances(settlement.clone()).await;
            }
        }
        tracing::info!("Balance settlement receiver closed");
    }
}
