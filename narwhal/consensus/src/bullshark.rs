// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::metrics::ConsensusMetrics;
use crate::{
    consensus::{ConsensusState, Dag},
    utils, ConsensusError, Outcome,
};
use config::{Authority, Committee, Stake};
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
}

impl Bullshark {
    /// Create a new Bullshark consensus instance.
    pub fn new(
        committee: Committee,
        store: Arc<ConsensusStore>,
        protocol_config: ProtocolConfig,
        metrics: Arc<ConsensusMetrics>,
        num_sub_dags_per_schedule: u64,
    ) -> Self {
        Self {
            committee,
            protocol_config,
            store,
            last_successful_leader_election_timestamp: Instant::now(),
            max_inserted_certificate_round: 0,
            metrics,
            num_sub_dags_per_schedule,
        }
    }

    // Returns the the authority which is the leader for the provided `round`.
    // Pay attention that this method will return always the first authority as the leader
    // when used under a test environment.
    pub fn leader_authority(committee: &Committee, round: Round) -> Authority {
        assert_eq!(
            round % 2,
            0,
            "We should never attempt to do a leader election for odd rounds"
        );

        cfg_if::cfg_if! {
            if #[cfg(test)] {
                // We apply round robin in leader election. Since we expect round to be an even number,
                // 2, 4, 6, 8... it can't work well for leader election as we'll omit leaders. Thus
                // we can always divide by 2 to get a monotonically incremented sequence,
                // 2/2 = 1, 4/2 = 2, 6/2 = 3, 8/2 = 4  etc, and then do minus 1 so we can always
                // start with base zero 0.
                let next_leader = (round/2 - 1) as usize % committee.size();
                let authorities = committee.authorities().collect::<Vec<_>>();

                (*authorities.get(next_leader).unwrap()).clone()
            } else {
                // Elect the leader in a stake-weighted choice seeded by the round
                committee.leader(round)
            }
        }
    }

    /// Returns the certificate originated by the leader of the specified round (if any). The Authority
    /// leader of the round is always returned and that's irrespective of whether the certificate exists
    /// as that's deterministically determined.
    fn leader<'a>(
        committee: &Committee,
        round: Round,
        dag: &'a Dag,
    ) -> (Authority, Option<&'a Certificate>) {
        // Note: this function is often called with even rounds only. While we do not aim at random selection
        // yet (see issue #10), repeated calls to this function should still pick from the whole roster of leaders.
        let leader = Self::leader_authority(committee, round);

        // Return its certificate and the certificate's digest.
        match dag.get(&round).and_then(|x| x.get(&leader.id())) {
            None => (leader, None),
            Some((_, certificate)) => (leader, Some(certificate)),
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

        let leader = match Self::leader(&self.committee, leader_round, &state.dag) {
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

        for leader in utils::order_leaders(
            &self.committee,
            leader,
            state,
            Self::leader,
            self.metrics.clone(),
        )
        .iter()
        .rev()
        {
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
                reputation_score,
                state.last_committed_sub_dag.as_ref(),
            );

            // Persist the update.
            self.store
                .write_consensus_state(&state.last_committed, &sub_dag)?;

            // Update the last sub dag
            state.last_committed_sub_dag = Some(sub_dag.clone());

            committed_sub_dags.push(sub_dag);
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

    fn report_leader_on_time_metrics(&mut self, certificate_round: Round, state: &ConsensusState) {
        if certificate_round > self.max_inserted_certificate_round
            && certificate_round % 2 == 0
            && certificate_round > 2
        {
            let previous_leader_round = certificate_round - 2;
            let authority = Self::leader_authority(&self.committee, previous_leader_round);

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
