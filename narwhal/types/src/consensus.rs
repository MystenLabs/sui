// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::mutable_key_type)]

use crate::{Batch, Certificate, CertificateDigest, Round};
use config::Committee;
use crypto::{PublicKey, PublicKeyBytes};
use fastcrypto::hash::Hash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use store::{
    rocks::{DBMap, TypedStoreError},
    traits::Map,
};
use tokio::sync::mpsc;

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
}

impl CommittedSubDag {
    pub fn len(&self) -> usize {
        self.certificates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn num_batches(&self) -> usize {
        self.certificates
            .iter()
            .map(|x| x.header.payload.len())
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
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, Eq, PartialEq)]
pub struct ReputationScores {
    /// Holds the score for every authority. If an authority is not amongst
    /// the records of the map then we assume that its score is zero.
    pub scores_per_authority: HashMap<PublicKey, u64>,
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
        let scores_per_authority = committee
            .authorities()
            .map(|a| (a.0.clone(), 0_u64))
            .collect();

        Self {
            scores_per_authority,
            ..Default::default()
        }
    }
    /// Adds the provided `score` to the existing score for the provided `authority`
    pub fn add_score(&mut self, authority: PublicKey, score: u64) {
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CommittedSubDagShell {
    /// The sequence of committed certificates' digests.
    pub certificates: Vec<CertificateDigest>,
    /// The leader certificate's digest responsible of committing this sub-dag.
    pub leader: CertificateDigest,
    // The round of the leader
    pub leader_round: Round,
    /// Sequence number of the CommittedSubDag
    pub sub_dag_index: SequenceNumber,
    /// The so far calculated reputation score for nodes
    pub reputation_score: ReputationScores,
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

/// Convenience type to propagate store errors.
pub type StoreResult<T> = Result<T, TypedStoreError>;

/// The persistent storage of the sequencer.
pub struct ConsensusStore {
    /// The latest committed round of each validator.
    last_committed: DBMap<PublicKeyBytes, Round>,
    /// The global consensus sequence.
    committed_sub_dags_by_index: DBMap<SequenceNumber, CommittedSubDagShell>,
}

impl ConsensusStore {
    /// Create a new consensus store structure by using already loaded maps.
    pub fn new(
        last_committed: DBMap<PublicKeyBytes, Round>,
        sequence: DBMap<SequenceNumber, CommittedSubDagShell>,
    ) -> Self {
        Self {
            last_committed,
            committed_sub_dags_by_index: sequence,
        }
    }

    /// Clear the store.
    pub fn clear(&self) -> StoreResult<()> {
        self.last_committed.clear()?;
        self.committed_sub_dags_by_index.clear()?;
        Ok(())
    }

    /// Persist the consensus state.
    pub fn write_consensus_state(
        &self,
        last_committed: &HashMap<PublicKeyBytes, Round>,
        sub_dag: &CommittedSubDag,
    ) -> Result<(), TypedStoreError> {
        let shell = CommittedSubDagShell::from_sub_dag(sub_dag);

        let mut write_batch = self.last_committed.batch();
        write_batch = write_batch.insert_batch(&self.last_committed, last_committed.iter())?;
        write_batch = write_batch.insert_batch(
            &self.committed_sub_dags_by_index,
            std::iter::once((sub_dag.sub_dag_index, shell)),
        )?;
        write_batch.write()
    }

    /// Load the last committed round of each validator.
    pub fn read_last_committed(&self) -> HashMap<PublicKeyBytes, Round> {
        self.last_committed.iter().collect()
    }

    /// Gets the latest sub dag index from the store
    pub fn get_latest_sub_dag_index(&self) -> SequenceNumber {
        let s = self
            .committed_sub_dags_by_index
            .iter()
            .skip_to_last()
            .next()
            .map(|(seq, _)| seq)
            .unwrap_or_default();
        s
    }

    /// Returns thet latest subdag committed. If none is committed yet, then
    /// None is returned instead.
    pub fn get_latest_sub_dag(&self) -> Option<CommittedSubDagShell> {
        self.committed_sub_dags_by_index
            .iter()
            .skip_to_last()
            .next()
            .map(|(_, subdag)| subdag)
    }

    /// Load all the sub dags committed with sequence number of at least `from`.
    pub fn read_committed_sub_dags_from(
        &self,
        from: &SequenceNumber,
    ) -> StoreResult<Vec<CommittedSubDagShell>> {
        Ok(self
            .committed_sub_dags_by_index
            .iter()
            .skip_to(from)?
            .map(|(_, sub_dag)| sub_dag)
            .collect())
    }
}
