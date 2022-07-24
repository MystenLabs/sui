// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{CertificateDigest, Round};
use crypto::traits::VerifyingKey;
use std::{collections::HashMap, ops::RangeInclusive};
use store::{
    rocks::{DBMap, TypedStoreError},
    traits::Map,
};
use tokio::sync::mpsc;

/// A global sequence number assigned to every certificate.
pub type SequenceNumber = u64;

/// Shutdown token dropped when a task is properly shut down.
pub type ShutdownToken = mpsc::Sender<()>;

/// Convenience type to propagate store errors.
pub type StoreResult<T> = Result<T, TypedStoreError>;

/// The persistent storage of the sequencer.
pub struct ConsensusStore<PublicKey: VerifyingKey> {
    /// The latest committed round of each validator.
    last_committed: DBMap<PublicKey, Round>,
    /// The global consensus sequence.
    sequence: DBMap<SequenceNumber, CertificateDigest>,
}

impl<PublicKey: VerifyingKey> ConsensusStore<PublicKey> {
    /// Create a new consensus store structure by using already loaded maps.
    pub fn new(
        last_committed: DBMap<PublicKey, Round>,
        sequence: DBMap<SequenceNumber, CertificateDigest>,
    ) -> Self {
        Self {
            last_committed,
            sequence,
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

    /// Load the last committed round of each validator.
    pub fn read_last_committed(&self) -> HashMap<PublicKey, Round> {
        self.last_committed.iter().collect()
    }

    /// Load the certificate digests sequenced at a specific indices.
    pub fn read_sequenced_certificates(
        &self,
        missing: &RangeInclusive<SequenceNumber>,
    ) -> StoreResult<Vec<Option<CertificateDigest>>> {
        Ok(self
            .sequence
            .iter()
            .skip_to(missing.start())?
            .take_while(|(index, _)| index <= missing.end())
            .map(|(_, digest)| Some(digest))
            .collect())
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
}
