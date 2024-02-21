// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::VecDeque, sync::Arc};

use consensus_config::AuthorityIndex;
use parking_lot::RwLock;

use crate::{
    base_committer::BaseCommitter,
    block::{Round, Slot},
    commit::{Decision, LeaderStatus},
    context::Context,
    dag_state::DagState,
};

#[cfg(test)]
#[path = "tests/universal_committer_tests.rs"]
mod universal_committer_tests;

#[cfg(test)]
#[path = "tests/pipelined_committer_tests.rs"]
mod pipelined_committer_tests;

/// A universal committer uses a collection of committers to commit a sequence of leaders.
/// It can be configured to use a combination of different commit strategies, including
/// multi-leaders, backup leaders, and pipelines.
#[allow(unused)]
pub(crate) struct UniversalCommitter {
    /// The per-epoch configuration of this authority.
    context: Arc<Context>,
    /// In memory block store representing the dag state
    dag_state: Arc<RwLock<DagState>>,
    /// The list of committers for multi-leader or pipelining
    committers: Vec<BaseCommitter>,
}

impl UniversalCommitter {
    /// Try to commit part of the dag. This function is idempotent and returns a list of
    /// ordered decided leaders.
    #[tracing::instrument(skip_all, fields(last_decided = %last_decided))]
    pub(crate) fn try_commit(&self, last_decided: Slot) -> Vec<LeaderStatus> {
        let highest_accepted_round = self.dag_state.read().highest_accepted_round();

        // Try to decide as many leaders as possible, starting with the highest round.
        let mut leaders = VecDeque::new();
        // try to commit a leader up to the highest_accepted_round - 2. There is no
        // reason to try and iterate on higher rounds as in order to make a direct
        // decision for a leader at round R we need blocks from round R+2 to figure
        // out that enough certificates and support exist to commit a leader.
        'outer: for round in (last_decided.round..=highest_accepted_round.saturating_sub(2)).rev() {
            for committer in self.committers.iter().rev() {
                // Skip committers that don't have a leader for this round.
                let Some(leader) = committer.elect_leader(round) else {
                    continue;
                };

                // now that we reached the last committed leader we can stop the commit rule
                if leader == last_decided {
                    tracing::debug!("Reached last committed {leader}, now exit");
                    break 'outer;
                }

                tracing::debug!("Trying to decide {leader} with {committer}",);

                // Try to directly decide the leader.
                let mut status = committer.try_direct_decide(leader);
                tracing::debug!("Outcome of direct rule: {status}");

                // If we can't directly decide the leader, try to indirectly decide it.
                if status.is_decided() {
                    leaders.push_front((status.clone(), Decision::Direct));
                } else {
                    status = committer.try_indirect_decide(leader, leaders.iter().map(|(x, _)| x));
                    leaders.push_front((status.clone(), Decision::Indirect));
                    tracing::debug!("Outcome of indirect rule: {status}");
                }
            }
        }

        // The decided sequence is the longest prefix of decided leaders.
        leaders
            .into_iter()
            // Filter out all the genesis.
            .filter(|(x, _)| x.round() > 0)
            // Stop the sequence upon encountering an undecided leader.
            .take_while(|(x, _)| x.is_decided())
            // We want to report metrics at this point to ensure that the decisions
            // are reported only once hence we increase our accuracy
            .inspect(|(x, direct_decided)| {
                self.update_metrics(x, *direct_decided);
                tracing::debug!("Decided {x}");
            })
            .map(|(x, _)| x)
            .collect()
    }

    /// Return list of leaders for the round.
    /// Can return empty vec if round does not have a designated leader.
    pub(crate) fn get_leaders(&self, round: Round) -> Vec<AuthorityIndex> {
        self.committers
            .iter()
            .filter_map(|committer| committer.elect_leader(round))
            .map(|l| l.authority)
            .collect()
    }

    /// Update metrics.
    fn update_metrics(&self, leader: &LeaderStatus, decision: Decision) {
        let authority = leader.authority().to_string();
        let decision_str = if decision == Decision::Direct {
            "direct"
        } else {
            "indirect"
        };
        let status = match leader {
            LeaderStatus::Commit(..) => format!("{decision_str}-commit"),
            LeaderStatus::Skip(..) => format!("{decision_str}-skip"),
            LeaderStatus::Undecided(..) => return,
        };
        self.context
            .metrics
            .node_metrics
            .decided_leaders_total
            .with_label_values(&[&authority, &status])
            .inc();
    }
}

/// A builder for a universal committer. By default, the builder creates a single
/// base committer, that is, a single leader and no pipeline.
#[allow(unused)]
pub(crate) mod universal_committer_builder {
    use super::*;
    use crate::{
        base_committer::BaseCommitterOptions, commit::DEFAULT_WAVE_LENGTH,
        leader_schedule::LeaderSchedule,
    };

    pub(crate) struct UniversalCommitterBuilder {
        context: Arc<Context>,
        leader_schedule: LeaderSchedule,
        dag_state: Arc<RwLock<DagState>>,
        wave_length: Round,
        number_of_leaders: usize,
        pipeline: bool,
    }

    impl UniversalCommitterBuilder {
        pub(crate) fn new(context: Arc<Context>, dag_state: Arc<RwLock<DagState>>) -> Self {
            let leader_schedule = LeaderSchedule::new(context.clone());
            Self {
                context,
                leader_schedule,
                dag_state,
                wave_length: DEFAULT_WAVE_LENGTH,
                number_of_leaders: 1,
                pipeline: false,
            }
        }

        pub(crate) fn with_wave_length(mut self, wave_length: Round) -> Self {
            self.wave_length = wave_length;
            self
        }

        pub(crate) fn with_number_of_leaders(mut self, number_of_leaders: usize) -> Self {
            self.number_of_leaders = number_of_leaders;
            self
        }

        pub(crate) fn with_pipeline(mut self, pipeline: bool) -> Self {
            self.pipeline = pipeline;
            self
        }

        pub(crate) fn build(self) -> UniversalCommitter {
            let mut committers = Vec::new();
            let pipeline_stages = if self.pipeline { self.wave_length } else { 1 };
            for round_offset in 0..pipeline_stages {
                for leader_offset in 0..self.number_of_leaders {
                    let options = BaseCommitterOptions {
                        wave_length: self.wave_length,
                        round_offset,
                        leader_offset: leader_offset as Round,
                    };
                    let committer = BaseCommitter::new(
                        self.context.clone(),
                        self.leader_schedule.clone(),
                        self.dag_state.clone(),
                        options,
                    );
                    committers.push(committer);
                }
            }

            UniversalCommitter {
                context: self.context,
                dag_state: self.dag_state,
                committers,
            }
        }
    }
}
