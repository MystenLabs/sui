// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    accumulators::{self, AccumulatorSettlementTxBuilder},
    authority::{
        ExecutionEnv, authority_per_epoch_store::AuthorityPerEpochStore,
        epoch_start_configuration::EpochStartConfigTrait,
        shared_object_version_manager::Schedulable,
    },
    execution_cache::TransactionCacheRead,
    execution_scheduler::execution_scheduler_impl::{BarrierDependencyBuilder, ExecutionScheduler},
    execution_scheduler::funds_withdraw_scheduler::FundsSettlement,
};
use futures::stream::{FuturesUnordered, StreamExt};
use mysten_metrics::{monitored_mpsc, spawn_monitored_task};
use parking_lot::Mutex;
use std::sync::Arc;
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID,
    effects::TransactionEffectsAPI,
    executable_transaction::VerifiedExecutableTransaction,
    transaction::{TransactionKey, VerifiedTransaction},
};
use tracing::{debug, error};

struct SettlementWorkItem {
    settlement_key: TransactionKey,
    env: ExecutionEnv,
}

#[derive(Clone)]
struct SettlementQueueSender {
    sender: monitored_mpsc::UnboundedSender<SettlementWorkItem>,
}

impl SettlementQueueSender {
    fn send(&self, item: SettlementWorkItem) {
        if let Err(e) = self.sender.send(item) {
            error!(
                "Failed to send settlement work item: {:?}",
                e.0.settlement_key
            );
        }
    }
}

#[derive(Clone)]
pub(crate) struct SettlementScheduler {
    execution_scheduler: ExecutionScheduler,
    transaction_cache_read: Arc<dyn TransactionCacheRead>,
    settlement_queue_sender: Arc<Mutex<Option<SettlementQueueSender>>>,
}

impl SettlementScheduler {
    pub(crate) fn new(
        execution_scheduler: ExecutionScheduler,
        transaction_cache_read: Arc<dyn TransactionCacheRead>,
    ) -> Self {
        Self {
            execution_scheduler,
            transaction_cache_read,
            settlement_queue_sender: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn enqueue(
        &self,
        certs: Vec<(Schedulable, ExecutionEnv)>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        let mut rest = Vec::with_capacity(certs.len());
        let mut settlement_txns = Vec::new();

        for (schedulable, env) in certs {
            match &schedulable {
                Schedulable::AccumulatorSettlement(_, _) => {
                    settlement_txns.push((schedulable.key(), env));
                }
                _ => {
                    rest.push((schedulable, env));
                }
            }
        }

        self.execution_scheduler.enqueue(rest, epoch_store);
        self.schedule_settlement_transactions(settlement_txns, epoch_store);
    }

    fn schedule_settlement_transactions(
        &self,
        settlement_txns: Vec<(TransactionKey, ExecutionEnv)>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        if settlement_txns.is_empty() {
            return;
        }

        if epoch_store
            .protocol_config()
            .split_checkpoints_in_consensus_handler()
        {
            let queue = self.get_or_start_queue(epoch_store);
            for (settlement_key, env) in settlement_txns {
                queue.send(SettlementWorkItem {
                    settlement_key,
                    env,
                });
            }
        } else {
            let execution_scheduler = self.execution_scheduler.clone();
            let epoch_store = epoch_store.clone();
            spawn_monitored_task!(epoch_store.clone().within_alive_epoch(async move {
                let mut futures: FuturesUnordered<_> = settlement_txns
                    .into_iter()
                    .map(|(key, env)| {
                        let epoch_store = epoch_store.clone();
                        async move {
                            (
                                key,
                                epoch_store.wait_for_settlement_transactions(key).await,
                                env,
                            )
                        }
                    })
                    .collect();

                while let Some((settlement_key, txns, env)) = futures.next().await {
                    let mut barrier_deps = BarrierDependencyBuilder::new();
                    let txns = txns
                        .into_iter()
                        .map(|tx| {
                            let deps = barrier_deps.process_tx(*tx.digest(), tx.transaction_data());
                            let env = env.clone().with_barrier_dependencies(deps);
                            (tx, env)
                        })
                        .collect::<Vec<_>>();

                    execution_scheduler.enqueue_transactions(txns, &epoch_store);

                    let execution_scheduler = execution_scheduler.clone();
                    let epoch_store = epoch_store.clone();
                    let env = env.clone();
                    spawn_monitored_task!(epoch_store.clone().within_alive_epoch(async move {
                        let barrier_tx = epoch_store
                            .wait_for_barrier_transaction(settlement_key)
                            .await;
                        let deps = barrier_deps
                            .process_tx(*barrier_tx.digest(), barrier_tx.transaction_data());
                        let env = env.with_barrier_dependencies(deps);
                        execution_scheduler
                            .enqueue_transactions(vec![(barrier_tx, env)], &epoch_store);
                    }));
                }
            }));
        }
    }

    fn get_or_start_queue(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SettlementQueueSender {
        let mut guard = self.settlement_queue_sender.lock();
        if let Some(sender) = guard.as_ref() {
            return sender.clone();
        }

        let (sender, recv) = monitored_mpsc::unbounded_channel("settlement_queue");
        let queue_sender = SettlementQueueSender { sender };
        *guard = Some(queue_sender.clone());

        let scheduler = self.clone();
        let epoch_store = epoch_store.clone();
        spawn_monitored_task!(epoch_store.clone().within_alive_epoch(Self::run_queue(
            recv,
            scheduler,
            epoch_store
        )));

        queue_sender
    }

    async fn run_queue(
        mut recv: monitored_mpsc::UnboundedReceiver<SettlementWorkItem>,
        scheduler: SettlementScheduler,
        epoch_store: Arc<AuthorityPerEpochStore>,
    ) {
        while let Some(item) = recv.recv().await {
            let result = epoch_store
                .within_alive_epoch(scheduler.construct_and_execute_settlement(
                    item.settlement_key,
                    item.env,
                    &epoch_store,
                ))
                .await;
            if result.is_err() {
                debug!("Settlement queue task ended: epoch is no longer alive");
                return;
            }
        }
        debug!("Settlement queue task ended: channel closed");
    }

    async fn construct_and_execute_settlement(
        &self,
        settlement_key: TransactionKey,
        env: ExecutionEnv,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        let batch_info = match epoch_store.take_settlement_batch_info(&settlement_key) {
            Some(info) => info,
            None => {
                debug!("SettlementBatchInfo not found for key {:?}", settlement_key);
                return;
            }
        };

        let digests = match epoch_store
            .notify_read_tx_key_to_digest(&batch_info.tx_keys)
            .await
        {
            Ok(digests) => digests,
            Err(e) => {
                error!("Failed to read tx digests for settlement: {:?}", e);
                return;
            }
        };

        let effects = self
            .transaction_cache_read
            .notify_read_executed_effects(
                "SettlementScheduler::construct_and_execute_settlement",
                &digests,
            )
            .await;

        let epoch = epoch_store.epoch();
        let accumulator_root_obj_initial_shared_version = epoch_store
            .epoch_start_config()
            .accumulator_root_obj_initial_shared_version()
            .expect("accumulator root object must exist");

        let checkpoint_seq = batch_info.checkpoint_seq;

        let builder = AccumulatorSettlementTxBuilder::new(
            Some(self.transaction_cache_read.as_ref()),
            &effects,
            checkpoint_seq,
            batch_info.tx_index_offset,
        );

        let funds_changes = builder.collect_funds_changes();
        let settlement_txns = builder.build_tx(
            epoch_store.protocol_config(),
            epoch,
            accumulator_root_obj_initial_shared_version,
            batch_info.checkpoint_height,
            checkpoint_seq,
        );

        let settlement_txns: Vec<_> = settlement_txns
            .into_iter()
            .map(|tx| {
                VerifiedExecutableTransaction::new_system(
                    VerifiedTransaction::new_system_transaction(tx),
                    epoch,
                )
            })
            .collect();

        let settlement_digests: Vec<_> = settlement_txns.iter().map(|tx| *tx.digest()).collect();

        debug!(
            ?settlement_key,
            "early settlement: constructed settlement transactions"
        );

        let mut barrier_deps = BarrierDependencyBuilder::new();
        let txns = settlement_txns
            .into_iter()
            .map(|tx| {
                let deps = barrier_deps.process_tx(*tx.digest(), tx.transaction_data());
                let env = env.clone().with_barrier_dependencies(deps);
                (tx, env)
            })
            .collect::<Vec<_>>();

        self.execution_scheduler
            .enqueue_transactions(txns, epoch_store);

        let settlement_effects = self
            .transaction_cache_read
            .notify_read_executed_effects(
                "SettlementScheduler::settlement_effects",
                &settlement_digests,
            )
            .await;
        let barrier_tx = accumulators::build_accumulator_barrier_tx(
            epoch,
            accumulator_root_obj_initial_shared_version,
            batch_info.checkpoint_height,
            &settlement_effects,
        );

        let barrier_tx = VerifiedExecutableTransaction::new_system(
            VerifiedTransaction::new_system_transaction(barrier_tx),
            epoch,
        );
        let barrier_digest = *barrier_tx.digest();

        let deps = barrier_deps.process_tx(*barrier_tx.digest(), barrier_tx.transaction_data());
        let env = env.with_barrier_dependencies(deps);
        self.execution_scheduler
            .enqueue_transactions(vec![(barrier_tx, env)], epoch_store);

        let barrier_effects = self
            .transaction_cache_read
            .notify_read_executed_effects("SettlementScheduler::barrier_effects", &[barrier_digest])
            .await;

        let all_settlement_effects: Vec<_> = settlement_effects
            .into_iter()
            .chain(barrier_effects)
            .collect();

        let mut next_accumulator_version = None;
        for fx in all_settlement_effects.iter() {
            assert!(
                fx.status().is_ok(),
                "settlement transaction cannot fail (digest: {:?}) {:#?}",
                fx.transaction_digest(),
                fx
            );
            if let Some(version) = fx
                .mutated()
                .iter()
                .find_map(|(oref, _)| (oref.0 == SUI_ACCUMULATOR_ROOT_OBJECT_ID).then_some(oref.1))
            {
                assert!(
                    next_accumulator_version.is_none(),
                    "Only one settlement transaction should mutate the accumulator root object"
                );
                next_accumulator_version = Some(version);
            }
        }

        let funds_settlement = FundsSettlement {
            next_accumulator_version: next_accumulator_version
                .expect("Accumulator root object should be mutated"),
            funds_changes,
        };

        self.execution_scheduler
            .settle_address_funds(funds_settlement);

        debug!(?settlement_key, "early settlement: completed");
    }
}
