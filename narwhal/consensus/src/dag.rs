// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crypto::{traits::VerifyingKey, Hash};
use dag::node_dag::NodeDag;
use std::{collections::BTreeMap, ops::RangeInclusive, sync::RwLock};
use thiserror::Error;
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        oneshot,
    },
    task::JoinHandle,
};
use tracing::instrument;
use types::{Certificate, CertificateDigest, Round};

use crate::DEFAULT_CHANNEL_SIZE;

#[cfg(any(test))]
#[path = "tests/dag_tests.rs"]
pub mod dag_tests;

/// Dag represents the Direct Acyclic Graph that is constructed by the certificate of each round without any
/// consensus running on top of it. This is a [`VerifyingKey`], [`Certificate`] and [`Round`]-aware
///  variant of the Dag, with a secondary index to link a (pubkey, round) pair to the possible
/// certified collection by that authority at that round.
///
#[derive(Debug)]
pub struct InnerDag<PublicKey: VerifyingKey> {
    /// Receives new certificates from the primary. The primary should send us new certificates only
    /// if it already sent us its whole history.
    rx_primary: Receiver<Certificate<PublicKey>>,

    /// Receives new commands for the Dag.
    rx_commands: Receiver<DagCommand<PublicKey>>,

    /// The Virtual DAG data structure, which lets us track certificates in a memory-conscious way
    dag: NodeDag<Certificate<PublicKey>>,

    /// Secondary index: An authority-aware map of the DAG's veertex Certificates
    vertices: RwLock<BTreeMap<(PublicKey, Round), CertificateDigest>>,
}

/// The publicly exposed Dag handle, to which one can expose commands
pub struct Dag<PublicKey: VerifyingKey> {
    tx_commands: Sender<DagCommand<PublicKey>>,
}

/// Represents the errors that can be encountered in this concrete, [`VerifyingKey`],
/// [`Certificate`] and [`Round`]-aware variant of the Dag.
#[derive(Debug, Error)]
pub enum ValidatorDagError<PublicKey: VerifyingKey> {
    #[error("No remaining certificates for this authority: {0}")]
    OutOfCertificates(PublicKey),
    #[error("No known certificates for this authority: {0} at round {1}")]
    NoCertificateForCoordinates(PublicKey, Round),
    // an invariant violation at the level of the generic DAG (unrelated to Certificate specifics)
    #[error("Dag invariant violation {0}")]
    DagInvariantViolation(#[from] dag::node_dag::NodeDagError),
}

enum DagCommand<PublicKey: VerifyingKey> {
    Insert(
        Certificate<PublicKey>,
        oneshot::Sender<Result<(), ValidatorDagError<PublicKey>>>,
    ),
    Contains(CertificateDigest, oneshot::Sender<bool>),
    Rounds(
        PublicKey,
        oneshot::Sender<Result<std::ops::RangeInclusive<Round>, ValidatorDagError<PublicKey>>>,
    ),
    ReadCausal(
        CertificateDigest,
        oneshot::Sender<Result<Vec<CertificateDigest>, ValidatorDagError<PublicKey>>>,
    ),
    NodeReadCausal(
        (PublicKey, Round),
        oneshot::Sender<Result<Vec<CertificateDigest>, ValidatorDagError<PublicKey>>>,
    ),
    Remove(
        CertificateDigest,
        oneshot::Sender<Result<(), ValidatorDagError<PublicKey>>>,
    ),
}

impl<PublicKey: VerifyingKey> InnerDag<PublicKey> {
    pub fn spawn(rx_primary: Receiver<Certificate<PublicKey>>) -> (JoinHandle<()>, Dag<PublicKey>) {
        let (tx_commands, rx_commands) = tokio::sync::mpsc::channel(DEFAULT_CHANNEL_SIZE);

        let handle = tokio::spawn(async move {
            Self {
                rx_primary,
                rx_commands,
                dag: NodeDag::new(),
                vertices: RwLock::new(BTreeMap::new()),
            }
            .run()
            .await
        });
        let dag = Dag { tx_commands };
        (handle, dag)
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                 Some(certificate) = self.rx_primary.recv() => {
                    // The Core (process_certificate) guarantees the certificate
                    // has gone through causal completion => this is ready to be inserted
                    let _ = self.insert(certificate);
                }
                Some(command) = self.rx_commands.recv() => {
                    match command {
                        DagCommand::Insert(cert, sender) => { let _ = sender.send(self.insert(cert)); },
                        DagCommand::Contains(dig, sender)=> {
                            let _ = sender.send(self.contains(dig));
                        },
                        DagCommand::Rounds(pk, sender) => {
                            let _ = sender.send(self.rounds(pk));
                        },
                        DagCommand::Remove(dig, sender) => {
                            let _ = sender.send(self.remove(dig));
                        },
                        DagCommand::ReadCausal(dig, sender) => {
                            let res = self.read_causal(dig);
                            let _ = sender.send(res.map(|r| r.collect()));
                        },
                        DagCommand::NodeReadCausal((pk, round), sender) => {
                            let res = self.node_read_causal(pk, round);
                            let _ = sender.send(res.map(|r| r.collect()));
                        },
                    }
                }
            }
        }
    }

    #[instrument(level = "debug", err)]
    fn insert(
        &mut self,
        certificate: Certificate<PublicKey>,
    ) -> Result<(), ValidatorDagError<PublicKey>> {
        let digest = certificate.digest();
        let round = certificate.round();
        let origin = certificate.origin();

        {
            // TODO: lock-free atomicity (per-key guard here)
            let mut vertices = self.vertices.write().unwrap();
            // This fails if the validation of the certificate is incomplete
            self.dag.try_insert(certificate)?;
            vertices.insert((origin, round), digest);
        }
        Ok(())
    }

    /// Returns whether the node is still in the Dag as a strong reference, i.e. that it hasn't been removed through compression.
    /// For the purposes of this memory-conscious graph, this is just "contains" semantics.
    fn contains(&self, digest: CertificateDigest) -> bool {
        self.dag.contains_live(digest)
    }

    /// Returns the oldest and newest rounds for which a validator has (live) certificates in the DAG
    #[instrument(level = "debug", err)]
    fn rounds(
        &mut self,
        origin: PublicKey,
    ) -> Result<std::ops::RangeInclusive<Round>, ValidatorDagError<PublicKey>> {
        let (earliest, latest) = {
            let vertices = self.vertices.read().unwrap();
            let range = vertices.range((origin.clone(), Round::MIN)..(origin.clone(), Round::MAX));

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

            let earliest = strong_references.next().cloned();
            let latest = strong_references.last().cloned().or(earliest);
            (earliest, latest)
        };
        match (earliest, latest) {
            (Some(init), Some(end)) => Ok(RangeInclusive::new(init, end)),
            _ => Err(ValidatorDagError::OutOfCertificates(origin)),
        }
    }

    /// Returns a breadth first traversal of the Dag, starting with the certified collection
    /// passed as argument.
    #[instrument(level = "debug", err)]
    fn read_causal(
        &self,
        start: CertificateDigest,
    ) -> Result<impl Iterator<Item = CertificateDigest>, ValidatorDagError<PublicKey>> {
        let bft = self.dag.bft(start)?;
        Ok(bft.map(|node_ref| node_ref.value().digest()))
    }

    /// Returns a breadth first traversal of the Dag, starting with the certified collection
    /// passed as argument.
    #[instrument(level = "debug", err)]
    fn node_read_causal(
        &self,
        origin: PublicKey,
        round: Round,
    ) -> Result<impl Iterator<Item = CertificateDigest>, ValidatorDagError<PublicKey>> {
        let vertices = self.vertices.read().unwrap();
        let start_digest = vertices.get(&(origin.clone(), round)).ok_or(
            ValidatorDagError::NoCertificateForCoordinates(origin, round),
        )?;
        self.read_causal(*start_digest)
    }

    /// Removes a certificate from the Dag, reclaiming memory in the process.
    fn remove(&mut self, digest: CertificateDigest) -> Result<(), ValidatorDagError<PublicKey>> {
        {
            // TODO: lock-free atomicity
            let mut vertices = self.vertices.write().unwrap();
            if self.dag.make_compressible(digest)? {
                vertices.retain(|_k, v| v != &digest);
            }
        }
        Ok(())
    }
}

impl<PublicKey: VerifyingKey> Dag<PublicKey> {
    pub fn new(rx_primary: Receiver<Certificate<PublicKey>>) -> (JoinHandle<()>, Self) {
        let (tx_commands, rx_commands) = tokio::sync::mpsc::channel(DEFAULT_CHANNEL_SIZE);

        let handle = tokio::spawn(async move {
            InnerDag {
                rx_primary,
                rx_commands,
                dag: NodeDag::new(),
                vertices: RwLock::new(BTreeMap::new()),
            }
            .run()
            .await
        });
        let dag = Dag { tx_commands };
        (handle, dag)
    }

    /// Inserts a Certificate in the Dag. The certificate must have been validated and so must all its parents, recursively
    pub async fn insert(
        &mut self,
        certificate: Certificate<PublicKey>,
    ) -> Result<(), ValidatorDagError<PublicKey>> {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .tx_commands
            .send(DagCommand::Insert(certificate, sender))
            .await
        {
            panic!("Failed to send Insert command to store: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to Insert command from store")
    }

    /// Returns whether the node is still in the Dag as a strong reference, i.e. that it hasn't ben removed through compression.
    /// For the purposes of this memory-conscious graph, this is just "contains" semantics.
    pub async fn contains(&self, digest: CertificateDigest) -> bool {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .tx_commands
            .send(DagCommand::Contains(digest, sender))
            .await
        {
            panic!("Failed to send Contains command to store: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to Contains command from store")
    }

    /// Returns the oldest and newest rounds for which a validator has (live) certificates in the DAG
    pub async fn rounds(
        &mut self,
        origin: PublicKey,
    ) -> Result<std::ops::RangeInclusive<Round>, ValidatorDagError<PublicKey>> {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .tx_commands
            .send(DagCommand::Rounds(origin, sender))
            .await
        {
            panic!("Failed to send Rounds command to store: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to Rounds command from store")
    }

    /// Returns a breadth first traversal of the Dag, starting with the certified collection
    /// passed as argument.
    pub async fn read_causal(
        &self,
        start: CertificateDigest,
    ) -> Result<Vec<CertificateDigest>, ValidatorDagError<PublicKey>> {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .tx_commands
            .send(DagCommand::ReadCausal(start, sender))
            .await
        {
            panic!("Failed to send ReadCausal command to store: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to ReadCausal command from store")
    }

    /// Returns a breadth first traversal of the Dag, starting with the certified collection
    /// passed as argument.
    pub async fn node_read_causal(
        &self,
        origin: PublicKey,
        round: Round,
    ) -> Result<Vec<CertificateDigest>, ValidatorDagError<PublicKey>> {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .tx_commands
            .send(DagCommand::NodeReadCausal((origin, round), sender))
            .await
        {
            panic!("Failed to send NodeReadCausal command to store: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to NodeReadCausal command from store")
    }

    /// Removes a certificate from the Dag, reclaiming memory in the process.
    pub async fn remove(
        &mut self,
        digest: CertificateDigest,
    ) -> Result<(), ValidatorDagError<PublicKey>> {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .tx_commands
            .send(DagCommand::Remove(digest, sender))
            .await
        {
            panic!("Failed to send Remove command to store: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to Remove command from store")
    }
}
