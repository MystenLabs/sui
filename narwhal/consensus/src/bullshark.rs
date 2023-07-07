// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::consensus::{LeaderSchedule, LeaderSwapTable};
use crate::metrics::ConsensusMetrics;
use crate::{
    consensus::{ConsensusState, Dag},
    utils, ConsensusError, Outcome,
};
use config::{Committee, Stake};
use fastcrypto::hash::Hash;
use std::sync::Arc;
use storage::ConsensusStore;
use sui_protocol_config::ProtocolConfig;
use tokio::time::Instant;
use tracing::{debug, error_span};
use types::{Certificate, CertificateAPI, CommittedSubDag, HeaderAPI, ReputationScores, Round};

#[cfg(test)]
#[path = "tests/bullshark_tests.rs"]
pub mod bullshark_tests;

#[cfg(test)]
#[path = "tests/randomized_tests.rs"]
pub mod randomized_tests;

pub struct Bullshark {
    /// The committee information.
    pub committee: Committee,
    /// The protocol config settings allowing us to enable/disable features and support properties
    /// according to the supported protocol version.
    pub protocol_config: ProtocolConfig,
    /// Persistent storage to safe ensure crash-recovery.
    pub store: Arc<ConsensusStore>,
    /// The most recent round of inserted certificate
    pub max_inserted_certificate_round: Round,
    pub metrics: Arc<ConsensusMetrics>,
    /// The last time we had a successful leader election
    pub last_successful_leader_election_timestamp: Instant,
    /// The number of committed subdags that will trigger the schedule change and reputation
    /// score reset.
    pub num_sub_dags_per_schedule: u64,
    /// The leader election schedule to be used when need to find a round's leader
    pub leader_schedule: LeaderSchedule,
}

impl Bullshark {
    /// Create a new Bullshark consensus instance.
    pub fn new(
        committee: Committee,
        store: Arc<ConsensusStore>,
        protocol_config: ProtocolConfig,
        metrics: Arc<ConsensusMetrics>,
        num_sub_dags_per_schedule: u64,
        leader_schedule: LeaderSchedule,
    ) -> Self {
        Self {
            committee,
            protocol_config,
            store,
            last_successful_leader_election_timestamp: Instant::now(),
            max_inserted_certificate_round: 0,
            metrics,
            num_sub_dags_per_schedule,
            leader_schedule,
        }
    }

    /// Calculates the reputation score for the current commit by taking into account the reputation
    /// scores from the previous commit (assuming that exists). It returns the updated reputation score.
    fn resolve_reputation_score(
        &self,
        state: &mut ConsensusState,
        committed_sequence: &[Certificate],
        sub_dag_index: u64,
    ) -> ReputationScores {
        // we reset the scores for every schedule change window, or initialise when it's the first
        // sub dag we are going to create.
        // TODO: when schedule change is implemented we should probably change a little bit
        // this logic here.
        let mut reputation_score =
            if sub_dag_index == 1 || sub_dag_index % self.num_sub_dags_per_schedule == 0 {
                ReputationScores::new(&self.committee)
            } else {
                state
                    .last_committed_sub_dag
                    .as_ref()
                    .expect("Committed sub dag should always exist for sub_dag_index > 1")
                    .reputation_score
                    .clone()
            };

        // update the score for the previous leader. If no previous leader exists,
        // then this is the first time we commit a leader, so no score update takes place
        if let Some(last_committed_sub_dag) = state.last_committed_sub_dag.as_ref() {
            for certificate in committed_sequence {
                // TODO: we could iterate only the certificates of the round above the previous leader's round
                if certificate
                    .header()
                    .parents()
                    .iter()
                    .any(|digest| *digest == last_committed_sub_dag.leader.digest())
                {
                    reputation_score.add_score(certificate.origin(), 1);
                }
            }
        }

        // we check if this is the last sub dag of the current schedule. If yes then we mark the
        // scores as final_of_schedule = true so any downstream user can now that those are the last
        // ones calculated for the current schedule.
        reputation_score.final_of_schedule =
            (sub_dag_index + 1) % self.num_sub_dags_per_schedule == 0;

        // Always ensure that all the authorities are present in the reputation scores - even
        // when score is zero.
        assert_eq!(
            reputation_score.total_authorities() as usize,
            self.committee.size()
        );

        reputation_score
    }

    pub fn process_certificate(
        &mut self,
        state: &mut ConsensusState,
        certificate: Certificate,
    ) -> Result<(Outcome, Vec<CommittedSubDag>), ConsensusError> {
        debug!("Processing {:?}", certificate);
        let round = certificate.round();

        // Add the new certificate to the local storage.
        if !state.try_insert(&certificate)? {
            // Certificate has not been added to the dag since it's below commit round
            return Ok((Outcome::CertificateBelowCommitRound, vec![]));
        }

        self.report_leader_on_time_metrics(round, state);

        // Try to order the dag to commit. Start from the highest round for which we have at least
        // f+1 certificates. This is because we need them to provide
        // enough support to the leader.
        let r = round - 1;

        // We only elect leaders for even round numbers.
        if r % 2 != 0 || r < 2 {
            return Ok((Outcome::NoLeaderElectedForOddRound, Vec::new()));
        }

        // Get the certificate's digest of the leader. If we already ordered this leader,
        // there is nothing to do.
        let leader_round = r;
        if leader_round <= state.last_round.committed_round {
            return Ok((Outcome::LeaderBelowCommitRound, Vec::new()));
        }

        let leader = match self
            .leader_schedule
            .leader_certificate(leader_round, &state.dag)
        {
            (_leader_authority, Some(certificate)) => certificate,
            (_leader_authority, None) => {
                // leader has not been found - we don't have any certificate
                return Ok((Outcome::LeaderNotFound, Vec::new()));
            }
        };

        // Check if the leader has f+1 support from its children (ie. round r+1).
        let stake: Stake = state
            .dag
            .get(&round)
            .expect("We should have the whole history by now")
            .values()
            .filter(|(_, x)| x.header().parents().contains(&leader.digest()))
            .map(|(_, x)| self.committee.stake_by_id(x.origin()))
            .sum();

        // If it is the case, we can commit the leader. But first, we need to recursively go back to
        // the last committed leader, and commit all preceding leaders in the right order. Committing
        // a leader block means committing all its dependencies.
        if stake < self.committee.validity_threshold() {
            debug!("Leader {:?} does not have enough support", leader);
            return Ok((Outcome::NotEnoughSupportForLeader, Vec::new()));
        }

        // Get an ordered list of past leaders that are linked to the current leader.
        debug!("Leader {:?} has enough support", leader);
        let mut committed_sub_dags = Vec::new();
        let mut total_committed_certificates = 0;

        for leader in self.order_leaders(leader, state).iter().rev() {
            let sub_dag_index = state.next_sub_dag_index();
            let _span = error_span!("bullshark_process_sub_dag", sub_dag_index);

            debug!("Leader {:?} has enough support", leader);

            let mut min_round = leader.round();
            let mut sequence = Vec::new();

            // Starting from the oldest leader, flatten the sub-dag referenced by the leader.
            for x in utils::order_dag(leader, state) {
                // Update and clean up internal state.
                state.update(&x);

                // For logging.
                min_round = min_round.min(x.round());

                // Add the certificate to the sequence.
                sequence.push(x);
            }
            debug!(min_round, "Subdag has {} certificates", sequence.len());

            total_committed_certificates += sequence.len();

            // We resolve the reputation score that should be stored alongside with this sub dag.
            let reputation_score = self.resolve_reputation_score(state, &sequence, sub_dag_index);

            let sub_dag = CommittedSubDag::new(
                sequence,
                leader.clone(),
                sub_dag_index,
                reputation_score.clone(),
                state.last_committed_sub_dag.as_ref(),
            );

            // Persist the update.
            self.store
                .write_consensus_state(&state.last_committed, &sub_dag)?;

            // Update the last sub dag
            state.last_committed_sub_dag = Some(sub_dag.clone());

            committed_sub_dags.push(sub_dag);

            // If the leader schedule has been updated, then we'll need to recalculate any upcoming
            // leaders for the rest of the recursive commits.
            if self.update_leader_schedule(leader.round(), &reputation_score) {
                // TODO: a new schedule has been produced, which means that we need to recalculate leaders
                // to ensure that the new ones are considered and update the commit path.
            }
        }

        // record the last time we got a successful leader election
        let elapsed = self.last_successful_leader_election_timestamp.elapsed();

        self.metrics
            .commit_rounds_latency
            .observe(elapsed.as_secs_f64());

        self.last_successful_leader_election_timestamp = Instant::now();

        // The total leader_commits are expected to grow the same amount on validators,
        // but strong vs weak counts are not expected to be the same across validators.
        self.metrics
            .leader_commits
            .with_label_values(&["strong"])
            .inc();
        self.metrics
            .leader_commits
            .with_label_values(&["weak"])
            .inc_by(committed_sub_dags.len() as u64 - 1);

        // Log the latest committed round of every authority (for debug).
        // Performance note: if tracing at the debug log level is disabled, this is cheap, see
        // https://github.com/tokio-rs/tracing/pull/326
        for (name, round) in &state.last_committed {
            debug!("Latest commit of {}: Round {}", name, round);
        }

        self.metrics
            .committed_certificates
            .report(total_committed_certificates as u64);

        Ok((Outcome::Commit, committed_sub_dags))
    }

    /// Order the past leaders that we didn't already commit.
    pub fn order_leaders(&self, leader: &Certificate, state: &ConsensusState) -> Vec<Certificate> {
        let mut to_commit = vec![leader.clone()];
        let mut leader = leader;
        assert_eq!(leader.round() % 2, 0);
        for r in (state.last_round.committed_round + 2..=leader.round() - 2)
            .rev()
            .step_by(2)
        {
            // Get the certificate proposed by the previous leader.
            let (prev_leader, authority) =
                match self.leader_schedule.leader_certificate(r, &state.dag) {
                    (authority, Some(x)) => (x, authority),
                    (authority, None) => {
                        self.metrics
                            .leader_election
                            .with_label_values(&["not_found", authority.hostname()])
                            .inc();

                        continue;
                    }
                };

            // Check whether there is a path between the last two leaders.
            if self.linked(leader, prev_leader, &state.dag) {
                to_commit.push(prev_leader.clone());
                leader = prev_leader;
            } else {
                self.metrics
                    .leader_election
                    .with_label_values(&["no_path", authority.hostname()])
                    .inc();
            }
        }

        // Now just report all the found leaders
        let committee = self.committee.clone();
        let metrics = self.metrics.clone();

        to_commit.iter().for_each(|certificate| {
            let authority = committee.authority(&certificate.origin()).unwrap();

            metrics
                .leader_election
                .with_label_values(&["committed", authority.hostname()])
                .inc();
        });

        to_commit
    }

    /// Checks if there is a path between two leaders.
    fn linked(&self, leader: &Certificate, prev_leader: &Certificate, dag: &Dag) -> bool {
        let mut parents = vec![leader];
        for r in (prev_leader.round()..leader.round()).rev() {
            parents = dag
                .get(&r)
                .expect("We should have the whole history by now")
                .values()
                .filter(|(digest, _)| {
                    parents
                        .iter()
                        .any(|x| x.header().parents().contains(digest))
                })
                .map(|(_, certificate)| certificate)
                .collect();
        }
        parents.contains(&prev_leader)
    }

    // When the provided `reputation_scores` are "final" for the current schedule window, then we
    // create the new leader swap table and update the leader schedule to use it. Otherwise we do
    // nothing. If the schedule has been updated then true is returned.
    fn update_leader_schedule(
        &mut self,
        leader_round: Round,
        reputation_scores: &ReputationScores,
    ) -> bool {
        // Do not perform any update if the feature is disabled
        if self.protocol_config.narwhal_new_leader_election_schedule()
            && reputation_scores.final_of_schedule
        {
            // create the new swap table and update the scheduler
            self.leader_schedule
                .update_leader_swap_table(LeaderSwapTable::new(
                    &self.committee,
                    leader_round,
                    reputation_scores,
                ));

            return true;
        }
        false
    }

    fn report_leader_on_time_metrics(&mut self, certificate_round: Round, state: &ConsensusState) {
        if certificate_round > self.max_inserted_certificate_round
            && certificate_round % 2 == 0
            && certificate_round > 2
        {
            let previous_leader_round = certificate_round - 2;

            // This metric reports the leader election success for the last leader election round.
            // Our goal is to identify the rate of missed/failed leader elections which are a source
            // of tx latency. The metric's authority label can not be considered fully accurate when
            // we do change schedule as we'll try to calculate the previous leader round by using the
            // updated scores and consequently the new swap table. If the leader for that position has
            // changed, then a different hostname will be erroneously reported. For now not a huge
            // issue as it will be affect either:
            // * only the round where we switch schedules
            // * on long periods of asynchrony where we end up changing schedules late
            // and we don't really expect it to happen frequently.
            let authority = self.leader_schedule.leader(previous_leader_round);

            if state.last_round.committed_round < previous_leader_round {
                self.metrics
                    .leader_commit_accuracy
                    .with_label_values(&["miss", authority.hostname()])
                    .inc();
            } else {
                self.metrics
                    .leader_commit_accuracy
                    .with_label_values(&["hit", authority.hostname()])
                    .inc();
            }
        }

        self.max_inserted_certificate_round =
            self.max_inserted_certificate_round.max(certificate_round);
    }
}
