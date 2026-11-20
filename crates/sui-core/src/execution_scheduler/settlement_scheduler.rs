// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityMetrics;
use crate::{
    accumulators::{self, AccumulatorSettlementTxBuilder},
    authority::{
        ExecutionEnv,
        authority_per_epoch_store::AuthorityPerEpochStore,
        shared_object_version_manager::{AssignedVersions, Schedulable},
    },
    checkpoints::{CheckpointStore, causal_order::CausalOrder},
    execution_cache::TransactionCacheRead,
    execution_scheduler::execution_scheduler_impl::{BarrierDependencyBuilder, ExecutionScheduler},
    execution_scheduler::funds_withdraw_scheduler::FundsSettlement,
};
use mysten_metrics::{monitored_mpsc, spawn_monitored_task};
use parking_lot::Mutex;
use std::{collections::HashSet, sync::Arc};
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID,
    base_types::TransactionDigest,
    effects::{TransactionEffects, TransactionEffectsAPI},
    executable_transaction::VerifiedExecutableTransaction,
    transaction::{TransactionDataAPI, TransactionKey, VerifiedTransaction},
};
use tracing::{debug, error};

#[derive(Clone)]
pub struct SettlementBatchInfo {
    pub settlement_key: TransactionKey,
    pub tx_keys: Vec<TransactionKey>,
    pub checkpoint_height: u64,
    pub checkpoint_seq: u64,
    pub assigned_versions: AssignedVersions,
}

struct SettlementWorkItem {
    settlement_key: TransactionKey,
    env: ExecutionEnv,
    batch_info: SettlementBatchInfo,
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
    checkpoint_store: Arc<CheckpointStore>,
    settlement_queue_sender: Arc<Mutex<Option<SettlementQueueSender>>>,
    metrics: Arc<AuthorityMetrics>,
}

impl SettlementScheduler {
    pub(crate) fn new(
        execution_scheduler: ExecutionScheduler,
        transaction_cache_read: Arc<dyn TransactionCacheRead>,
        checkpoint_store: Arc<CheckpointStore>,
        metrics: Arc<AuthorityMetrics>,
    ) -> Self {
        Self {
            execution_scheduler,
            transaction_cache_read,
            checkpoint_store,
            settlement_queue_sender: Arc::new(Mutex::new(None)),
            metrics,
        }
    }

    pub(crate) fn enqueue(
        &self,
        certs: Vec<(Schedulable, ExecutionEnv)>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        assert!(
            certs
                .iter()
                .all(|(s, _)| !matches!(s, Schedulable::AccumulatorSettlement(_, _))),
            "settlement transactions must be scheduled via enqueue_v2"
        );
        self.execution_scheduler.enqueue(certs, epoch_store);
    }

    pub(crate) fn enqueue_v2(
        &self,
        certs: Vec<(Schedulable, ExecutionEnv)>,
        settlement: SettlementBatchInfo,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) {
        self.execution_scheduler.enqueue(certs, epoch_store);
        // Settlement transactions carry no causal index: the execution driver admits
        // them unconditionally (they never block, and running them is what unblocks
        // transactions waiting on the accumulator versions they write), so there is
        // nothing to admit against or retire.
        let env = ExecutionEnv::new().with_assigned_versions(settlement.assigned_versions.clone());
        let queue = self.get_or_start_queue(epoch_store);
        queue.send(SettlementWorkItem {
            settlement_key: settlement.settlement_key,
            env,
            batch_info: settlement,
        });
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
        let mut current_checkpoint_seq: u64 = 0;
        let mut running_tx_offset: u64 = 0;

        while let Some(item) = recv.recv().await {
            if item.batch_info.checkpoint_seq != current_checkpoint_seq {
                current_checkpoint_seq = item.batch_info.checkpoint_seq;
                running_tx_offset = 0;
            }

            let tx_index_offset = running_tx_offset;
            let batch_tx_count = item.batch_info.tx_keys.len() as u64;

            let result = epoch_store
                .within_alive_epoch(scheduler.construct_and_execute_settlement(
                    item.settlement_key,
                    item.env,
                    item.batch_info,
                    tx_index_offset,
                    &epoch_store,
                ))
                .await;
            match result {
                Err(_) => {
                    debug!("Settlement queue task ended: epoch is no longer alive");
                    return;
                }
                Ok(settlement_tx_count) => {
                    running_tx_offset += batch_tx_count + settlement_tx_count as u64;
                }
            }
        }
        debug!("Settlement queue task ended: channel closed");
    }

    async fn construct_and_execute_settlement(
        &self,
        settlement_key: TransactionKey,
        env: ExecutionEnv,
        batch_info: SettlementBatchInfo,
        tx_index_offset: u64,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> usize {
        let digests = self.resolve_tx_digests(&batch_info, epoch_store).await;

        let effects = self
            .transaction_cache_read
            .notify_read_executed_effects(
                "SettlementScheduler::construct_and_execute_settlement",
                &digests,
            )
            .await;

        let ccp_digest = self.extract_consensus_commit_prologue_digest(&digests, &effects);

        let sorted_effects =
            CausalOrder::order_for_checkpoint(effects, ccp_digest, epoch_store.protocol_config());

        let epoch = epoch_store.epoch();
        let accumulator_root_obj_initial_shared_version = epoch_store
            .epoch_start_config()
            .accumulator_root_obj_initial_shared_version()
            .expect("accumulator root object must exist");

        let checkpoint_seq = batch_info.checkpoint_seq;

        let builder = AccumulatorSettlementTxBuilder::new(
            Some(self.transaction_cache_read.as_ref()),
            &sorted_effects,
            checkpoint_seq,
            tx_index_offset,
        );

        self.metrics
            .accumulator_deposits
            .inc_by(builder.num_deposits());
        self.metrics
            .accumulator_withdrawals
            .inc_by(builder.num_withdrawals());

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

        let settlement_tx_count = settlement_txns.len() + 1; // +1 for barrier
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
            .spawn_transaction_scheduling(txns, epoch_store);

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
            .spawn_transaction_scheduling(vec![(barrier_tx, env)], epoch_store);

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

        settlement_tx_count
    }

    async fn resolve_tx_digests(
        &self,
        batch_info: &SettlementBatchInfo,
        epoch_store: &AuthorityPerEpochStore,
    ) -> Vec<TransactionDigest> {
        let missing_randomness_keys: HashSet<_> = batch_info
            .tx_keys
            .iter()
            .copied()
            .filter(|key| {
                matches!(key, TransactionKey::RandomnessRound(_, _))
                    && epoch_store.tx_key_to_digest(key).is_none()
            })
            .collect();
        let digests = epoch_store.notify_read_tx_key_to_digest(&batch_info.tx_keys);
        tokio::pin!(digests);

        if !missing_randomness_keys.is_empty() {
            // A crash can lose randomness key mappings even when the certified transactions and
            // effects survive. Already executed checkpoints will not be scheduled to repair them.
            // Race state sync against normal key delivery so live consensus never depends on a
            // checkpoint arriving, and wait for sync because certification alone does not store data.
            let checkpoint = tokio::select! {
                biased;
                result = &mut digests => return result.expect("db error"),
                checkpoint = self.checkpoint_store.notify_read_synced_checkpoint(batch_info.checkpoint_seq) => checkpoint,
            };
            let contents = self
                .checkpoint_store
                .get_checkpoint_contents(&checkpoint.content_digest)
                .expect("db error")
                .expect("synced checkpoint contents must exist");
            let checkpoint_digests: Vec<_> = contents.iter().map(|tx| tx.transaction).collect();
            let transactions = self
                .transaction_cache_read
                .multi_get_transaction_blocks(&checkpoint_digests);
            for tx in transactions {
                let tx = tx.expect("synced checkpoint transaction must exist");
                if let Some(key) = tx.non_digest_key()
                    && missing_randomness_keys.contains(&key)
                    && epoch_store.tx_key_to_digest(&key).is_none()
                {
                    epoch_store.insert_tx_key(key, *tx.digest());
                }
            }
        }

        // Keep the original registrations alive, including notifications delivered before sync.
        digests.await.expect("db error")
    }

    fn extract_consensus_commit_prologue_digest(
        &self,
        digests: &[TransactionDigest],
        effects: &[TransactionEffects],
    ) -> Option<TransactionDigest> {
        let d = digests.first()?;
        let tx = self
            .transaction_cache_read
            .get_transaction_block(d)
            .expect("Transaction block must exist");
        tx.transaction_data()
            .is_consensus_commit_prologue()
            .then(|| {
                assert_eq!(tx.digest(), effects[0].transaction_digest());
                *d
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::{AuthorityState, test_authority_builder::TestAuthorityBuilder};
    use std::time::Duration;
    use sui_types::{
        base_types::{ExecutionDigests, SequenceNumber},
        committee::Committee,
        crypto::RandomnessRound,
        gas::GasCostSummary,
        message_envelope::Message,
        messages_checkpoint::{
            CertifiedCheckpointSummary, CheckpointContents, CheckpointSummary, VerifiedCheckpoint,
        },
    };

    async fn setup() -> (Arc<AuthorityState>, SettlementScheduler) {
        let state = TestAuthorityBuilder::new().build().await;
        let scheduler = SettlementScheduler::new(
            state.execution_scheduler().as_ref().clone(),
            state.get_transaction_cache_reader().clone(),
            state.checkpoint_store.clone(),
            state.metrics.clone(),
        );
        (state, scheduler)
    }

    fn randomness_tx(round: u64) -> VerifiedTransaction {
        VerifiedTransaction::new_randomness_state_update(
            0,
            RandomnessRound::new(round),
            vec![round as u8],
            SequenceNumber::from_u64(1),
        )
    }

    fn batch(tx_keys: Vec<TransactionKey>) -> SettlementBatchInfo {
        SettlementBatchInfo {
            settlement_key: TransactionKey::AccumulatorSettlement(0, 7),
            tx_keys,
            checkpoint_height: 7,
            checkpoint_seq: 7,
            assigned_versions: AssignedVersions::empty(),
        }
    }

    fn effects(tx: &VerifiedTransaction) -> TransactionEffects {
        let mut effects = TransactionEffects::default();
        *effects.transaction_digest_mut_for_testing() = *tx.digest();
        effects
    }

    fn checkpoint(
        transactions: &[VerifiedTransaction],
    ) -> (VerifiedCheckpoint, CheckpointContents) {
        let contents = CheckpointContents::new_with_digests_only_for_tests(
            transactions
                .iter()
                .map(|tx| ExecutionDigests::new(*tx.digest(), effects(tx).digest())),
        );
        let summary = CheckpointSummary::new(
            &sui_protocol_config::ProtocolConfig::get_for_max_version_UNSAFE(),
            0,
            7,
            transactions.len() as u64,
            &contents,
            None,
            GasCostSummary::default(),
            None,
            0,
            vec![],
            vec![],
        );
        let (committee, keys) = Committee::new_simple_test_committee();
        let certified =
            CertifiedCheckpointSummary::new_from_keypairs_for_testing(summary, &keys, &committee);
        (certified.try_into_verified(&committee).unwrap(), contents)
    }

    fn sync_checkpoint(
        state: &AuthorityState,
        checkpoint: &VerifiedCheckpoint,
        contents: CheckpointContents,
        transactions: &[VerifiedTransaction],
    ) {
        for tx in transactions {
            state
                .get_state_sync_store()
                .insert_transaction_and_effects(tx, &effects(tx));
        }
        state
            .checkpoint_store
            .insert_checkpoint_contents(contents)
            .unwrap();
        state
            .checkpoint_store
            .insert_verified_checkpoint(checkpoint)
            .unwrap();
        state
            .checkpoint_store
            .update_highest_synced_checkpoint(checkpoint)
            .unwrap();
    }

    async fn resolve(
        scheduler: &SettlementScheduler,
        batch: &SettlementBatchInfo,
        state: &AuthorityState,
    ) -> Vec<TransactionDigest> {
        tokio::time::timeout(
            Duration::from_secs(2),
            scheduler.resolve_tx_digests(batch, &state.epoch_store_for_testing()),
        )
        .await
        .expect("settlement transaction keys should resolve")
    }

    #[tokio::test]
    async fn resolve_tx_digests_recovers_requested_randomness_keys_in_batch_order() {
        let (state, scheduler) = setup().await;
        let txs = [randomness_tx(1), randomness_tx(2), randomness_tx(3)];
        let (checkpoint, contents) = checkpoint(&txs);
        sync_checkpoint(&state, &checkpoint, contents, &txs);
        let digest = TransactionDigest::random();
        let batch = batch(vec![
            txs[1].non_digest_key().unwrap(),
            TransactionKey::Digest(digest),
            txs[0].non_digest_key().unwrap(),
        ]);

        assert_eq!(
            resolve(&scheduler, &batch, &state).await,
            vec![*txs[1].digest(), digest, *txs[0].digest()]
        );
        let epoch_store = state.epoch_store_for_testing();
        assert_eq!(
            epoch_store.tx_key_to_digest(&txs[0].non_digest_key().unwrap()),
            Some(*txs[0].digest())
        );
        assert_eq!(
            epoch_store.tx_key_to_digest(&txs[2].non_digest_key().unwrap()),
            None
        );
    }

    #[tokio::test]
    async fn resolve_tx_digests_preserves_existing_mapping() {
        let (state, scheduler) = setup().await;
        let txs = [randomness_tx(1), randomness_tx(2)];
        let (checkpoint, contents) = checkpoint(&txs);
        sync_checkpoint(&state, &checkpoint, contents, &txs);
        let existing_digest = TransactionDigest::random();
        let existing_key = txs[0].non_digest_key().unwrap();
        state
            .epoch_store_for_testing()
            .insert_tx_key(existing_key, existing_digest);
        let batch = batch(txs.iter().map(|tx| tx.non_digest_key().unwrap()).collect());

        assert_eq!(
            resolve(&scheduler, &batch, &state).await,
            vec![existing_digest, *txs[1].digest()]
        );
        assert_eq!(
            state
                .epoch_store_for_testing()
                .tx_key_to_digest(&existing_key),
            Some(existing_digest)
        );
    }

    #[tokio::test]
    async fn resolve_tx_digests_waits_for_later_checkpoint_sync() {
        let (state, scheduler) = setup().await;
        let txs = [randomness_tx(1)];
        let (checkpoint, contents) = checkpoint(&txs);
        let batch = batch(vec![txs[0].non_digest_key().unwrap()]);
        let epoch_store = state.epoch_store_for_testing().clone();
        let resolution = scheduler.resolve_tx_digests(&batch, &epoch_store);
        tokio::pin!(resolution);
        assert!(futures::poll!(&mut resolution).is_pending());

        // Certification can arrive before state sync has stored the contents and transactions.
        state
            .checkpoint_store
            .insert_verified_checkpoint(&checkpoint)
            .unwrap();
        assert!(futures::poll!(&mut resolution).is_pending());
        sync_checkpoint(&state, &checkpoint, contents, &txs);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), resolution)
                .await
                .expect("later checkpoint sync should resolve the missing key"),
            vec![*txs[0].digest()]
        );
    }

    #[tokio::test]
    async fn resolve_tx_digests_accepts_ordinary_key_without_checkpoint() {
        let (state, scheduler) = setup().await;
        let key = TransactionKey::ConsensusCommitPrologue(0, 8, 0);
        let batch = batch(vec![key]);
        let epoch_store = state.epoch_store_for_testing().clone();
        let resolution = scheduler.resolve_tx_digests(&batch, &epoch_store);
        tokio::pin!(resolution);
        assert!(futures::poll!(&mut resolution).is_pending());
        let digest = TransactionDigest::random();
        state.epoch_store_for_testing().insert_tx_key(key, digest);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), resolution)
                .await
                .unwrap(),
            vec![digest]
        );
    }

    #[tokio::test]
    async fn resolve_tx_digests_accepts_key_notification_before_checkpoint() {
        let (state, scheduler) = setup().await;
        let key = TransactionKey::RandomnessRound(0, RandomnessRound::new(1));
        let batch = batch(vec![key]);
        let epoch_store = state.epoch_store_for_testing().clone();
        let resolution = scheduler.resolve_tx_digests(&batch, &epoch_store);
        tokio::pin!(resolution);
        assert!(futures::poll!(&mut resolution).is_pending());
        let digest = TransactionDigest::random();
        state.epoch_store_for_testing().insert_tx_key(key, digest);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), resolution)
                .await
                .unwrap(),
            vec![digest]
        );
    }
}
