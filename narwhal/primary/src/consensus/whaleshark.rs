// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::consensus::metrics::ConsensusMetrics;
use crate::consensus::{
    utils, ConsensusError, ConsensusState, Dag, LeaderSchedule, LeaderSwapTable, Outcome, Protocol,
};
use config::{Committee, Stake};
use fastcrypto::hash::Hash;
use std::cmp::max;
use std::collections::VecDeque;
use std::sync::Arc;
use storage::ConsensusStore;
use sui_protocol_config::ProtocolConfig;
use tokio::time::Instant;
use tracing::{debug, error_span};
use types::{Certificate, CertificateAPI, CommittedSubDag, HeaderAPI, ReputationScores, Round};

#[derive(Debug, PartialEq, Eq)]
pub enum LeaderElectionStatus {
    Undecided,
    StrongApproval,
    StrongRejection,
    WeakApproval,
    WeakRejection,
}

pub struct Whaleshark {
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
    pub next_leader_round: Round,
    pub next_known_round: Round,
    pub leader_election_status: VecDeque<LeaderElectionStatus>,
}

impl Whaleshark {
    /// Create a new Whaleshark consensus instance.
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
            next_leader_round: 1,
            next_known_round: 1,
            leader_election_status: VecDeque::new(),
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

    /// Commits the leader of round `leader_round`. It is also recursively committing any earlier
    /// leader that hasn't been committed, assuming that's possible.
    /// If the schedule has changed due to a commit and there are more leaders to commit, then this
    /// method will return the enum `ScheduleChanged` so the caller will know to retry for the uncommitted
    /// leaders with the updated schedule now.
    fn try_commit_leaders(
        &mut self,
        new_round: Round,
        state: &mut ConsensusState,
    ) -> Result<(Outcome, Vec<CommittedSubDag>), ConsensusError> {
        self.next_leader_round = state.last_round.committed_round + 1;
        self.next_known_round = max(self.next_known_round, new_round + 1);
        while self.leader_election_status.len()
            < (self.next_known_round - self.next_leader_round) as usize
        {
            self.leader_election_status
                .push_back(LeaderElectionStatus::Undecided);
        }

        let mut round = self.next_known_round;
        while round > self.next_leader_round {
            round -= 1;
            let status_index = (round - self.next_leader_round) as usize;
            if self.leader_election_status[status_index] != LeaderElectionStatus::Undecided {
                continue;
            }

            // Count leader approval and rejections.
            let leader_certificate: Option<&Certificate> =
                self.leader_schedule.leader_certificate(round, &state.dag).1;
            let mut approve_stake = 0;
            let mut reject_stake = 0;
            let voters = state
                .dag
                .get(&(round + 1));
            if let Some(voters) = voters {
                for (_, x) in voters.values()
                {
                    let stake = self.committee.stake_by_id(x.origin());
                    let Some(leader) = leader_certificate else {
                        reject_stake += stake;
                        continue;
                    };
                    if x.header().parents().contains(&leader.digest()) {
                        approve_stake += stake;
                    } else {
                        reject_stake += stake;
                    }
                }
            }

            if approve_stake >= self.committee.quorum_threshold() {
                self.leader_election_status[status_index] = LeaderElectionStatus::StrongApproval;
            } else if reject_stake >= self.committee.quorum_threshold() {
                self.leader_election_status[status_index] = LeaderElectionStatus::StrongRejection;
            }
        }

        let mut leaders_to_commit = self.order_leaders(state);
        if leaders_to_commit.is_empty() {
            return Ok((Outcome::NotEnoughSupportForLeader, vec![]));
        }

        let mut committed_sub_dags = Vec::new();
        while let Some(leader) = leaders_to_commit.pop_front() {
            let sub_dag_index = state.next_sub_dag_index();
            let _span = error_span!("Whaleshark_process_sub_dag", sub_dag_index);

            debug!("Leader {:?} has enough support", leader);

            let mut min_round = leader.round();
            let mut sequence = Vec::new();

            // Starting from the oldest leader, flatten the sub-dag referenced by the leader.
            for x in utils::order_dag(&leader, state) {
                // Update and clean up internal state.
                state.update(&x);

                // For logging.
                min_round = min_round.min(x.round());

                // Add the certificate to the sequence.
                sequence.push(x);
            }
            debug!(min_round, "Subdag has {} certificates", sequence.len());

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

            let num_leaders_determined = leader.round() - self.next_leader_round + 1;
            for _ in 0..num_leaders_determined {
                self.leader_election_status.pop_front();
            }
            self.next_leader_round = leader.round() + 1;

            // If the leader schedule has been updated, then we'll need to recalculate any upcoming
            // leaders for the rest of the recursive commits. We do that by repeating the leader
            // election for the round that triggered the original commit
            if self.update_leader_schedule(leader.round(), &reputation_score) {
                self.leader_election_status.clear();
                // return that schedule has changed only when there are more leaders to commit until,
                // the `leader_round`, otherwise we have committed everything we could and practically
                // the leader of `leader_round` is the one that changed the schedule.
                if !leaders_to_commit.is_empty() {
                    return Ok((Outcome::ScheduleChanged, committed_sub_dags));
                }
            }
        }

        Ok((Outcome::Commit, committed_sub_dags))
    }

    /// Order the past leaders that we didn't already commit. It orders the leaders from the one
    /// of the older (smaller) round to the newest round.
    fn order_leaders(&mut self, state: &ConsensusState) -> VecDeque<Certificate> {
        let mut to_commit = VecDeque::new();

        for round in self.next_leader_round..self.next_known_round {
            let status_index: usize = (round - self.next_leader_round) as usize;
            let leader_certificate = self.leader_schedule.leader_certificate(round, &state.dag).1;

            if self.leader_election_status[status_index] == LeaderElectionStatus::Undecided {
                for r in round + 2..self.next_known_round {
                    let index: usize = (r - self.next_leader_round) as usize;
                    if self.leader_election_status[index] == LeaderElectionStatus::Undecided {
                        break;
                    }
                    if self.leader_election_status[index] == LeaderElectionStatus::StrongApproval
                        || self.leader_election_status[index] == LeaderElectionStatus::WeakApproval
                    {
                        let higher_leader_certificate = self
                            .leader_schedule
                            .leader_certificate(r, &state.dag)
                            .1
                            .unwrap();
                        let Some(next_round_certs) = state.dag.get(&(round + 1)) else {
                            break;
                        };
                        let approval_stake: Stake = next_round_certs
                            .values()
                            .map(|(_, x)| {
                                if !self.linked(higher_leader_certificate, x, &state.dag) {
                                    return 0;
                                }
                                let Some(leader) = leader_certificate else {
                                    return 0;
                                };
                                let stake = self.committee.stake_by_id(x.origin());
                                if x.header().parents().contains(&leader.digest()) {
                                    stake
                                } else {
                                    0
                                }
                            })
                            .sum();
                        if approval_stake >= self.committee.validity_threshold() {
                            self.leader_election_status[status_index] =
                                LeaderElectionStatus::WeakApproval;
                        } else {
                            self.leader_election_status[status_index] =
                                LeaderElectionStatus::WeakRejection;
                        }
                        break;
                    }
                }
            }

            if self.leader_election_status[status_index] == LeaderElectionStatus::Undecided {
                break;
            }

            if self.leader_election_status[status_index] == LeaderElectionStatus::StrongApproval
                || self.leader_election_status[status_index] == LeaderElectionStatus::WeakApproval
            {
                to_commit.push_back(leader_certificate.unwrap().clone());
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
    fn linked(&self, leader: &Certificate, cert: &Certificate, dag: &Dag) -> bool {
        let mut parents = vec![leader];
        for r in (cert.round()..leader.round()).rev() {
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
        parents.contains(&cert)
    }

    // When the provided `reputation_scores` are "final" for the current schedule window, then we
    // create the new leader swap table and update the leader schedule to use it. Otherwise we do
    // nothing. If the schedule has been updated then true is returned.
    fn update_leader_schedule(
        &mut self,
        leader_round: Round,
        reputation_scores: &ReputationScores,
    ) -> bool {
        if reputation_scores.final_of_schedule {
            // create the new swap table and update the scheduler
            self.leader_schedule
                .update_leader_swap_table(LeaderSwapTable::new(
                    &self.committee,
                    leader_round,
                    reputation_scores,
                    self.protocol_config.consensus_bad_nodes_stake_threshold(),
                ));

            self.metrics
                .num_of_bad_nodes
                .set(self.leader_schedule.num_of_bad_nodes() as i64);

            return true;
        }
        false
    }

    fn report_leader_on_time_metrics(&mut self, certificate_round: Round, state: &ConsensusState) {
        if certificate_round > self.max_inserted_certificate_round
            && certificate_round > 1
        {
            let previous_leader_round = certificate_round - 1;

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

impl Protocol for Whaleshark {
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

        self.report_leader_on_time_metrics(round, state);

        let mut committed_sub_dags = Vec::new();
        let outcome = loop {
            let (outcome, committed) = self.try_commit_leaders(certificate.round(), state)?;

            // always extend the returned sub dags
            committed_sub_dags.extend(committed);

            // break the loop and return the result as long as there is no schedule change.
            // We want to retry if there is a schedule change.
            if outcome != Outcome::ScheduleChanged {
                break outcome;
            }
        };

        // If we have no sub dag to commit then we simply return the outcome directly.
        // Otherwise we let the rest of the method run.
        if committed_sub_dags.is_empty() {
            return Ok((outcome, committed_sub_dags));
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

        let total_committed_certificates: u64 = committed_sub_dags
            .iter()
            .map(|sub_dag| sub_dag.certificates.len() as u64)
            .sum();

        self.metrics
            .committed_certificates
            .report(total_committed_certificates);

        Ok((Outcome::Commit, committed_sub_dags))
    }
}
