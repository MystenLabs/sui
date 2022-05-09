// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crypto::{traits::VerifyingKey, Hash};
use dag::node_dag::NodeDag;
use std::{collections::BTreeMap, ops::RangeInclusive};
use thiserror::Error;
use tokio::{sync::mpsc::Receiver, task::JoinHandle};
use tracing::instrument;
use types::{Certificate, CertificateDigest, Round};

/// Dag represents the pure dag that is constructed  by the certificate of each round without any
/// consensus running on top of it. This is a [`VerifyingKey`], [`Certificate`] and [`Round`]-aware
///  variant of the Dag, with a secondary index to link a (pubkey, round) pair to the possible
/// certified collection by that authority at that round.
///
#[derive(Debug)]
pub struct Dag<PublicKey: VerifyingKey> {
    /// Receives new certificates from the primary. The primary should send us new certificates only
    /// if it already sent us its whole history.
    rx_primary: Receiver<Certificate<PublicKey>>,

    /// The Virtual DAG data structure, which lets us track certificates in a memory-conscious way
    dag: NodeDag<Certificate<PublicKey>>,

    /// Secondary index: An authority-aware map of the DAG's veertex Certificates
    vertices: BTreeMap<(PublicKey, Round), CertificateDigest>,
}

/// Represents the errors that can be encountered in this concrete, [`VerifyingKey`],
/// [`Certificate`] and [`Round`]-aware variant of the Dag.
#[derive(Debug, Error)]
pub enum ValidatorDagError<PublicKey: VerifyingKey> {
    #[error("No remaining certificates for this authority: {0}")]
    OutOfCertificates(PublicKey),
    #[error("No known certificates for this authority: {0} at round {1}")]
    NoCertificateForCoordinates(PublicKey, Round),

    // The generic Dag structure
    #[error("Dag invariant violation {0}")]
    DagInvariantViolation(#[from] dag::node_dag::DagError),
}

impl<PublicKey: VerifyingKey> Dag<PublicKey> {
    pub fn spawn(rx_primary: Receiver<Certificate<PublicKey>>) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                rx_primary,
                dag: NodeDag::new(),
                vertices: BTreeMap::new(),
            }
            .run()
            .await
        })
    }

    async fn run(&mut self) {
        while let Some(certificate) = self.rx_primary.recv().await {
            // The Core (process_certificate) guarantees the certificate
            // has gone through causal completion => this is ready to be inserted
            let _ = self.insert(certificate);
        }
    }

    /// Inserts a Certificate in the Dag. The certificate must have been validated and so must all its parents, recursively
    #[instrument(level = "debug", err)]
    pub fn insert(
        &mut self,
        certificate: Certificate<PublicKey>,
    ) -> Result<(), ValidatorDagError<PublicKey>> {
        let digest = certificate.digest();
        let round = certificate.round();
        let origin = certificate.origin();

        // This fails if the validation of the certificate is incomplete
        self.dag.try_insert(certificate)?;
        // TODO: atomicity with the previous
        self.vertices.insert((origin, round), digest);
        Ok(())
    }

    /// Returns whether the node is still in the Dag as a strong reference, i.e. that it hasn't ben removed through compression.
    /// For the purposes of this memory-conscious graph, this is just "contains" semantics.
    pub fn contains(&self, digest: CertificateDigest) -> bool {
        self.dag.contains_live(digest)
    }

    /// Returns the oldest and newest rounds for which a validator has (live) certificates in the DAG
    #[instrument(level = "debug", err)]
    pub fn rounds(
        &mut self,
        origin: PublicKey,
    ) -> Result<std::ops::RangeInclusive<Round>, ValidatorDagError<PublicKey>> {
        let range = self
            .vertices
            .range((origin.clone(), Round::MIN)..(origin.clone(), Round::MAX));

        // In non-pathological cases, the range is non-empty, and has a lot of dropped nodes towards the beginning
        // yet this can't be a take_while because the DAG removal may be non-contiguous.
        //
        // Hence we rely on removals cleaning the secondary index.
        let mut strong_references = range.flat_map(|((_key, round), val)| {
            if self.contains(*val) {
                Some(round)
            } else {
                None
            }
        });
        let earliest = strong_references.next();
        let latest = strong_references.last().or(earliest);
        match (earliest, latest) {
            (Some(init), Some(end)) => Ok(RangeInclusive::new(*init, *end)),
            _ => Err(ValidatorDagError::OutOfCertificates(origin)),
        }
    }

    /// Returns a breadth first traversal of the Dag, starting with the certified collection
    /// passed as argument.
    #[instrument(level = "debug", err)]
    pub async fn read_causal(
        &self,
        start: CertificateDigest,
    ) -> Result<impl Iterator<Item = Certificate<PublicKey>>, ValidatorDagError<PublicKey>> {
        let bft = self.dag.bft(start)?;
        Ok(bft.map(|node_ref| node_ref.value().clone()))
    }

    /// Returns a breadth first traversal of the Dag, starting with the certified collection
    /// passed as argument.
    #[instrument(level = "debug", err)]
    pub async fn node_read_causal(
        &self,
        origin: PublicKey,
        round: Round,
    ) -> Result<impl Iterator<Item = Certificate<PublicKey>>, ValidatorDagError<PublicKey>> {
        let start_digest = self.vertices.get(&(origin.clone(), round)).ok_or(
            ValidatorDagError::NoCertificateForCoordinates(origin, round),
        )?;
        self.read_causal(*start_digest).await
    }

    /// Removes a certificate from the Dag, reclaiming memory in the process.
    pub fn remove(
        &mut self,
        digest: CertificateDigest,
    ) -> Result<(), ValidatorDagError<PublicKey>> {
        // TODO: not very satisfying re: atomicity
        if self.dag.make_compressible(digest)? {
            self.vertices.retain(|_k, v| v != &digest);
        }
        Ok(())
    }
}
