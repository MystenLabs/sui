// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::metrics::ConsensusMetrics;
use crate::{
    consensus::{ConsensusProtocol, ConsensusState, Dag},
    utils, ConsensusError, Outcome,
};
use config::{AuthorityIdentifier, Committee, Stake};
use fastcrypto::hash::Hash;
use std::sync::Arc;
use tokio::time::Instant;
use tracing::{debug, error_span};
use types::{
    Certificate, CertificateAPI, CertificateDigest, CommittedSubDag, ConsensusStore, HeaderAPI,
    ReputationScores, Round,
};

#[cfg(test)]
#[path = "tests/bullshark_tests.rs"]
pub mod bullshark_tests;

#[cfg(test)]
#[path = "tests/randomized_tests.rs"]
pub mod randomized_tests;

/// LastRound is a helper struct to keep necessary info
/// around the leader election on the last election round.
/// When both the leader_found = true & leader_has_support = true
/// then we know that we do have a "successful" leader election
/// and consequently a commit.
#[derive(Default)]
pub struct LastRound {
    /// True when the leader has actually proposed a certificate
    /// and found in our DAG
    leader_found: bool,
    /// When the leader has enough support from downstream
    /// certificates
    leader_has_support: bool,
}

pub struct Bullshark {
    /// The committee information.
    pub committee: Committee,
    /// Persistent storage to safe ensure crash-recovery.
    pub store: Arc<ConsensusStore>,

    pub metrics: Arc<ConsensusMetrics>,
    /// The last time we had a successful leader election
    pub last_successful_leader_election_timestamp: Instant,
    /// The last round leader election result
    pub last_leader_election: LastRound,
    /// The most recent round of inserted certificate
    pub max_inserted_certificate_round: Round,
    /// The number of committed subdags that will trigger the schedule change and reputation
    /// score reset.
    pub num_sub_dags_per_schedule: u64,
}

impl ConsensusProtocol for Bullshark {
    fn process_certificate(
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

        // Report last leader election if was unsuccessful
        if round > self.max_inserted_certificate_round && round % 2 == 0 {
            let last_election_round = &self.last_leader_election;

            if !last_election_round.leader_found {
                self.metrics
                    .leader_election
                    .with_label_values(&["not_found"])
                    .inc();
            } else if !last_election_round.leader_has_support {
                self.metrics
                    .leader_election
                    .with_label_values(&["not_enough_support"])
                    .inc();
            }
        }

        self.max_inserted_certificate_round = self.max_inserted_certificate_round.max(round);

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
        let (leader_digest, leader) = match Self::leader(&self.committee, leader_round, &state.dag)
        {
            Some(x) => x,
            None => {
                self.last_leader_election = LastRound {
                    leader_found: false,
                    leader_has_support: false,
                };
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
            .filter(|(_, x)| x.header().parents().contains(leader_digest))
            .map(|(_, x)| self.committee.stake_by_id(x.origin()))
            .sum();

        self.last_leader_election = LastRound {
            leader_found: true,
            leader_has_support: false,
        };

        // If it is the case, we can commit the leader. But first, we need to recursively go back to
        // the last committed leader, and commit all preceding leaders in the right order. Committing
        // a leader block means committing all its dependencies.
        if stake < self.committee.validity_threshold() {
            debug!("Leader {:?} does not have enough support", leader);
            return Ok((Outcome::NotEnoughSupportForLeader, Vec::new()));
        }

        self.last_leader_election.leader_has_support = true;

        // Get an ordered list of past leaders that are linked to the current leader.
        debug!("Leader {:?} has enough support", leader);
        let mut committed_sub_dags = Vec::new();
        let mut total_committed_certificates = 0;

        // TODO: duplicated in tusk.rs
        for leader in utils::order_leaders(&self.committee, leader, state, Self::leader)
            .iter()
            .rev()
        {
            let sub_dag_index = state.latest_sub_dag_index + 1;
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

            // We update the reputation score stored in state
            let reputation_score = self.update_reputation_score(state, &sequence, sub_dag_index);

            let sub_dag = CommittedSubDag {
                certificates: sequence,
                leader: leader.clone(),
                sub_dag_index,
                reputation_score,
            };

            // Persist the update.
            self.store
                .write_consensus_state(&state.last_committed, &sub_dag)?;

            // Increase the global consensus index.
            state.latest_sub_dag_index = sub_dag_index;
            state.last_committed_leader = Some(sub_dag.leader.digest());

            committed_sub_dags.push(sub_dag);
        }

        // record the last time we got a successful leader election
        let elapsed = self.last_successful_leader_election_timestamp.elapsed();

        self.metrics
            .commit_rounds_latency
            .observe(elapsed.as_secs_f64());

        self.last_successful_leader_election_timestamp = Instant::now();

        self.metrics
            .leader_election
            .with_label_values(&["elected"])
            .inc();

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
}

impl Bullshark {
    /// Create a new Bullshark consensus instance.
    pub fn new(
        committee: Committee,
        store: Arc<ConsensusStore>,
        metrics: Arc<ConsensusMetrics>,
        num_sub_dags_per_schedule: u64,
    ) -> Self {
        Self {
            committee,
            store,
            last_successful_leader_election_timestamp: Instant::now(),
            last_leader_election: LastRound::default(),
            max_inserted_certificate_round: 0,
            metrics,
            num_sub_dags_per_schedule,
        }
    }

    // Returns the PublicKey of the authority which is the leader for the provided `round`.
    // Pay attention that this method will return always the first authority as the leader
    // when used under a test environment.
    pub fn leader_authority(committee: &Committee, round: Round) -> AuthorityIdentifier {
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

                authorities.get(next_leader).unwrap().id()
            } else {
                // Elect the leader in a stake-weighted choice seeded by the round
                committee.leader(round).id()
            }
        }
    }

    // TODO: duplicated in tusk.rs
    /// Returns the certificate (and the certificate's digest) originated by the leader of the
    /// specified round (if any).
    fn leader<'a>(
        committee: &Committee,
        round: Round,
        dag: &'a Dag,
    ) -> Option<&'a (CertificateDigest, Certificate)> {
        // Note: this function is often called with even rounds only. While we do not aim at random selection
        // yet (see issue #10), repeated calls to this function should still pick from the whole roster of leaders.
        let leader = Self::leader_authority(committee, round);

        // Return its certificate and the certificate's digest.
        dag.get(&round).and_then(|x| x.get(&leader))
    }

    /// Updates and calculates the reputation score for the current commit managing any internal state.
    /// It returns the updated reputation score.
    fn update_reputation_score(
        &mut self,
        state: &mut ConsensusState,
        committed_sequence: &[Certificate],
        sub_dag_index: u64,
    ) -> ReputationScores {
        // we reset the scores for every schedule change window.
        // TODO: when schedule change is implemented we should probably change a little bit
        // this logic here.
        if sub_dag_index % self.num_sub_dags_per_schedule == 0 {
            state.last_consensus_reputation_score = ReputationScores::new(&self.committee)
        }

        // update the score for the previous leader. If no previous leader exists,
        // then this is the first time we commit a leader, so no score update takes place
        if let Some(previous_leader) = state.last_committed_leader {
            for certificate in committed_sequence {
                // TODO: we could iterate only the certificates of the round above the previous leader's round
                if certificate
                    .header()
                    .parents()
                    .iter()
                    .any(|digest| *digest == previous_leader)
                {
                    state
                        .last_consensus_reputation_score
                        .add_score(certificate.origin(), 1);
                }
            }
        }

        // we check if this is the last subdag of the current schedule. If yes then we mark the
        // scores as final_of_schedule = true so any downstream user can now that those are the last
        // ones calculated for the current schedule.
        state.last_consensus_reputation_score.final_of_schedule =
            (sub_dag_index + 1) % self.num_sub_dags_per_schedule == 0;

        // Always ensure that all the authorities are present in the reputation scores - even
        // when score is zero.
        assert_eq!(
            state.last_consensus_reputation_score.total_authorities() as usize,
            self.committee.size()
        );

        state.last_consensus_reputation_score.clone()
    }
}
