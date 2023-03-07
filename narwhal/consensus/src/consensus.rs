// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::mutable_key_type)]

use crate::{metrics::ConsensusMetrics, ConsensusError, Outcome, SequenceNumber};
use config::Committee;
use crypto::PublicKey;
use fastcrypto::hash::Hash;
use mysten_metrics::spawn_logged_monitored_task;
use std::{
    cmp::{max, Ordering},
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use storage::CertificateStore;
use tokio::{sync::watch, task::JoinHandle};
use tracing::{debug, info, instrument};
use types::{
    metered_channel, Certificate, CertificateDigest, CommittedSubDag, CommittedSubDagShell,
    ConditionalBroadcastReceiver, ConsensusStore, ReputationScores, Round, Timestamp,
};

#[cfg(test)]
#[path = "tests/consensus_tests.rs"]
pub mod consensus_tests;

/// The representation of the DAG in memory.
pub type Dag = BTreeMap<Round, HashMap<PublicKey, (CertificateDigest, Certificate)>>;

/// The state that needs to be persisted for crash-recovery.
pub struct ConsensusState {
    /// The last committed round.
    pub last_committed_round: Round,
    /// Keeps the last committed round for each authority. This map is used to clean up the dag and
    /// ensure we don't commit twice the same certificate.
    pub last_committed: HashMap<PublicKey, Round>,
    /// Used to populate the index in the sub-dag construction.
    pub latest_sub_dag_index: SequenceNumber,
    /// The last calculated consensus reputation score
    pub last_consensus_reputation_score: ReputationScores,
    /// The last committed sub dag leader. This allow us to calculate the reputation score of the nodes
    /// that vote for the last leader.
    pub last_committed_leader: Option<CertificateDigest>,
    /// Keeps the latest committed certificate (and its parents) for every authority. Anything older
    /// must be regularly cleaned up through the function `update`.
    pub dag: Dag,
    /// Metrics handler
    pub metrics: Arc<ConsensusMetrics>,
}

impl ConsensusState {
    pub fn new(metrics: Arc<ConsensusMetrics>) -> Self {
        Self {
            last_committed_round: 0,
            last_committed: Default::default(),
            latest_sub_dag_index: 0,
            dag: Default::default(),
            last_consensus_reputation_score: ReputationScores::default(),
            last_committed_leader: None,
            metrics,
        }
    }

    pub fn new_from_store(
        metrics: Arc<ConsensusMetrics>,
        last_committed_round: Round,
        gc_depth: Round,
        recovered_last_committed: HashMap<PublicKey, Round>,
        latest_sub_dag: Option<CommittedSubDagShell>,
        cert_store: CertificateStore,
    ) -> Self {
        let gc_round = last_committed_round.saturating_sub(gc_depth);
        let dag =
            Self::construct_dag_from_cert_store(cert_store, gc_round, &recovered_last_committed)
                .expect("error when recovering DAG from store");
        metrics.recovered_consensus_state.inc();

        let (latest_sub_dag_index, last_consensus_reputation_score, last_committed_leader) =
            latest_sub_dag
                .map(|s| (s.sub_dag_index, s.reputation_score, Some(s.leader)))
                .unwrap_or_default();

        Self {
            last_committed_round,
            last_committed: recovered_last_committed,
            last_consensus_reputation_score,
            latest_sub_dag_index,
            last_committed_leader,
            dag,
            metrics,
        }
    }

    #[instrument(level = "info", skip_all)]
    pub fn construct_dag_from_cert_store(
        cert_store: CertificateStore,
        gc_round: Round,
        last_committed: &HashMap<PublicKey, Round>,
    ) -> Result<Dag, ConsensusError> {
        let mut dag: Dag = BTreeMap::new();

        info!("Recreating dag from last GC round: {}", gc_round,);

        // get all certificates at rounds > gc_round
        let certificates = cert_store.after_round(gc_round + 1).unwrap();

        let mut num_certs = 0;
        for cert in &certificates {
            if Self::try_insert_in_dag(&mut dag, last_committed, cert)? {
                info!("Inserted certificate: {:?}", cert);
                num_certs += 1;
            }
        }
        info!(
            "Dag is restored and contains {} certs for {} rounds",
            num_certs,
            dag.len()
        );

        Ok(dag)
    }

    /// Returns true if certificate is inserted in the dag.
    pub fn try_insert(&mut self, certificate: &Certificate) -> Result<bool, ConsensusError> {
        Self::try_insert_in_dag(&mut self.dag, &self.last_committed, certificate)
    }

    /// Returns true if certificate is inserted in the dag.
    fn try_insert_in_dag(
        dag: &mut Dag,
        last_committed: &HashMap<PublicKey, Round>,
        certificate: &Certificate,
    ) -> Result<bool, ConsensusError> {
        let origin_last_committed_round = last_committed
            .get(&certificate.origin())
            .cloned()
            .unwrap_or_default();
        if certificate.round() <= origin_last_committed_round {
            debug!(
                "Ignoring certificate {:?} as it is at or before last committed round {} for this origin",
                certificate, origin_last_committed_round
            );
            return Ok(false);
        }

        if let Some((_, existing_certificate)) = dag.entry(certificate.round()).or_default().insert(
            certificate.origin(),
            (certificate.digest(), certificate.clone()),
        ) {
            // we want to error only if we try to insert a different certificate in the dag
            if existing_certificate.digest() != certificate.digest() {
                return Err(ConsensusError::CertificateEquivocation(
                    certificate.clone(),
                    existing_certificate,
                ));
            }
        }
        Ok(true)
    }

    /// Update and clean up internal state after committing a certificate.
    pub fn update(&mut self, certificate: &Certificate, gc_depth: Round) {
        self.last_committed
            .entry(certificate.origin())
            .and_modify(|r| *r = max(*r, certificate.round()))
            .or_insert_with(|| certificate.round());
        self.last_committed_round = max(self.last_committed_round, certificate.round());

        self.metrics
            .last_committed_round
            .with_label_values(&[])
            .set(self.last_committed_round as i64);
        let elapsed = certificate.metadata.created_at.elapsed().as_secs_f64();
        self.metrics
            .certificate_commit_latency
            .observe(certificate.metadata.created_at.elapsed().as_secs_f64());

        // NOTE: This log entry is used to compute performance.
        tracing::debug!(
            "Certificate {:?} took {} seconds to be committed at round {}",
            certificate.digest(),
            elapsed,
            certificate.round(),
        );

        // Purge all certificates past the gc depth.
        self.dag
            .retain(|r, _| r + gc_depth >= self.last_committed_round);
        // Also purge this certificate, and other certificates at the same origin below its round.
        self.dag.retain(|r, authorities| {
            if r <= &certificate.round() {
                authorities.remove(&certificate.origin());
                !authorities.is_empty()
            } else {
                true
            }
        });
    }
}

/// Describe how to sequence input certificates.
pub trait ConsensusProtocol {
    fn process_certificate(
        &mut self,
        // The state of the consensus protocol.
        state: &mut ConsensusState,
        // The new certificate.
        certificate: Certificate,
    ) -> Result<(Outcome, Vec<CommittedSubDag>), ConsensusError>;
}

pub struct Consensus<ConsensusProtocol> {
    /// The committee information.
    committee: Committee,

    /// Receiver for shutdown.
    rx_shutdown: ConditionalBroadcastReceiver,
    /// Receives new certificates from the primary. The primary should send us new certificates only
    /// if it already sent us its whole history.
    rx_new_certificates: metered_channel::Receiver<Certificate>,
    /// Outputs the sequence of ordered certificates to the primary (for cleanup and feedback).
    tx_committed_certificates: metered_channel::Sender<(Round, Vec<Certificate>)>,
    /// Outputs the highest committed round in the consensus. Controls GC round downstream.
    tx_consensus_round_updates: watch::Sender<Round>,
    /// Outputs the sequence of ordered certificates to the application layer.
    tx_sequence: metered_channel::Sender<CommittedSubDag>,

    /// The consensus protocol to run.
    protocol: ConsensusProtocol,

    /// Metrics handler
    metrics: Arc<ConsensusMetrics>,

    /// Inner state
    state: ConsensusState,
}

impl<Protocol> Consensus<Protocol>
where
    Protocol: ConsensusProtocol + Send + 'static,
{
    #[must_use]
    pub fn spawn(
        committee: Committee,
        gc_depth: Round,
        store: Arc<ConsensusStore>,
        cert_store: CertificateStore,
        rx_shutdown: ConditionalBroadcastReceiver,
        rx_new_certificates: metered_channel::Receiver<Certificate>,
        tx_committed_certificates: metered_channel::Sender<(Round, Vec<Certificate>)>,
        tx_consensus_round_updates: watch::Sender<Round>,
        tx_sequence: metered_channel::Sender<CommittedSubDag>,
        protocol: Protocol,
        metrics: Arc<ConsensusMetrics>,
    ) -> JoinHandle<()> {
        // The consensus state (everything else is immutable).
        let recovered_last_committed = store.read_last_committed();
        let last_committed_round = recovered_last_committed
            .iter()
            .max_by(|a, b| a.1.cmp(b.1))
            .map(|(_k, v)| *v)
            .unwrap_or_else(|| 0);
        let latest_sub_dag = store.get_latest_sub_dag();
        if let Some(sub_dag) = &latest_sub_dag {
            assert_eq!(
                sub_dag.leader_round, last_committed_round,
                "Last subdag leader round {} is not equal to the last committed round {}!",
                sub_dag.leader_round, last_committed_round,
            );
        }

        let state = ConsensusState::new_from_store(
            metrics.clone(),
            last_committed_round,
            gc_depth,
            recovered_last_committed,
            latest_sub_dag,
            cert_store,
        );

        tx_consensus_round_updates
            .send(state.last_committed_round)
            .expect("Failed to send last_committed_round on initialization!");

        let s = Self {
            committee,
            rx_shutdown,
            rx_new_certificates,
            tx_committed_certificates,
            tx_consensus_round_updates,
            tx_sequence,
            protocol,
            metrics,
            state,
        };

        spawn_logged_monitored_task!(s.run(), "Consensus", INFO)
    }

    async fn run(self) {
        match self.run_inner().await {
            Ok(_) => {}
            Err(err @ ConsensusError::ShuttingDown) => {
                debug!("{:?}", err)
            }
            Err(err) => panic!("Failed to run consensus: {:?}", err),
        }
    }

    async fn run_inner(mut self) -> Result<(), ConsensusError> {
        // Listen to incoming certificates.
        'main: loop {
            tokio::select! {

                _ = self.rx_shutdown.receiver.recv() => {
                    return Ok(())
                }

                Some(certificate) = self.rx_new_certificates.recv() => {
                    match certificate.epoch().cmp(&self.committee.epoch()) {
                        Ordering::Equal => {
                            // we can proceed.
                        }
                        _ => {
                            tracing::debug!("Already moved to the next epoch");
                            continue 'main;
                        }
                    }

                    // Process the certificate using the selected consensus protocol.
                    let (_, committed_sub_dags) = self.protocol.process_certificate(&mut self.state, certificate)?;

                    // We extract a list of headers from this specific validator that
                    // have been agreed upon, and signal this back to the narwhal sub-system
                    // to be used to re-send batches that have not made it to a commit.
                    let mut commited_certificates = Vec::new();

                    // Output the sequence in the right order.
                    let mut i = 0;
                    for committed_sub_dag in committed_sub_dags {
                         tracing::debug!("Commit in Sequence {:?}", committed_sub_dag.sub_dag_index);

                        for certificate in &committed_sub_dag.certificates {
                            i+=1;

                            if i % 5_000 == 0 {
                                #[cfg(not(feature = "benchmark"))]
                                tracing::debug!("Committed {}", certificate.header);
                            }

                            #[cfg(feature = "benchmark")]
                            for digest in certificate.header.payload.keys() {
                                // NOTE: This log entry is used to compute performance.
                                tracing::info!("Committed {} -> {:?}", certificate.header, digest);
                            }

                            commited_certificates.push(certificate.clone());
                        }

                        // NOTE: The size of the sub-dag can be arbitrarily large (depending on the network condition
                        // and Byzantine leaders).
                        self.tx_sequence.send(committed_sub_dag).await.map_err(|_|ConsensusError::ShuttingDown)?;
                    }

                    if !commited_certificates.is_empty(){
                        // Highest committed certificate round is the leader round / commit round
                        // expected by primary.
                        let leader_commit_round = commited_certificates.iter().map(|c| c.round()).max().unwrap();

                        self.tx_committed_certificates
                        .send((leader_commit_round, commited_certificates))
                        .await
                        .map_err(|_|ConsensusError::ShuttingDown)?;

                        self.tx_consensus_round_updates.send(leader_commit_round)
                        .map_err(|_|ConsensusError::ShuttingDown)?;
                    }

                    self.metrics
                        .consensus_dag_rounds
                        .with_label_values(&[])
                        .set(self.state.dag.len() as i64);
                },

            }
        }
    }
}
