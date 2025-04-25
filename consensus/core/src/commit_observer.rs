// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use parking_lot::RwLock;
use tokio::time::Instant;
use tracing::{debug, info};

use crate::{
    block::{BlockAPI, VerifiedBlock},
    commit::{load_committed_subdag_from_store, CommitAPI, CommitIndex},
    commit_finalizer::{CommitFinalizer, CommitFinalizerHandle},
    context::Context,
    dag_state::DagState,
    error::ConsensusResult,
    leader_schedule::LeaderSchedule,
    linearizer::Linearizer,
    storage::Store,
    transaction_certifier::TransactionCertifier,
    CommitConsumer, CommittedSubDag,
};

/// Role of CommitObserver
/// - Called by core when try_commit() returns newly committed leaders.
/// - The newly committed leaders are sent to commit observer and then commit observer
///   gets subdags for each leader via the commit interpreter (linearizer)
/// - The committed subdags are sent as consensus output via an unbounded tokio channel.
///
/// There is no flow control on sending output. Consensus backpressure is applied earlier
/// at consensus input level, and on commit sync.
///
/// Commit is persisted in store before the CommittedSubDag is sent to the commit handler.
/// When Sui recovers, it blocks until the commits it knows about are recovered. So consensus
/// must be able to quickly recover the commits it has sent to Sui.
pub(crate) struct CommitObserver {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
    /// Persistent storage for blocks, commits and other consensus data.
    store: Arc<dyn Store>,
    leader_schedule: Arc<LeaderSchedule>,
    /// Component to deterministically collect subdags for committed leaders.
    commit_interpreter: Linearizer,
    /// An unbounded channel to send commits to commit handler.
    commit_finalizer_handle: CommitFinalizerHandle,
}

impl CommitObserver {
    pub(crate) fn new(
        context: Arc<Context>,
        commit_consumer: CommitConsumer,
        dag_state: Arc<RwLock<DagState>>,
        transaction_certifier: TransactionCertifier,
        leader_schedule: Arc<LeaderSchedule>,
    ) -> Self {
        let store = dag_state.read().store();
        let commit_interpreter = Linearizer::new(context.clone(), dag_state.clone());
        let commit_finalizer_handle = CommitFinalizer::start(
            context.clone(),
            transaction_certifier,
            commit_consumer.commit_sender,
        );
        let mut observer = Self {
            context,
            dag_state,
            store,
            leader_schedule,
            commit_interpreter,
            commit_finalizer_handle,
        };

        observer.recover_and_send_commits(commit_consumer.last_processed_commit_index);
        observer
    }

    pub(crate) fn handle_commit(
        &mut self,
        committed_leaders: Vec<(VerifiedBlock, bool)>,
    ) -> ConsensusResult<Vec<CommittedSubDag>> {
        let _s = self
            .context
            .metrics
            .node_metrics
            .scope_processing_time
            .with_label_values(&["CommitObserver::handle_commit"])
            .start_timer();

        let (leader_blocks, direct_commits): (Vec<VerifiedBlock>, Vec<bool>) =
            committed_leaders.into_iter().unzip();

        let mut committed_sub_dags = self.commit_interpreter.handle_commit(leader_blocks);
        self.report_metrics(&committed_sub_dags);

        // Send scores as part of the first sub dag, if the leader schedule has been updated.
        let schedule_updated = self
            .leader_schedule
            .leader_schedule_updated(&self.dag_state);
        if schedule_updated {
            let reputation_scores_desc = self
                .leader_schedule
                .leader_swap_table
                .read()
                .reputation_scores_desc
                .clone();
            committed_sub_dags[0].reputation_scores_desc = reputation_scores_desc;
        }

        for (commit, direct) in committed_sub_dags.iter().zip(direct_commits) {
            tracing::debug!(
                "Sending commit {} leader {} to execution.",
                commit.commit_ref,
                commit.leader
            );
            tracing::trace!("Committed subdag: {:#?}", commit);
            // Failures in sender.send() are assumed to be permanent
            self.commit_finalizer_handle
                .send((commit.clone(), direct))?;
        }

        self.dag_state
            .write()
            .add_scoring_subdags(committed_sub_dags.clone());

        Ok(committed_sub_dags)
    }

    fn recover_and_send_commits(&mut self, last_processed_commit_index: CommitIndex) {
        let now = Instant::now();
        // TODO: remove this check, to allow consensus to regenerate commits?
        let last_commit = self
            .store
            .read_last_commit()
            .expect("Reading the last commit should not fail");

        if let Some(last_commit) = &last_commit {
            let last_commit_index = last_commit.index();

            assert!(last_commit_index >= last_processed_commit_index);
            if last_commit_index == last_processed_commit_index {
                debug!("Nothing to recover for commit observer as commit index {last_commit_index} = {last_processed_commit_index} last processed index");
                return;
            }
        };

        // We should not send the last processed commit again, so last_processed_commit_index+1
        let unsent_commits = self
            .store
            .scan_commits(((last_processed_commit_index + 1)..=CommitIndex::MAX).into())
            .expect("Scanning commits should not fail");

        info!("Recovering commit observer after index {last_processed_commit_index} with last commit {} and {} unsent commits", last_commit.map(|c|c.index()).unwrap_or_default(), unsent_commits.len());

        // Resend all the committed subdags to the consensus output channel
        // for all the commits above the last processed index.
        let mut last_sent_commit_index = last_processed_commit_index;
        let num_unsent_commits = unsent_commits.len();
        for (index, commit) in unsent_commits.into_iter().enumerate() {
            // Commit index must be continuous.
            assert_eq!(commit.index(), last_sent_commit_index + 1);

            // On recovery leader schedule will be updated with the current scores
            // and the scores will be passed along with the last commit sent to
            // sui so that the current scores are available for submission.
            let reputation_scores = if index == num_unsent_commits - 1 {
                self.leader_schedule
                    .leader_swap_table
                    .read()
                    .reputation_scores_desc
                    .clone()
            } else {
                vec![]
            };

            info!("Sending commit {} during recovery", commit.index());
            let committed_sub_dag =
                load_committed_subdag_from_store(self.store.as_ref(), commit, reputation_scores);
            self.commit_finalizer_handle
                .send((committed_sub_dag, false))
                .unwrap();

            last_sent_commit_index += 1;
        }

        info!(
            "Commit observer recovery completed, took {:?}",
            now.elapsed()
        );
    }

    fn report_metrics(&self, committed: &[CommittedSubDag]) {
        let metrics = &self.context.metrics.node_metrics;
        let utc_now = self.context.clock.timestamp_utc_ms();

        for commit in committed {
            info!(
                "Consensus commit {} with leader {} has {} blocks",
                commit.commit_ref,
                commit.leader,
                commit.blocks.len()
            );

            metrics
                .last_committed_leader_round
                .set(commit.leader.round as i64);
            metrics
                .last_commit_index
                .set(commit.commit_ref.index as i64);
            metrics
                .blocks_per_commit_count
                .observe(commit.blocks.len() as f64);

            for block in &commit.blocks {
                let latency_ms = utc_now
                    .checked_sub(block.timestamp_ms())
                    .unwrap_or_default();
                metrics
                    .block_commit_latency
                    .observe(Duration::from_millis(latency_ms).as_secs_f64());
            }
        }

        self.context
            .metrics
            .node_metrics
            .sub_dags_per_commit_count
            .observe(committed.len() as f64);
    }
}

#[cfg(test)]
mod tests {
    use consensus_config::AuthorityIndex;
    use mysten_metrics::monitored_mpsc::{unbounded_channel, UnboundedReceiver};
    use parking_lot::RwLock;
    use rstest::rstest;
    use tokio::time::timeout;

    use super::*;
    use crate::{
        block::BlockRef, context::Context, dag_state::DagState,
        linearizer::median_timestamp_by_stake, storage::mem_store::MemStore,
        test_dag_builder::DagBuilder,
    };

    #[rstest]
    #[tokio::test]
    async fn test_handle_commit(#[values(true, false)] consensus_median_timestamp: bool) {
        use crate::leader_schedule::LeaderSwapTable;

        telemetry_subscribers::init_for_testing();
        let num_authorities = 4;
        let (mut context, _keys) = Context::new_for_test(num_authorities);
        context
            .protocol_config
            .set_consensus_median_based_commit_timestamp_for_testing(consensus_median_timestamp);

        let context = Arc::new(context);

        let mem_store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            mem_store.clone(),
        )));
        let last_processed_commit_index = 0;
        let (commit_consumer, mut commit_receiver, _transaction_receiver) =
            CommitConsumer::new(last_processed_commit_index);
        let (blocks_sender, _blocks_receiver) = unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        const NUM_OF_COMMITS_PER_SCHEDULE: u64 = 5;
        let leader_schedule = Arc::new(
            LeaderSchedule::new(context.clone(), LeaderSwapTable::default())
                .with_num_commits_per_schedule(NUM_OF_COMMITS_PER_SCHEDULE),
        );

        let mut observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule.clone(),
        );

        // Populate fully connected test blocks for round 0 ~ 10, authorities 0 ~ 3.
        let num_rounds = 10;
        let mut builder = DagBuilder::new(context.clone());
        builder
            .layers(1..=num_rounds)
            .build()
            .persist_layers(dag_state.clone());
        transaction_certifier.add_voted_blocks(
            builder
                .all_blocks()
                .iter()
                .map(|b| (b.clone(), vec![]))
                .collect(),
        );

        let leaders = builder
            .leader_blocks(1..=num_rounds)
            .into_iter()
            .map(Option::unwrap)
            .collect::<Vec<_>>();

        // Commit first 5 leaders.
        let mut commits = observer
            .handle_commit(leaders[0..5].iter().map(|b| (b.clone(), true)).collect())
            .unwrap();

        // Trigger a leader schedule update.
        leader_schedule.update_leader_schedule_v2(&dag_state);

        // Commit the next 5 leaders.
        commits.extend(
            observer
                .handle_commit(leaders[5..].iter().map(|b| (b.clone(), true)).collect())
                .unwrap(),
        );

        // Check commits are returned by CommitObserver::handle_commit is accurate
        let mut expected_stored_refs: Vec<BlockRef> = vec![];
        for (idx, subdag) in commits.iter().enumerate() {
            tracing::info!("{subdag:?}");
            assert_eq!(subdag.leader, leaders[idx].reference());

            // 5th subdag should contain the updated scores.
            if idx == 5 {
                let scores = vec![
                    (AuthorityIndex::new_for_test(1), 9),
                    (AuthorityIndex::new_for_test(3), 9),
                    (AuthorityIndex::new_for_test(0), 9),
                    (AuthorityIndex::new_for_test(2), 9),
                ];
                assert_eq!(subdag.reputation_scores_desc, scores);
            } else {
                assert!(subdag.reputation_scores_desc.is_empty());
            }

            let expected_ts = if consensus_median_timestamp {
                let block_refs = leaders[idx]
                    .ancestors()
                    .iter()
                    .filter(|block_ref| block_ref.round == leaders[idx].round() - 1)
                    .cloned()
                    .collect::<Vec<_>>();
                let blocks = dag_state
                    .read()
                    .get_blocks(&block_refs)
                    .into_iter()
                    .map(|block_opt| block_opt.expect("We should have all blocks in dag state."));
                median_timestamp_by_stake(&context, blocks).unwrap()
            } else {
                leaders[idx].timestamp_ms()
            };

            let expected_ts = if idx == 0 {
                expected_ts
            } else {
                expected_ts.max(commits[idx - 1].timestamp_ms)
            };

            assert_eq!(expected_ts, subdag.timestamp_ms);

            if idx == 0 {
                // First subdag includes the leader block plus all ancestor blocks
                // of the leader minus the genesis round blocks
                assert_eq!(subdag.blocks.len(), 1);
            } else {
                // Every subdag after will be missing the leader block from the previous
                // committed subdag
                assert_eq!(subdag.blocks.len(), num_authorities);
            }
            for block in subdag.blocks.iter() {
                expected_stored_refs.push(block.reference());
                assert!(block.round() <= leaders[idx].round());
            }
            assert_eq!(subdag.commit_ref.index, idx as CommitIndex + 1);
        }

        // Check commits sent over consensus output channel is accurate
        let mut processed_subdag_index = 0;
        while let Ok(Some(subdag)) = timeout(Duration::from_secs(1), commit_receiver.recv()).await {
            assert_eq!(subdag, commits[processed_subdag_index]);
            processed_subdag_index = subdag.commit_ref.index as usize;
            if processed_subdag_index == leaders.len() {
                break;
            }
        }
        assert_eq!(processed_subdag_index, leaders.len());

        verify_channel_empty(&mut commit_receiver).await;

        // Check commits have been persisted to storage
        let last_commit = mem_store.read_last_commit().unwrap().unwrap();
        assert_eq!(
            last_commit.index(),
            commits.last().unwrap().commit_ref.index
        );
        let all_stored_commits = mem_store
            .scan_commits((0..=CommitIndex::MAX).into())
            .unwrap();
        assert_eq!(all_stored_commits.len(), leaders.len());
        let blocks_existence = mem_store.contains_blocks(&expected_stored_refs).unwrap();
        assert!(blocks_existence.iter().all(|exists| *exists));
    }

    #[tokio::test]
    async fn test_recover_and_send_commits() {
        telemetry_subscribers::init_for_testing();
        let num_authorities = 4;
        let context = Arc::new(Context::new_for_test(num_authorities).0);
        let mem_store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            mem_store.clone(),
        )));
        let (blocks_sender, _blocks_receiver) = unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        let last_processed_commit_index = 0;
        let (commit_consumer, mut commit_receiver, _transaction_receiver) =
            CommitConsumer::new(last_processed_commit_index);
        let leader_schedule = Arc::new(LeaderSchedule::from_store(
            context.clone(),
            dag_state.clone(),
        ));

        let mut observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule.clone(),
        );

        // Populate fully connected test blocks for round 0 ~ 10, authorities 0 ~ 3.
        let num_rounds = 10;
        let mut builder = DagBuilder::new(context.clone());
        builder
            .layers(1..=num_rounds)
            .build()
            .persist_layers(dag_state.clone());
        transaction_certifier.add_voted_blocks(
            builder
                .all_blocks()
                .iter()
                .map(|b| (b.clone(), vec![]))
                .collect(),
        );

        let leaders = builder
            .leader_blocks(1..=num_rounds)
            .into_iter()
            .map(Option::unwrap)
            .collect::<Vec<_>>();

        // Commit first batch of leaders (2) and "receive" the subdags as the
        // consumer of the consensus output channel.
        let expected_last_processed_index: usize = 2;
        let mut commits = observer
            .handle_commit(
                leaders
                    .iter()
                    .take(expected_last_processed_index)
                    .map(|b| (b.clone(), true))
                    .collect::<Vec<_>>(),
            )
            .unwrap();

        // Check commits sent over consensus output channel is accurate
        let mut processed_subdag_index = 0;
        while let Ok(Some(subdag)) = timeout(Duration::from_secs(1), commit_receiver.recv()).await {
            tracing::info!("Processed {subdag}");
            assert_eq!(subdag, commits[processed_subdag_index]);
            assert_eq!(subdag.reputation_scores_desc, vec![]);
            processed_subdag_index = subdag.commit_ref.index as usize;
            if processed_subdag_index == expected_last_processed_index {
                break;
            }
        }
        assert_eq!(processed_subdag_index, expected_last_processed_index);

        verify_channel_empty(&mut commit_receiver).await;

        // Check last stored commit is correct
        let last_commit = mem_store.read_last_commit().unwrap().unwrap();
        assert_eq!(
            last_commit.index(),
            expected_last_processed_index as CommitIndex
        );

        // Handle next batch of leaders (1), these will be sent by consensus but not
        // "processed" by consensus output channel. Simulating something happened on
        // the consumer side where the commits were not persisted.
        commits.append(
            &mut observer
                .handle_commit(
                    leaders
                        .iter()
                        .skip(expected_last_processed_index)
                        .map(|b| (b.clone(), true))
                        .collect::<Vec<_>>(),
                )
                .unwrap(),
        );

        let expected_last_sent_index = num_rounds as usize;
        while let Ok(Some(subdag)) = timeout(Duration::from_secs(1), commit_receiver.recv()).await {
            tracing::info!("{subdag} was sent but not processed by consumer");
            assert_eq!(subdag, commits[processed_subdag_index]);
            assert_eq!(subdag.reputation_scores_desc, vec![]);
            processed_subdag_index = subdag.commit_ref.index as usize;
            if processed_subdag_index == expected_last_sent_index {
                break;
            }
        }
        assert_eq!(processed_subdag_index, expected_last_sent_index);

        verify_channel_empty(&mut commit_receiver).await;

        // Check last stored commit is correct. We should persist the last commit
        // that was sent over the channel regardless of how the consumer handled
        // the commit on their end.
        let last_commit = mem_store.read_last_commit().unwrap().unwrap();
        assert_eq!(last_commit.index(), expected_last_sent_index as CommitIndex);

        // Re-create commit observer starting from index 2 which represents the
        // last processed index from the consumer over consensus output channel
        let (commit_consumer, mut commit_receiver, _transaction_receiver) =
            CommitConsumer::new(expected_last_processed_index as CommitIndex);
        let _observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule,
        );

        // Check commits sent over consensus output channel is accurate starting
        // from last processed index of 2 and finishing at last sent index of 3.
        processed_subdag_index = expected_last_processed_index;
        while let Ok(Some(subdag)) = timeout(Duration::from_secs(1), commit_receiver.recv()).await {
            tracing::info!("Processed {subdag} on resubmission");
            assert_eq!(subdag, commits[processed_subdag_index]);
            assert_eq!(subdag.reputation_scores_desc, vec![]);
            processed_subdag_index = subdag.commit_ref.index as usize;
            if processed_subdag_index == expected_last_sent_index {
                break;
            }
        }
        assert_eq!(processed_subdag_index, expected_last_sent_index);

        verify_channel_empty(&mut commit_receiver).await;
    }

    #[tokio::test]
    async fn test_send_no_missing_commits() {
        telemetry_subscribers::init_for_testing();
        let num_authorities = 4;
        let context = Arc::new(Context::new_for_test(num_authorities).0);
        let mem_store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            mem_store.clone(),
        )));
        let last_processed_commit_index = 0;
        let (commit_consumer, mut commit_receiver, _transaction_receiver) =
            CommitConsumer::new(last_processed_commit_index);
        let (blocks_sender, _blocks_receiver) = unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        let leader_schedule = Arc::new(LeaderSchedule::from_store(
            context.clone(),
            dag_state.clone(),
        ));

        let mut observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule.clone(),
        );

        // Populate fully connected test blocks for round 0 ~ 10, authorities 0 ~ 3.
        let num_rounds = 10;
        let mut builder = DagBuilder::new(context.clone());
        builder
            .layers(1..=num_rounds)
            .build()
            .persist_layers(dag_state.clone());
        transaction_certifier.add_voted_blocks(
            builder
                .all_blocks()
                .iter()
                .map(|b| (b.clone(), vec![]))
                .collect(),
        );

        let leaders = builder
            .leader_blocks(1..=num_rounds)
            .into_iter()
            .map(Option::unwrap)
            .collect::<Vec<_>>();

        // Commit all of the leaders and "receive" the subdags as the consumer of
        // the consensus output channel.
        let expected_last_processed_index: usize = 10;
        let commits = observer
            .handle_commit(leaders.iter().map(|b| (b.clone(), true)).collect())
            .unwrap();

        // Check commits sent over consensus output channel is accurate
        let mut processed_subdag_index = 0;
        while let Ok(Some(subdag)) = timeout(Duration::from_secs(1), commit_receiver.recv()).await {
            tracing::info!("Processed {subdag}");
            assert_eq!(subdag, commits[processed_subdag_index]);
            assert_eq!(subdag.reputation_scores_desc, vec![]);
            processed_subdag_index = subdag.commit_ref.index as usize;
            if processed_subdag_index == expected_last_processed_index {
                break;
            }
        }
        assert_eq!(processed_subdag_index, expected_last_processed_index);

        verify_channel_empty(&mut commit_receiver).await;

        // Check last stored commit is correct
        let last_commit = mem_store.read_last_commit().unwrap().unwrap();
        assert_eq!(
            last_commit.index(),
            expected_last_processed_index as CommitIndex
        );

        // Re-create commit observer starting from index 3 which represents the
        // last processed index from the consumer over consensus output channel
        let (commit_consumer, mut commit_receiver, _transaction_receiver) =
            CommitConsumer::new(expected_last_processed_index as CommitIndex);
        let _observer = CommitObserver::new(
            context.clone(),
            commit_consumer,
            dag_state.clone(),
            transaction_certifier.clone(),
            leader_schedule,
        );

        // No commits should be resubmitted as consensus store's last commit index
        // is equal to last processed index by consumer
        verify_channel_empty(&mut commit_receiver).await;
    }

    /// After receiving all expected subdags, ensure channel is empty
    async fn verify_channel_empty(receiver: &mut UnboundedReceiver<CommittedSubDag>) {
        if let Ok(Some(_)) = timeout(Duration::from_secs(1), receiver.recv()).await {
            panic!("Expected the consensus output channel to be empty, but found more subdags.")
        }
    }
}
