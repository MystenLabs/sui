// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::{AuthorityIdentifier, Committee};
use dag::node_dag::{NodeDag, NodeDagError};
use fastcrypto::hash::Hash;
use mysten_metrics::spawn_logged_monitored_task;
use std::{
    borrow::Borrow,
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    ops::RangeInclusive,
    sync::{Arc, RwLock},
};
use thiserror::Error;
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        oneshot,
    },
    task::JoinHandle,
};
use tracing::instrument;
use types::{metered_channel, Certificate, CertificateDigest, ConditionalBroadcastReceiver, Round};

use crate::{metrics::ConsensusMetrics, DEFAULT_CHANNEL_SIZE};

#[cfg(any(test))]
#[path = "tests/dag_tests.rs"]
pub mod dag_tests;

/// Dag represents the Direct Acyclic Graph that is constructed by the certificate of each round without any
/// consensus running on top of it. This is a [`fastcrypto::traits::VerifyingKey`], [`Certificate`] and [`Round`]-aware
///  variant of the Dag, with a secondary index to link a (pubkey, round) pair to the possible
/// certified collection by that authority at that round.
struct InnerDag {
    /// Receives new certificates from the primary. The primary should send us new certificates only
    /// if it already sent us its whole history.
    rx_primary: metered_channel::Receiver<Certificate>,

    /// Receives new commands for the Dag.
    rx_commands: Receiver<DagCommand>,

    /// The Virtual DAG data structure, which lets us track certificates in a memory-conscious way
    dag: NodeDag<Certificate>,

    /// Secondary index: An authority-aware map of the DAG's vertex Certificates
    vertices: RwLock<BTreeMap<(AuthorityIdentifier, Round), CertificateDigest>>,

    /// Metrics handler
    metrics: Arc<ConsensusMetrics>,
    /// Receiver of shutdown signal
    rx_shutdown: ConditionalBroadcastReceiver,
}

/// The publicly exposed Dag handle, to which one can send commands
pub struct Dag {
    tx_commands: Sender<DagCommand>,
}

/// Represents the errors that can be encountered in this concrete, [`fastcrypto::traits::VerifyingKey`],
/// [`Certificate`] and [`Round`]-aware variant of the Dag.
#[derive(Debug, Error)]
pub enum ValidatorDagError {
    #[error("No remaining certificates in Dag for this authority: {0}")]
    OutOfCertificates(AuthorityIdentifier),
    #[error("No known certificates for this authority: {0} at round {1}")]
    NoCertificateForCoordinates(AuthorityIdentifier, Round),
    // an invariant violation at the level of the generic DAG (unrelated to Certificate specifics)
    #[error("Dag invariant violation {0}")]
    DagInvariantViolation(#[from] NodeDagError),
}

#[allow(clippy::large_enum_variant)]
enum DagCommand {
    Insert(
        Box<Certificate>,
        oneshot::Sender<Result<(), ValidatorDagError>>,
    ),
    Contains(CertificateDigest, oneshot::Sender<bool>),
    HasEverContained(CertificateDigest, oneshot::Sender<bool>),
    Rounds(
        AuthorityIdentifier,
        oneshot::Sender<Result<RangeInclusive<Round>, ValidatorDagError>>,
    ),
    ReadCausal(
        CertificateDigest,
        oneshot::Sender<Result<Vec<CertificateDigest>, ValidatorDagError>>,
    ),
    NodeReadCausal(
        (AuthorityIdentifier, Round),
        oneshot::Sender<Result<Vec<CertificateDigest>, ValidatorDagError>>,
    ),
    Remove(
        Vec<CertificateDigest>,
        oneshot::Sender<Result<(), ValidatorDagError>>,
    ),
    NotifyRead(
        CertificateDigest,
        oneshot::Sender<Result<Certificate, ValidatorDagError>>,
    ),
}

impl InnerDag {
    fn new(
        committee: &Committee,
        rx_primary: metered_channel::Receiver<Certificate>,
        rx_commands: Receiver<DagCommand>,
        dag: NodeDag<Certificate>,
        vertices: RwLock<BTreeMap<(AuthorityIdentifier, Round), CertificateDigest>>,
        metrics: Arc<ConsensusMetrics>,
        rx_shutdown: ConditionalBroadcastReceiver,
    ) -> Self {
        let mut idg = InnerDag {
            rx_primary,
            rx_commands,
            dag,
            vertices,
            metrics,
            rx_shutdown,
        };
        let genesis = Certificate::genesis(committee);
        for cert in genesis.into_iter() {
            idg.insert(cert)
                .expect("Insertion of the certificates produced by genesis should be leaves!");
        }
        idg
    }

    async fn run(&mut self) {
        let mut obligations = HashMap::<CertificateDigest, VecDeque<oneshot::Sender<_>>>::new();
        loop {
            tokio::select! {
                 Some(certificate) = self.rx_primary.recv() => {
                    // The Synchronizer (process_certificate) guarantees the certificate
                    // has gone through causal completion => this is ready to be inserted
                    let _ = self.insert(certificate);
                }
                Some(command) = self.rx_commands.recv() => {
                    match command {
                        DagCommand::Insert(cert, sender) => {
                            let _ = sender.send(self.insert(*cert.clone()));
                            let digest = cert.digest();
                            if let Some(mut senders) = obligations.remove(&digest) {
                                while let Some(s) = senders.pop_front() {
                                    let _ = s.send(Ok(*cert.clone()));
                                }
                            }
                        },
                        DagCommand::Contains(dig, sender)=> {
                            let _ = sender.send(self.contains(dig));
                        },
                        DagCommand::HasEverContained(dig, sender) => {
                            let _ = sender.send(self.has_ever_contained(dig));
                        }
                        DagCommand::Rounds(id, sender) => {
                            let _ = sender.send(self.rounds(id));
                        },
                        DagCommand::Remove(dig, sender) => {
                            let _ = sender.send(self.remove(dig));
                        },
                        DagCommand::ReadCausal(dig, sender) => {
                            let res = self.read_causal(dig);
                            let _ = sender.send(res.map(|r| r.collect()));
                        },
                        DagCommand::NodeReadCausal((id, round), sender) => {
                            let res = self.node_read_causal(id, round);
                            let _ = sender.send(res.map(|r| r.collect()));
                        },
                        DagCommand::NotifyRead(dig, sender) => {
                            let res = self.dag.get(dig);
                            if let Ok(node_ref) = res {
                                let _ = sender.send(Ok((*node_ref.value()).clone()));
                            } else {
                                obligations
                                    .entry(dig)
                                    .or_insert_with(VecDeque::new)
                                    .push_back(sender);
                            }
                        },
                    }
                },
                _ = self.rx_shutdown.receiver.recv() => {
                    return;
                }
            }
        }
    }

    #[instrument(level = "trace", skip_all, fields(certificate = ?certificate), err)]
    fn insert(&mut self, certificate: Certificate) -> Result<(), ValidatorDagError> {
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

        self.update_metrics();

        Ok(())
    }

    /// Returns whether the node is still in the Dag as a strong reference, i.e. that it hasn't been removed through compression.
    /// For the purposes of this memory-conscious graph, this is just "contains" semantics.
    fn contains(&self, digest: CertificateDigest) -> bool {
        self.dag.contains_live(digest)
    }

    /// Returns whether the dag has ever contained a node with the provided digest. The method will return
    /// true either when the node is live (uncompressed) or has been already compressed as still exists
    /// as weak reference.
    #[instrument(level = "trace", skip_all, fields(digest = ?digest))]
    fn has_ever_contained(&self, digest: CertificateDigest) -> bool {
        self.dag.contains(digest)
    }

    /// Returns the oldest and newest rounds for which a validator has (live) certificates in the DAG
    #[instrument(level = "trace", skip_all, fields(origin = ?origin), err)]
    fn rounds(
        &mut self,
        origin: AuthorityIdentifier,
    ) -> Result<RangeInclusive<Round>, ValidatorDagError> {
        // Our garbage collection is a mark-and-sweep algorithm, where the mark part is in `make_compressible` and
        // `read_causal` triggers a sweep.
        // To make sure we don't return rounds as live when wouldn't be seen as such from a subsequent `read_causal`
        // we need to trigger the sweep in the first place.

        // Look for the heads of the graph, then trigger the sweep
        for digest in self.dag.head_digests() {
            let _res = self.read_causal(digest).map(|iter| iter.last());
        }
        // TODO: this may become a big source of latency if the sweep has a lot of work to do! Make sure read_causal
        // calls are triggered from heads of the DAG in a background thread, and scheduled after calling remove.
        // - Subsequent `read_causal` calls will be cheaper,
        // - those background read_causals should flip a dirty bit here, so we maintain the invariant of at most one graph sweep
        //   between a remove_collections and a `rounds` call

        let (earliest, latest) = {
            // Perform the actual round probe
            let vertices = self.vertices.read().unwrap();
            let range = vertices.range((origin, Round::MIN)..(origin, Round::MAX));

            // In non-pathological cases, the range is non-empty, and has a lot of dropped nodes towards the beginning
            // yet this can't be a take_while because the DAG removal may be non-contiguous.
            //
            // Hence we rely on removals cleaning the secondary index.
            let mut strong_reference_rounds =
                range.flat_map(|((_key, round), val)| self.contains(*val).then_some(round));

            let earliest = strong_reference_rounds.next().cloned();
            let latest = strong_reference_rounds.last().cloned().or(earliest);
            (earliest, latest)
        };
        match (earliest, latest) {
            (Some(init), Some(end)) => Ok(RangeInclusive::new(init, end)),
            _ => Err(ValidatorDagError::OutOfCertificates(origin)),
        }
    }

    /// Returns a breadth first traversal of the Dag, starting with the certified collection
    /// passed as argument.
    #[instrument(level = "trace", skip_all, fields(start_certificate_id = ?start), err)]
    fn read_causal(
        &self,
        start: CertificateDigest,
    ) -> Result<impl Iterator<Item = CertificateDigest>, ValidatorDagError> {
        let bft = self.dag.bft(start)?;
        Ok(bft.map(|node_ref| node_ref.value().digest()))
    }

    /// Returns a breadth first traversal of the Dag, starting with the certified collection
    /// passed as argument.
    #[instrument(level = "trace", skip_all, fields(origin = ?origin, round = ?round), err)]
    fn node_read_causal(
        &self,
        origin: AuthorityIdentifier,
        round: Round,
    ) -> Result<impl Iterator<Item = CertificateDigest>, ValidatorDagError> {
        let vertices = self.vertices.read().unwrap();
        let start_digest = vertices.get(&(origin, round)).ok_or(
            ValidatorDagError::NoCertificateForCoordinates(origin, round),
        )?;
        self.read_causal(*start_digest)
    }

    /// Removes certificates from the Dag, reclaiming memory in the process.
    #[instrument(level = "trace", skip_all, fields(num_certificate_ids = digests.len()), err)]
    fn remove(&mut self, digests: Vec<CertificateDigest>) -> Result<(), ValidatorDagError> {
        {
            // TODO: lock-free atomicity
            let mut vertices = self.vertices.write().unwrap();
            // Deduplicate to avoid conflicts in acquiring references
            let digests = {
                let mut s = HashSet::new();
                digests.iter().for_each(|d| {
                    s.insert(*d);
                });
                s
            };
            let dag_removal_results = digests
                .iter()
                .map(|digest| self.dag.make_compressible(*digest));
            let (_successes, failures): (_, Vec<_>) = dag_removal_results.partition(Result::is_ok);
            let failures = failures
                .into_iter()
                .filter(|e| !matches!(e, Err(NodeDagError::DroppedDigest(_))))
                .collect::<Vec<_>>();

            // They're all unknown digest failures at this point,
            vertices.retain(|_k, v| !digests.contains(v));
            if !failures.is_empty() {
                let failure_digests = failures
                    .into_iter()
                    .filter_map(
                        |e| match_opt::match_opt!(e, Err(NodeDagError::UnknownDigests(d)) => d),
                    )
                    .flatten()
                    .collect::<Vec<_>>();
                return Err(NodeDagError::UnknownDigests(failure_digests).into());
            }
        }
        Ok(())
    }

    /// Updates the dag-related metrics
    fn update_metrics(&self) {
        let vertices = self.vertices.read().unwrap();

        self.metrics
            .external_consensus_dag_vertices_elements
            .with_label_values(&[])
            .set(vertices.len() as i64);

        self.metrics
            .external_consensus_dag_size
            .with_label_values(&[])
            .set(self.dag.size() as i64)
    }
}

impl Dag {
    pub fn new(
        committee: &Committee,
        rx_primary: metered_channel::Receiver<Certificate>,
        metrics: Arc<ConsensusMetrics>,
        rx_shutdown: ConditionalBroadcastReceiver,
    ) -> (JoinHandle<()>, Self) {
        let (tx_commands, rx_commands) = tokio::sync::mpsc::channel(DEFAULT_CHANNEL_SIZE);
        let mut idg = InnerDag::new(
            committee,
            rx_primary,
            rx_commands,
            /* dag */ NodeDag::new(),
            /* vertices */ RwLock::new(BTreeMap::new()),
            metrics,
            rx_shutdown,
        );

        let handle = spawn_logged_monitored_task!(async move { idg.run().await }, "DAGTask");
        let dag = Dag { tx_commands };
        (handle, dag)
    }

    /// Inserts a Certificate in the Dag.
    ///
    /// Note: the caller is responsible for validation of the certificate, including, but not limited to:
    /// - the certificate's signatures are valid,
    /// - the certificate has a valid number of parents by stake,
    /// - the certificate is well-formed (e.g. hashes match),
    /// - all the parents' certificates are recursively valid and have been inserted in the Dag.
    ///
    pub async fn insert(&self, certificate: Certificate) -> Result<(), ValidatorDagError> {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .tx_commands
            .send(DagCommand::Insert(Box::new(certificate), sender))
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

    /// Returns whether the dag has ever contained a node with the provided digest. The method will return
    /// true either when the node is live (uncompressed) or has been already compressed as still exists
    /// as weak reference.
    pub async fn has_ever_contained(&self, digest: CertificateDigest) -> bool {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .tx_commands
            .send(DagCommand::HasEverContained(digest, sender))
            .await
        {
            panic!("Failed to send HasEverContained command to store: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to HasEverContained command from store")
    }

    /// Returns the oldest and newest rounds for which a validator has (live) certificates in the DAG
    pub async fn rounds(
        &self,
        origin: AuthorityIdentifier,
    ) -> Result<RangeInclusive<Round>, ValidatorDagError> {
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
    ) -> Result<Vec<CertificateDigest>, ValidatorDagError> {
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
        origin: AuthorityIdentifier,
        round: Round,
    ) -> Result<Vec<CertificateDigest>, ValidatorDagError> {
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

    /// Removes certificates from the Dag, reclaiming memory in the process.
    ///
    /// Note: If some digests are unknown to the Dag, this will return an error, but will nonetheless delete
    /// the certificates for known digests which are removable.
    ///
    pub async fn remove<J: Borrow<CertificateDigest>>(
        &self,
        digest: impl IntoIterator<Item = J>,
    ) -> Result<(), ValidatorDagError> {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .tx_commands
            .send(DagCommand::Remove(
                digest.into_iter().map(|k| *k.borrow()).collect(),
                sender,
            ))
            .await
        {
            panic!("Failed to send Remove command to store: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to Remove command from store")
    }
    /// Returns the certificate for the digest by waiting until it is
    /// available in the dag
    pub async fn notify_read(
        &self,
        digest: CertificateDigest,
    ) -> Result<Certificate, ValidatorDagError> {
        let (sender, receiver) = oneshot::channel();
        if let Err(e) = self
            .tx_commands
            .send(DagCommand::NotifyRead(digest, sender))
            .await
        {
            panic!("Failed to send NotifyRead command: {e}");
        }
        receiver
            .await
            .expect("Failed to receive reply to NotifyRead command")
    }
}
