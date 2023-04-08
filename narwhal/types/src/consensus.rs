// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::mutable_key_type)]

use crate::{Batch, Certificate, CertificateAPI, CertificateDigest, HeaderAPI, Round, TimestampMs};
use config::{AuthorityIdentifier, Committee};
use enum_dispatch::enum_dispatch;
use fastcrypto::hash::Hash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::warn;

/// A global sequence number assigned to every CommittedSubDag.
pub type SequenceNumber = u64;

#[derive(Clone, Debug)]
/// The output of Consensus, which includes all the batches for each certificate in the sub dag
/// It is sent to the the ExecutionState handle_consensus_transactions
pub struct ConsensusOutput {
    pub sub_dag: Arc<CommittedSubDag>,
    pub batches: Vec<(Certificate, Vec<Batch>)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CommittedSubDag {
    /// The sequence of committed certificates.
    pub certificates: Vec<Certificate>,
    /// The leader certificate responsible of committing this sub-dag.
    pub leader: Certificate,
    /// The index associated with this CommittedSubDag
    pub sub_dag_index: SequenceNumber,
    /// The so far calculated reputation score for nodes
    pub reputation_score: ReputationScores,
    /// The timestamp that should identify this commit. This is guaranteed to be monotonically
    /// incremented. This is not necessarily the leader's timestamp. We compare the leader's timestamp
    /// with the previously committed sud dag timestamp and we always keep the max.
    /// Property is explicitly private so the method commit_timestamp() should be used instead which
    /// bears additional resolution logic.
    commit_timestamp: TimestampMs,
}

impl CommittedSubDag {
    pub fn new(
        certificates: Vec<Certificate>,
        leader: Certificate,
        sub_dag_index: SequenceNumber,
        reputation_score: ReputationScores,
        previous_sub_dag: Option<&CommittedSubDag>,
    ) -> Self {
        // Narwhal enforces some invariants on the header.created_at, so we can use it as a timestamp.
        let previous_sub_dag_ts = previous_sub_dag
            .map(|s| s.commit_timestamp)
            .unwrap_or_default();
        let commit_timestamp = previous_sub_dag_ts.max(*leader.header().created_at());

        if previous_sub_dag_ts > *leader.header().created_at() {
            warn!(sub_dag_index = ?sub_dag_index, "Leader timestamp {} is older than previously committed sub dag timestamp {}. Auto-correcting to max {}.",
            leader.header().created_at(), previous_sub_dag_ts, commit_timestamp);
        }

        Self {
            certificates,
            leader,
            sub_dag_index,
            reputation_score,
            commit_timestamp,
        }
    }

    pub fn from_commit(
        commit: ConsensusCommit,
        certificates: Vec<Certificate>,
        leader: Certificate,
    ) -> Self {
        Self {
            certificates,
            leader,
            sub_dag_index: commit.sub_dag_index(),
            reputation_score: commit.reputation_score(),
            commit_timestamp: commit.commit_timestamp(),
        }
    }

    pub fn len(&self) -> usize {
        self.certificates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn num_batches(&self) -> usize {
        self.certificates
            .iter()
            .map(|x| x.header().payload().len())
            .sum()
    }

    pub fn is_last(&self, output: &Certificate) -> bool {
        self.certificates
            .iter()
            .last()
            .map_or_else(|| false, |x| x == output)
    }

    pub fn leader_round(&self) -> Round {
        self.leader.round()
    }

    pub fn commit_timestamp(&self) -> TimestampMs {
        // If commit_timestamp is zero, then safely assume that this is an upgraded node that is
        // replaying this commit and field is never initialised. It's safe to fallback on leader's
        // timestamp.
        if self.commit_timestamp == 0 {
            return *self.leader.header().created_at();
        }
        self.commit_timestamp
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, Eq, PartialEq)]
pub struct ReputationScores {
    /// Holds the score for every authority. If an authority is not amongst
    /// the records of the map then we assume that its score is zero.
    pub scores_per_authority: HashMap<AuthorityIdentifier, u64>,
    /// When true it notifies us that those scores will be the last updated scores of the
    /// current schedule before they get reset for the next schedule and start
    /// scoring from the beginning. In practice we can leverage this information to
    /// use the scores during the next schedule until the next final ones are calculated.
    pub final_of_schedule: bool,
}

impl ReputationScores {
    /// Creating a new ReputationScores instance pre-populating the authorities entries with
    /// zero score value.
    pub fn new(committee: &Committee) -> Self {
        let scores_per_authority = committee.authorities().map(|a| (a.id(), 0_u64)).collect();

        Self {
            scores_per_authority,
            ..Default::default()
        }
    }
    /// Adds the provided `score` to the existing score for the provided `authority`
    pub fn add_score(&mut self, authority: AuthorityIdentifier, score: u64) {
        self.scores_per_authority
            .entry(authority)
            .and_modify(|value| *value += score)
            .or_insert(score);
    }

    pub fn total_authorities(&self) -> u64 {
        self.scores_per_authority.len() as u64
    }

    pub fn all_zero(&self) -> bool {
        !self.scores_per_authority.values().any(|e| *e > 0)
    }
}

#[enum_dispatch(ConsensusCommitAPI)]
trait ConsensusCommitAPI {
    fn certificates(&self) -> Vec<CertificateDigest>;
    fn leader(&self) -> CertificateDigest;
    fn leader_round(&self) -> Round;
    fn sub_dag_index(&self) -> SequenceNumber;
    fn reputation_score(&self) -> ReputationScores;
    fn commit_timestamp(&self) -> TimestampMs;
}

// TODO: remove once the upgrade has been rolled out. We want to keep only the
// CommittedSubDag
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CommittedSubDagShell {
    /// The sequence of committed certificates' digests.
    pub certificates: Vec<CertificateDigest>,
    /// The leader certificate's digest responsible of committing this sub-dag.
    pub leader: CertificateDigest,
    /// The round of the leader
    pub leader_round: Round,
    /// Sequence number of the CommittedSubDag
    pub sub_dag_index: SequenceNumber,
    /// The so far calculated reputation score for nodes
    pub reputation_score: ReputationScores,
}

impl ConsensusCommitAPI for CommittedSubDagShell {
    fn certificates(&self) -> Vec<CertificateDigest> {
        self.certificates.clone()
    }

    fn leader(&self) -> CertificateDigest {
        self.leader
    }

    fn leader_round(&self) -> Round {
        self.leader_round
    }

    fn sub_dag_index(&self) -> SequenceNumber {
        self.sub_dag_index
    }

    fn reputation_score(&self) -> ReputationScores {
        self.reputation_score.clone()
    }

    fn commit_timestamp(&self) -> TimestampMs {
        // We explicitly return 0 as we don't have this information stored already. This will be
        // handle accordingly to the CommittedSubdag struct.
        0
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConsensusCommitV2 {
    /// The sequence of committed certificates' digests.
    pub certificates: Vec<CertificateDigest>,
    /// The leader certificate's digest responsible of committing this sub-dag.
    pub leader: CertificateDigest,
    /// The round of the leader
    pub leader_round: Round,
    /// Sequence number of the CommittedSubDag
    pub sub_dag_index: SequenceNumber,
    /// The so far calculated reputation score for nodes
    pub reputation_score: ReputationScores,
    /// The timestamp that should identify this commit. This is guaranteed to be monotonically
    /// incremented
    pub commit_timestamp: TimestampMs,
}

impl ConsensusCommitV2 {
    pub fn from_sub_dag(sub_dag: &CommittedSubDag) -> Self {
        Self {
            certificates: sub_dag.certificates.iter().map(|x| x.digest()).collect(),
            leader: sub_dag.leader.digest(),
            leader_round: sub_dag.leader.round(),
            sub_dag_index: sub_dag.sub_dag_index,
            reputation_score: sub_dag.reputation_score.clone(),
            commit_timestamp: sub_dag.commit_timestamp,
        }
    }
}

impl ConsensusCommitAPI for ConsensusCommitV2 {
    fn certificates(&self) -> Vec<CertificateDigest> {
        self.certificates.clone()
    }

    fn leader(&self) -> CertificateDigest {
        self.leader
    }

    fn leader_round(&self) -> Round {
        self.leader_round
    }

    fn sub_dag_index(&self) -> SequenceNumber {
        self.sub_dag_index
    }

    fn reputation_score(&self) -> ReputationScores {
        self.reputation_score.clone()
    }

    fn commit_timestamp(&self) -> TimestampMs {
        self.commit_timestamp
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[enum_dispatch(ConsensusCommitAPI)]
pub enum ConsensusCommit {
    V1(CommittedSubDagShell),
    V2(ConsensusCommitV2),
}

impl ConsensusCommit {
    pub fn certificates(&self) -> Vec<CertificateDigest> {
        match self {
            ConsensusCommit::V1(sub_dag) => sub_dag.certificates(),
            ConsensusCommit::V2(sub_dag) => sub_dag.certificates(),
        }
    }

    pub fn leader(&self) -> CertificateDigest {
        match self {
            ConsensusCommit::V1(sub_dag) => sub_dag.leader(),
            ConsensusCommit::V2(sub_dag) => sub_dag.leader(),
        }
    }

    pub fn leader_round(&self) -> Round {
        match self {
            ConsensusCommit::V1(sub_dag) => sub_dag.leader_round(),
            ConsensusCommit::V2(sub_dag) => sub_dag.leader_round(),
        }
    }

    pub fn sub_dag_index(&self) -> SequenceNumber {
        match self {
            ConsensusCommit::V1(sub_dag) => sub_dag.sub_dag_index(),
            ConsensusCommit::V2(sub_dag) => sub_dag.sub_dag_index(),
        }
    }

    pub fn reputation_score(&self) -> ReputationScores {
        match self {
            ConsensusCommit::V1(sub_dag) => sub_dag.reputation_score(),
            ConsensusCommit::V2(sub_dag) => sub_dag.reputation_score(),
        }
    }

    pub fn commit_timestamp(&self) -> TimestampMs {
        match self {
            ConsensusCommit::V1(sub_dag) => sub_dag.commit_timestamp(),
            ConsensusCommit::V2(sub_dag) => sub_dag.commit_timestamp(),
        }
    }
}

impl CommittedSubDagShell {
    pub fn from_sub_dag(sub_dag: &CommittedSubDag) -> Self {
        Self {
            certificates: sub_dag.certificates.iter().map(|x| x.digest()).collect(),
            leader: sub_dag.leader.digest(),
            leader_round: sub_dag.leader.round(),
            sub_dag_index: sub_dag.sub_dag_index,
            reputation_score: sub_dag.reputation_score.clone(),
        }
    }
}

/// Shutdown token dropped when a task is properly shut down.
pub type ShutdownToken = mpsc::Sender<()>;

#[cfg(test)]
mod tests {
    use crate::{Certificate, Header, HeaderV1Builder};
    use crate::{CommittedSubDag, ReputationScores};
    use config::AuthorityIdentifier;
    use indexmap::IndexMap;
    use std::collections::BTreeSet;
    use test_utils::CommitteeFixture;

    #[test]
    fn test_zero_timestamp_in_sub_dag() {
        let fixture = CommitteeFixture::builder().build();
        let committee = fixture.committee();

        let header_builder = HeaderV1Builder::default();
        let header = header_builder
            .author(AuthorityIdentifier(1u16))
            .round(2)
            .epoch(0)
            .created_at(50)
            .payload(IndexMap::new())
            .parents(BTreeSet::new())
            .build()
            .unwrap();

        let certificate =
            Certificate::new_unsigned(&committee, Header::V1(header), Vec::new()).unwrap();

        // AND we initialise the sub dag via the "restore" way
        let sub_dag_round = CommittedSubDag {
            certificates: vec![certificate.clone()],
            leader: certificate,
            sub_dag_index: 1,
            reputation_score: ReputationScores::default(),
            commit_timestamp: 0,
        };

        // AND commit timestamp is the leader's timestamp
        assert_eq!(sub_dag_round.commit_timestamp(), 50);
    }

    #[test]
    fn test_monotonically_incremented_commit_timestamps() {
        // Create a certificate (leader) of round 2 with a high timestamp
        let newer_timestamp = 100;
        let older_timestamp = 50;

        let fixture = CommitteeFixture::builder().build();
        let committee = fixture.committee();

        let header_builder = HeaderV1Builder::default();
        let header = header_builder
            .author(AuthorityIdentifier(1u16))
            .round(2)
            .epoch(0)
            .created_at(newer_timestamp)
            .payload(IndexMap::new())
            .parents(BTreeSet::new())
            .build()
            .unwrap();

        let certificate =
            Certificate::new_unsigned(&committee, Header::V1(header), Vec::new()).unwrap();

        // AND
        let sub_dag_round_2 = CommittedSubDag::new(
            vec![certificate.clone()],
            certificate,
            1,
            ReputationScores::default(),
            None,
        );

        // AND commit timestamp is the leader's timestamp
        assert_eq!(sub_dag_round_2.commit_timestamp, newer_timestamp);

        // Now create the leader of round 4 with the older timestamp
        let header_builder = HeaderV1Builder::default();
        let header = header_builder
            .author(AuthorityIdentifier(1u16))
            .round(4)
            .epoch(0)
            .created_at(older_timestamp)
            .payload(IndexMap::new())
            .parents(BTreeSet::new())
            .build()
            .unwrap();

        let certificate =
            Certificate::new_unsigned(&committee, Header::V1(header), Vec::new()).unwrap();

        // WHEN create the sub dag based on the "previously committed" sub dag.
        let sub_dag_round_4 = CommittedSubDag::new(
            vec![certificate.clone()],
            certificate,
            2,
            ReputationScores::default(),
            Some(&sub_dag_round_2),
        );

        // THEN the latest sub dag should have the highest committed timestamp - basically the
        // same as the previous commit round
        assert_eq!(
            sub_dag_round_4.commit_timestamp,
            sub_dag_round_2.commit_timestamp
        );
    }
}
