// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::mutable_key_type)]

use crate::{Certificate, CertificateDigest, Round};
use crypto::PublicKey;
use fastcrypto::hash::Hash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use store::{
    rocks::{DBMap, TypedStoreError},
    traits::Map,
};
use tokio::sync::mpsc;

/// A global sequence number assigned to every certificate.
pub type SequenceNumber = u64;

/// The output format of the consensus.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ConsensusOutput {
    /// The sequenced certificate.
    pub certificate: Certificate,
    /// The (global) index associated with this certificate.
    pub consensus_index: SequenceNumber,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CommittedSubDag {
    /// The sequence of committed certificates.
    pub certificates: Vec<ConsensusOutput>,
    /// The leader certificate responsible of committing this sub-dag.
    pub leader: Certificate,
}

impl CommittedSubDag {
    pub fn len(&self) -> usize {
        self.certificates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn num_of_batches(&self) -> usize {
        self.certificates
            .iter()
            .map(|x| x.certificate.header.payload.len())
            .sum()
    }

    pub fn is_last(&self, output: &ConsensusOutput) -> bool {
        self.certificates
            .iter()
            .last()
            .map_or_else(|| false, |x| x == output)
    }

    pub fn round(&self) -> Round {
        self.leader.round()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CommittedSubDagShell {
    /// The sequence of committed certificates' digests.
    pub certificates: Vec<(CertificateDigest, SequenceNumber)>,
    /// The leader certificate's digest responsible of committing this sub-dag.
    pub leader: CertificateDigest,
}

impl CommittedSubDagShell {
    pub fn from_sub_dag(sub_dag: &CommittedSubDag) -> Self {
        Self {
            certificates: sub_dag
                .certificates
                .iter()
                .map(|x| (x.certificate.digest(), x.consensus_index))
                .collect(),
            leader: sub_dag.leader.digest(),
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
    last_committed: DBMap<PublicKey, Round>,
    /// The global consensus sequence.
    sequence: DBMap<SequenceNumber, CertificateDigest>,
    /// All committed sub-dags, indexed by the round number of the leader committing it.
    committed_sub_dags: DBMap<Round, CommittedSubDagShell>,
}

impl ConsensusStore {
    /// Create a new consensus store structure by using already loaded maps.
    pub fn new(
        last_committed: DBMap<PublicKey, Round>,
        sequence: DBMap<SequenceNumber, CertificateDigest>,
        committed_sub_dags: DBMap<Round, CommittedSubDagShell>,
    ) -> Self {
        Self {
            last_committed,
            sequence,
            committed_sub_dags,
        }
    }

    /// Clear the store.
    pub fn clear(&self) -> StoreResult<()> {
        self.last_committed.clear()?;
        self.sequence.clear()?;
        Ok(())
    }

    /// Persist the consensus state.
    pub fn write_consensus_state(
        &self,
        last_committed: &HashMap<PublicKey, Round>,
        consensus_index: &SequenceNumber,
        certificate_id: &CertificateDigest,
    ) -> Result<(), TypedStoreError> {
        let mut write_batch = self.last_committed.batch();
        write_batch = write_batch.insert_batch(&self.last_committed, last_committed.iter())?;
        write_batch = write_batch.insert_batch(
            &self.sequence,
            std::iter::once((consensus_index, certificate_id)),
        )?;
        write_batch.write()
    }

    /// Persist a committed sub dag.
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn write_committed_sub_dag(
        &self,
        last_committed: &HashMap<PublicKey, Round>,
        sub_dag: &CommittedSubDag,
    ) -> Result<(), TypedStoreError> {
        // Compress the sub-dag to not write the entire certificates.
        let shell = CommittedSubDagShell::from_sub_dag(sub_dag);

        // Atomically persist the sub-dag and the last committed certificates.
        let mut write_batch = self.last_committed.batch();
        write_batch = write_batch.insert_batch(&self.last_committed, last_committed.iter())?;
        write_batch = write_batch.insert_batch(
            &self.committed_sub_dags,
            std::iter::once((sub_dag.leader.round(), shell)),
        )?;
        write_batch.write()
    }

    /// Load the last committed round of each validator.
    pub fn read_last_committed(&self) -> HashMap<PublicKey, Round> {
        self.last_committed.iter().collect()
    }

    /// Load the last committed round of a validator.
    pub fn read_last_committed_round(
        &self,
        validator: &PublicKey,
    ) -> Result<Option<Round>, TypedStoreError> {
        self.last_committed.get(validator)
    }

    /// Load the certificate digests sequenced starting from the specified
    /// sequence number (inclusive). If the specified sequence number is not
    /// found then the method will skip to the next higher one and consume
    /// until the end.
    /// Method returns a vector of a tuple of the certificate digest
    /// with the next certificate index.
    pub fn read_sequenced_certificates_from(
        &self,
        from: &SequenceNumber,
    ) -> StoreResult<Vec<(SequenceNumber, CertificateDigest)>> {
        Ok(self.sequence.iter().skip_to(from)?.collect())
    }

    /// Load the last (ie. the highest) consensus index associated to a certificate.
    pub fn read_last_consensus_index(&self) -> StoreResult<SequenceNumber> {
        Ok(self
            .sequence
            .keys()
            .skip_prior_to(&SequenceNumber::MAX)?
            .next()
            .unwrap_or_default())
    }

    /// Load all the sub dags committed by a leader with round number of at least `from`.
    pub fn read_committed_sub_dags_from(
        &self,
        from: &Round,
    ) -> StoreResult<Vec<CommittedSubDagShell>> {
        Ok(self
            .committed_sub_dags
            .iter()
            .skip_to(from)?
            .map(|(_, sub_dag)| sub_dag)
            .collect())
    }
}
