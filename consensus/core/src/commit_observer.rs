// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::{
    block::{timestamp_utc_ms, BlockAPI, VerifiedBlock},
    commit::CommitIndex,
    context::Context,
    linearizer::{CommittedSubDag, Linearizer},
    storage::Store,
};

/// Role of CommitObserver
/// - Called by core when block is created via try_new_block or force_new_block
/// and try_commit returns newly comitted leaders
/// - The newly committed leaders are sent to commit observer and then commit observer
/// gets subdags for each leader via the commit interpreter (linearizer)
/// - The committed subdags are sent as consensus output via an unbounded tokio channel.
/// No back pressure mechanism is needed as backpressure is handled as input into
/// consenus.
/// - Last commit index is persisted in store as soon as the subdag is sent to
/// output channel.
/// - When CommitObserver is initialized a last received commit index can be used
/// to ensure any missing commits are re-sent.

#[allow(unused)]
pub(crate) struct CommitObserver {
    context: Arc<Context>,
    /// Component to deterministically collect subdags for committed leaders.
    commit_interpreter: Linearizer,
    /// An unbounded channel to send committed sub-dags to the consumer of consensus output.
    sender: tokio::sync::mpsc::UnboundedSender<CommittedSubDag>,
    /// Persistent storage for blocks, commits and other consensus data.
    store: Arc<dyn Store>,
}

#[allow(unused)]
impl CommitObserver {
    pub(crate) fn new(
        context: Arc<Context>,
        commit_interpreter: Linearizer,
        sender: tokio::sync::mpsc::UnboundedSender<CommittedSubDag>,
        // Last CommitIndex that has been successfully received by the output channel.
        // First commit in the replayed sequence will have index last_received_index + 1.
        // Set to 0 to replay from the start (as normal sequence starts at index = 1).
        last_received_index: CommitIndex,
        store: Arc<dyn Store>,
    ) -> Self {
        let mut observer = Self {
            context,
            commit_interpreter,
            sender,
            store,
        };

        observer.send_missing_commits(last_received_index);
        observer
    }

    pub(crate) fn handle_commit(
        &mut self,
        committed_leaders: Vec<VerifiedBlock>,
    ) -> Vec<CommittedSubDag> {
        let commits = self.commit_interpreter.handle_commit(committed_leaders);
        let mut sent_sub_dags = vec![];
        let mut sent_commit_data = vec![];

        for (commit_data, committed_sub_dag) in commits.into_iter() {
            if let Err(err) = self.sender.send(committed_sub_dag.clone()) {
                tracing::error!(
                    "Failed to send committed sub-dag, probably due to shutdown: {err:?}"
                );
                break;
            }
            sent_sub_dags.push(committed_sub_dag);
            sent_commit_data.push(commit_data);
        }

        // Persist sent commits to store
        self.store
            .write(
                sent_sub_dags
                    .iter()
                    .flat_map(|subdag| subdag.blocks.clone())
                    .collect::<Vec<_>>(),
                sent_commit_data,
            )
            .expect("Writing commits to store should not fail");

        self.report_metrics(&sent_sub_dags);
        sent_sub_dags
    }

    fn send_missing_commits(&mut self, last_received_index: CommitIndex) {
        let last_commit = self
            .store
            .read_last_commit()
            .expect("Reading the last commit should not fail");

        if let Some(last_commit) = last_commit {
            let last_commit_index = last_commit.index;

            assert!(last_commit_index >= last_received_index);
            if last_commit_index == last_received_index {
                return;
            }
        };

        // We should not send the last received commit again, so last_received_index++
        let unsent_commits = self
            .store
            .scan_commits(last_received_index + 1)
            .expect("Scanning commits should not fail");

        for commit in unsent_commits {
            // Resend all the committed subdags to the consensus output channel
            // for all the commits above the last received index.
            assert!(commit.index > last_received_index);
            let committed_subdag =
                CommittedSubDag::new_from_commit_data(commit, self.store.clone());
            if let Err(err) = self.sender.send(committed_subdag) {
                tracing::error!(
                    "Failed to send committed sub-dag, probably due to shutdown: {:?}",
                    err
                );
            }
        }
    }

    fn report_metrics(&self, committed: &[CommittedSubDag]) {
        let utc_now = timestamp_utc_ms();
        let mut total = 0;
        for block in committed.iter().flat_map(|dag| &dag.blocks) {
            let latency_ms = utc_now
                .checked_sub(block.timestamp_ms())
                .unwrap_or_default();

            total += 1;
            self.context
                .metrics
                .node_metrics
                .block_commit_latency
                .observe(latency_ms as f64);
        }
        self.context
            .metrics
            .node_metrics
            .blocks_per_commit_count
            .observe(total as f64);
        self.context
            .metrics
            .node_metrics
            .sub_dags_per_commit_count
            .observe(committed.len() as f64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use consensus_config::AuthorityIndex;
    use parking_lot::RwLock;
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    use crate::{
        block::{BlockRef, BlockTimestampMs, TestBlock},
        commit::DEFAULT_WAVE_LENGTH,
        context::Context,
        dag_state::DagState,
        leader_schedule::LeaderSchedule,
        storage::mem_store::MemStore,
    };

    #[test]
    fn test_handle_commit() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);
        let mem_store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            mem_store.clone(),
        )));
        let linearizer = Linearizer::new(dag_state.clone());
        let leader_schedule = LeaderSchedule::new(context.clone());
        let last_received_index = 0;
        #[allow(clippy::disallowed_methods)] // allow unbounded_channel()
        let (sender, mut receiver) = unbounded_channel();

        let mut observer = CommitObserver::new(
            context.clone(),
            linearizer,
            sender,
            last_received_index,
            mem_store.clone(),
        );

        // Populate fully connected test blocks for round 0 ~ 10, authorities 0 ~ 3.
        let num_rounds = 10;
        let num_authorities = 4;
        let leaders = setup_test(
            context.clone(),
            dag_state.clone(),
            leader_schedule,
            num_rounds,
            num_authorities,
        );

        let commits = observer.handle_commit(leaders.clone());

        // Check commits are returned by CommitObserver::handle_commit is accurate
        let mut expected_stored_refs: Vec<BlockRef> = vec![];
        for (idx, subdag) in commits.iter().enumerate() {
            tracing::info!("{subdag:?}");
            assert_eq!(subdag.leader, leaders[idx].reference());
            assert_eq!(subdag.timestamp_ms, leaders[idx].timestamp_ms());
            if idx == 0 {
                // First subdag includes the leader block plus all ancestor blocks
                // of the leader minus the genesis round blocks
                assert_eq!(
                    subdag.blocks.len(),
                    (num_authorities * (DEFAULT_WAVE_LENGTH - 1)) as usize + 1
                );
            } else {
                // Every subdag after will be missing the leader block from the previous
                // committed subdag
                assert_eq!(
                    subdag.blocks.len(),
                    (num_authorities * DEFAULT_WAVE_LENGTH) as usize
                );
            }
            for block in subdag.blocks.iter() {
                expected_stored_refs.push(block.reference());
                assert!(block.round() <= leaders[idx].round());
            }
            assert_eq!(subdag.commit_index, idx as u64 + 1);
        }

        // Check commits sent over consensus output channel is accurate
        let mut received_subdag_count = 0;
        while let Ok(subdag) = receiver.try_recv() {
            assert_eq!(subdag.leader, commits[received_subdag_count].leader);
            assert_eq!(subdag.blocks, commits[received_subdag_count].blocks);
            assert_eq!(
                subdag.commit_index,
                commits[received_subdag_count].commit_index
            );
            received_subdag_count += 1;

            if received_subdag_count == leaders.len() {
                break;
            }
        }

        verify_channel_empty(&mut receiver);

        // Check commits have been persisted to storage
        let last_commit = mem_store.read_last_commit().unwrap().unwrap();
        assert_eq!(last_commit.index, commits.last().unwrap().commit_index);
        let all_stored_commits = mem_store.scan_commits(0).unwrap();
        assert_eq!(all_stored_commits.len(), leaders.len());
        let blocks_existence = mem_store.contains_blocks(&expected_stored_refs).unwrap();
        assert!(blocks_existence.iter().all(|exists| *exists));
    }

    #[test]
    fn test_send_missing_commits_from_index() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);
        let mem_store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            mem_store.clone(),
        )));
        let linearizer = Linearizer::new(dag_state.clone());
        let leader_schedule = LeaderSchedule::new(context.clone());
        let last_received_index = 0;
        #[allow(clippy::disallowed_methods)] // allow unbounded_channel()
        let (sender, mut receiver) = unbounded_channel();

        let mut observer = CommitObserver::new(
            context.clone(),
            linearizer.clone(),
            sender.clone(),
            last_received_index,
            mem_store.clone(),
        );

        // Populate fully connected test blocks for round 0 ~ 10, authorities 0 ~ 3.
        let num_rounds = 10;
        let num_authorities = 4;
        let leaders = setup_test(
            context.clone(),
            dag_state.clone(),
            leader_schedule,
            num_rounds,
            num_authorities,
        );

        // Commit first batch of leaders (2) and "receive" the subdags as the
        // consumer of the consensus output channel.
        let expected_last_received_index = 2;
        let mut commits = observer.handle_commit(
            leaders
                .clone()
                .into_iter()
                .take(expected_last_received_index)
                .collect::<Vec<_>>(),
        );

        // Check commits sent over consensus output channel is accurate
        let mut received_subdag_index = 0;
        while let Ok(subdag) = receiver.try_recv() {
            tracing::info!("Received {subdag}");
            assert_eq!(subdag.leader, commits[received_subdag_index].leader);
            assert_eq!(subdag.blocks, commits[received_subdag_index].blocks);
            assert_eq!(
                subdag.commit_index,
                commits[received_subdag_index].commit_index
            );
            received_subdag_index = subdag.commit_index as usize;

            if received_subdag_index == expected_last_received_index {
                break;
            }
        }

        verify_channel_empty(&mut receiver);

        // Check last stored commit is correct
        let last_commit = mem_store.read_last_commit().unwrap().unwrap();
        assert_eq!(
            last_commit.index,
            expected_last_received_index as CommitIndex
        );

        // Handle next batch of leaders (1), these will be sent by consensus but not
        // "received" by consensus output channel. Simulating something happened on
        // the consumer side where the commits were not persisted.
        commits.append(
            &mut observer.handle_commit(
                leaders
                    .clone()
                    .into_iter()
                    .skip(expected_last_received_index)
                    .collect::<Vec<_>>(),
            ),
        );

        let expected_last_sent_index = 3;
        while let Ok(subdag) = receiver.try_recv() {
            tracing::info!("{subdag} was sent but not received by consumer");
            assert_eq!(subdag.leader, commits[received_subdag_index].leader);
            assert_eq!(subdag.blocks, commits[received_subdag_index].blocks);
            assert_eq!(
                subdag.commit_index,
                commits[received_subdag_index].commit_index
            );
            received_subdag_index = subdag.commit_index as usize;

            if received_subdag_index == expected_last_sent_index {
                break;
            }
        }

        verify_channel_empty(&mut receiver);

        // Check last stored commit is correct. We should persist the last commit
        // that was sent over the channel regardless of how the consumer handled
        // the commit on their end.
        let last_commit = mem_store.read_last_commit().unwrap().unwrap();
        assert_eq!(last_commit.index, expected_last_sent_index as CommitIndex);

        // Re-create commit observer starting from index 2 which represents the
        // last received index from the consumer over consensus output channel
        let _observer = CommitObserver::new(
            context.clone(),
            linearizer,
            sender,
            expected_last_received_index as CommitIndex,
            mem_store.clone(),
        );

        // Check commits sent over consensus output channel is accurate starting
        // from last received index of 2 and finishing at last sent index of 3.
        received_subdag_index = expected_last_received_index;
        while let Ok(subdag) = receiver.try_recv() {
            tracing::info!("Received {subdag} on resubmission");
            assert_eq!(subdag.leader, commits[received_subdag_index].leader);
            assert_eq!(subdag.blocks, commits[received_subdag_index].blocks);
            assert_eq!(
                subdag.commit_index,
                commits[received_subdag_index].commit_index
            );
            received_subdag_index = subdag.commit_index as usize;

            if received_subdag_index == expected_last_sent_index {
                break;
            }
        }

        verify_channel_empty(&mut receiver);
    }

    #[test]
    fn test_send_no_missing_commits() {
        telemetry_subscribers::init_for_testing();
        let context = Arc::new(Context::new_for_test(4).0);
        let mem_store = Arc::new(MemStore::new());
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            mem_store.clone(),
        )));
        let linearizer = Linearizer::new(dag_state.clone());
        let leader_schedule = LeaderSchedule::new(context.clone());
        let last_received_index = 0;
        #[allow(clippy::disallowed_methods)] // allow unbounded_channel()
        let (sender, mut receiver) = unbounded_channel();

        let mut observer = CommitObserver::new(
            context.clone(),
            linearizer.clone(),
            sender.clone(),
            last_received_index,
            mem_store.clone(),
        );

        // Populate fully connected test blocks for round 0 ~ 10, authorities 0 ~ 3.
        let num_rounds = 10;
        let num_authorities = 4;
        let leaders = setup_test(
            context.clone(),
            dag_state.clone(),
            leader_schedule,
            num_rounds,
            num_authorities,
        );

        // Commit all of the leaders and "receive" the subdags as the consumer of
        // the consensus output channel.
        let expected_last_received_index = 3;
        let commits = observer.handle_commit(leaders.clone());

        // Check commits sent over consensus output channel is accurate
        let mut received_subdag_index = 0;
        while let Ok(subdag) = receiver.try_recv() {
            tracing::info!("Received {subdag}");
            assert_eq!(subdag.leader, commits[received_subdag_index].leader);
            assert_eq!(subdag.blocks, commits[received_subdag_index].blocks);
            assert_eq!(
                subdag.commit_index,
                commits[received_subdag_index].commit_index
            );
            received_subdag_index = subdag.commit_index as usize;

            if received_subdag_index == expected_last_received_index {
                break;
            }
        }

        verify_channel_empty(&mut receiver);

        // Check last stored commit is correct
        let last_commit = mem_store.read_last_commit().unwrap().unwrap();
        assert_eq!(
            last_commit.index,
            expected_last_received_index as CommitIndex
        );

        // Re-create commit observer starting from index 3 which represents the
        // last received index from the consumer over consensus output channel
        let _observer = CommitObserver::new(
            context.clone(),
            linearizer,
            sender,
            expected_last_received_index as CommitIndex,
            mem_store.clone(),
        );

        // No commits should be resubmitted as consensus store's last commit index
        // is equal to last received index by consumer
        verify_channel_empty(&mut receiver);
    }

    /// Populate fully connected test blocks for round & authorities provided.
    fn setup_test(
        context: Arc<Context>,
        dag_state: Arc<RwLock<DagState>>,
        leader_schedule: LeaderSchedule,
        num_rounds: u32,
        num_authorities: u32,
    ) -> Vec<VerifiedBlock> {
        let mut leaders = vec![];

        let (genesis_references, genesis): (Vec<_>, Vec<_>) = context
            .committee
            .authorities()
            .map(|index| {
                let author_idx = index.0.value() as u32;
                let block = TestBlock::new(0, author_idx).build();
                VerifiedBlock::new_for_test(block)
            })
            .map(|block| (block.reference(), block))
            .unzip();
        dag_state.write().accept_blocks(genesis);

        let mut ancestors = genesis_references;
        for round in 1..=num_rounds {
            let mut new_ancestors = vec![];
            for author in 0..num_authorities {
                let base_ts = round as BlockTimestampMs * 1000;
                let block = VerifiedBlock::new_for_test(
                    TestBlock::new(round, author)
                        .set_timestamp_ms(base_ts + (author + round) as u64)
                        .set_ancestors(ancestors.clone())
                        .build(),
                );
                dag_state.write().accept_block(block.clone());
                new_ancestors.push(block.reference());

                if round % DEFAULT_WAVE_LENGTH == 0
                    && AuthorityIndex::new_for_test(author)
                        == leader_schedule.elect_leader(round, 0)
                {
                    leaders.push(block.clone());
                }
            }
            ancestors = new_ancestors;
        }

        leaders
    }

    /// After receiving all expected subdags, ensure channel is empty
    fn verify_channel_empty(receiver: &mut UnboundedReceiver<CommittedSubDag>) {
        match receiver.try_recv() {
            Ok(_) => {
                panic!("Expected the consensus output channel to be empty, but found more subdags.")
            }
            Err(e) => match e {
                tokio::sync::mpsc::error::TryRecvError::Empty => {}
                tokio::sync::mpsc::error::TryRecvError::Disconnected => {
                    panic!("The consensus output channel was unexpectedly closed.")
                }
            },
        }
    }
}
