// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::mutable_key_type)]

use crate::{metrics::ConsensusMetrics, SequenceNumber};
use config::Committee;
use crypto::PublicKey;
use fastcrypto::hash::Hash;
use mysten_metrics::spawn_logged_monitored_task;
use std::{
    cmp::{max, Ordering},
    collections::HashMap,
    sync::Arc,
};
use storage::CertificateStore;
use tokio::{sync::watch, task::JoinHandle};
use tracing::{debug, info, instrument};
use types::{
    metered_channel, Certificate, CertificateDigest, CommittedSubDag, ConditionalBroadcastReceiver,
    ConsensusStore, Round, StoreResult, Timestamp,
};

#[cfg(test)]
#[path = "tests/consensus_tests.rs"]
pub mod consensus_tests;

/// The representation of the DAG in memory.
pub type Dag = HashMap<Round, HashMap<PublicKey, (CertificateDigest, Certificate)>>;

/// The state that needs to be persisted for crash-recovery.
pub struct ConsensusState {
    /// The last committed round.
    pub last_committed_round: Round,
    /// Keeps the last committed round for each authority. This map is used to clean up the dag and
    /// ensure we don't commit twice the same certificate.
    pub last_committed: HashMap<PublicKey, Round>,
    /// Used to populate the index in the sub-dag construction.
    pub latest_sub_dag_index: SequenceNumber,
    /// Keeps the latest committed certificate (and its parents) for every authority. Anything older
    /// must be regularly cleaned up through the function `update`.
    pub dag: Dag,
    /// Metrics handler
    pub metrics: Arc<ConsensusMetrics>,
}

impl ConsensusState {
    pub fn new(genesis: Vec<Certificate>, metrics: Arc<ConsensusMetrics>) -> Self {
        let genesis = genesis
            .into_iter()
            .map(|x| (x.origin(), (x.digest(), x)))
            .collect::<HashMap<_, _>>();

        Self {
            last_committed_round: 0,
            last_committed: genesis
                .iter()
                .map(|(x, (_, y))| (x.clone(), y.round()))
                .collect(),
            latest_sub_dag_index: 0,
            dag: [(0, genesis)]
                .iter()
                .cloned()
                .collect::<HashMap<_, HashMap<_, _>>>(),
            metrics,
        }
    }

    pub fn new_from_store(
        genesis: Vec<Certificate>,
        metrics: Arc<ConsensusMetrics>,
        recover_last_committed: HashMap<PublicKey, Round>,
        latest_sub_dag_index: SequenceNumber,
        cert_store: CertificateStore,
        gc_depth: Round,
    ) -> Self {
        let last_committed_round = *recover_last_committed
            .iter()
            .max_by(|a, b| a.1.cmp(b.1))
            .map(|(_k, v)| v)
            .unwrap_or_else(|| &0);

        if last_committed_round == 0 {
            return Self::new(genesis, metrics);
        }
        metrics.recovered_consensus_state.inc();

        let dag = Self::construct_dag_from_cert_store(
            cert_store,
            last_committed_round,
            &recover_last_committed,
            gc_depth,
        );

        Self {
            last_committed_round,
            last_committed: recover_last_committed,
            latest_sub_dag_index,
            dag,
            metrics,
        }
    }

    #[instrument(level = "info", skip_all)]
    pub fn construct_dag_from_cert_store(
        cert_store: CertificateStore,
        last_committed_round: Round,
        last_committed: &HashMap<PublicKey, Round>,
        gc_depth: Round,
    ) -> Dag {
        let mut dag: Dag = HashMap::new();
        let min_round = last_committed_round.saturating_sub(gc_depth);

        info!(
            "Recreating dag from last committed round: {}, min_round: {}",
            last_committed_round, min_round
        );

        // get all certificates at a round > min_round
        let certificates = cert_store.after_round(min_round + 1).unwrap();

        let mut num_certs = 0;
        for cert in &certificates {
            if Self::try_insert_in_dag(&mut dag, last_committed, cert).is_ok() {
                info!("Inserted certificate: {:?}", cert);
                num_certs += 1;
            }
        }
        info!(
            "Dag was restored and contains {} certs for {} rounds",
            num_certs,
            dag.len()
        );

        dag
    }

    #[allow(clippy::result_unit_err)]
    pub fn try_insert(&mut self, certificate: &Certificate) -> Result<(), ()> {
        Self::try_insert_in_dag(&mut self.dag, &self.last_committed, certificate)
    }

    #[allow(clippy::result_unit_err)]
    fn try_insert_in_dag(
        dag: &mut Dag,
        last_committed: &HashMap<PublicKey, Round>,
        certificate: &Certificate,
    ) -> Result<(), ()> {
        let last_committed_round = last_committed
            .get(&certificate.origin())
            .cloned()
            .unwrap_or_default();
        if certificate.round() < last_committed_round {
            debug!(
                "Ignoring certificate {:?} as it is past last committed round for this origin {}",
                certificate, last_committed_round
            );
            return Err(());
        }

        dag.entry(certificate.round())
            .or_insert_with(HashMap::new)
            .insert(
                certificate.origin(),
                (certificate.digest(), certificate.clone()),
            );
        Ok(())
    }

    /// Update and clean up internal state base on committed certificates.
    pub fn update(&mut self, certificate: &Certificate, gc_depth: Round) {
        self.last_committed
            .entry(certificate.origin())
            .and_modify(|r| *r = max(*r, certificate.round()))
            .or_insert_with(|| certificate.round());

        let last_committed_round = *Iterator::max(self.last_committed.values()).unwrap();
        self.last_committed_round = last_committed_round;

        self.metrics
            .last_committed_round
            .with_label_values(&[])
            .set(last_committed_round as i64);

        // NOTE: This log entry is used to compute performance.
        tracing::debug!(
            "Certificate {:?} took {} seconds to be committed at round {}",
            certificate.digest(),
            certificate.metadata.created_at.elapsed().as_secs_f64(),
            last_committed_round
        );

        self.metrics
            .certificate_commit_latency
            .observe(certificate.metadata.created_at.elapsed().as_secs_f64());

        // We purge all certificates past the gc depth
        self.dag.retain(|r, _| r + gc_depth >= last_committed_round);
        for (name, round) in &self.last_committed {
            self.dag.retain(|r, authorities| {
                // We purge certificates for `name` prior to its latest commit
                if r < round {
                    authorities.remove(name);
                }
                !authorities.is_empty()
            });
        }
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
    ) -> StoreResult<Vec<CommittedSubDag>>;

    fn update_committee(&mut self, new_committee: Committee) -> StoreResult<()>;
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
        store: Arc<ConsensusStore>,
        cert_store: CertificateStore,
        rx_shutdown: ConditionalBroadcastReceiver,
        rx_new_certificates: metered_channel::Receiver<Certificate>,
        tx_committed_certificates: metered_channel::Sender<(Round, Vec<Certificate>)>,
        tx_consensus_round_updates: watch::Sender<Round>,
        tx_sequence: metered_channel::Sender<CommittedSubDag>,
        protocol: Protocol,
        metrics: Arc<ConsensusMetrics>,
        gc_depth: Round,
    ) -> JoinHandle<()> {
        // The consensus state (everything else is immutable).
        let genesis = Certificate::genesis(&committee);
        let recovered_last_committed = store.read_last_committed();
        let latest_sub_dag_index = store.get_latest_sub_dag_index();
        let state = ConsensusState::new_from_store(
            genesis,
            metrics.clone(),
            recovered_last_committed,
            latest_sub_dag_index,
            cert_store,
            gc_depth,
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
        self.run_inner().await.expect("Failed to run consensus")
    }

    async fn run_inner(mut self) -> StoreResult<()> {
        // Listen to incoming certificates.
        loop {
            tokio::select! {

                _ = self.rx_shutdown.receiver.recv() => {
                    return Ok(())
                }

                Some(certificate) = self.rx_new_certificates.recv() => {
                    // If the core already moved to the next epoch we should pull the next
                    // committee as well.
                    match certificate.epoch().cmp(&self.committee.epoch()) {

                        Ordering::Equal => {
                            // we can proceed.
                        }
                        _ => {
                            tracing::debug!("Already moved to the next epoch");
                            continue
                        }
                    }

                    // Process the certificate using the selected consensus protocol.
                    let committed_sub_dags =
                        self.protocol
                            .process_certificate(&mut self.state, certificate)?;


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

                            #[cfg(not(feature = "benchmark"))]
                            if i % 5_000 == 0 {
                                tracing::debug!("Committed {}", certificate.header);
                            }

                            #[cfg(feature = "benchmark")]
                            for digest in certificate.header.payload.keys() {
                                // NOTE: This log entry is used to compute performance.
                                tracing::info!("Committed {} -> {:?}", certificate.header, digest);
                            }

                            // Update DAG size metric periodically to limit computation cost.
                            // TODO: this should be triggered on collection when library support for
                            // closure metrics is available.
                            if i % 1_000 == 0 {
                                self.metrics
                                    .dag_size_bytes
                                    .set((mysten_util_mem::malloc_size(&self.state.dag) + std::mem::size_of::<Dag>()) as i64);
                            }

                            commited_certificates.push(certificate.clone());
                        }

                        // NOTE: The size of the sub-dag can be arbitrarily large (depending on the network condition
                        // and Byzantine leaders).
                        if let Err(e) = self.tx_sequence.send(committed_sub_dag).await {
                            tracing::warn!("Failed to output sub dag: {e}");
                        }
                    }

                    if !commited_certificates.is_empty(){
                        // Highest committed certificate round is the leader round / commit round
                        // expected by primary.
                        let leader_commit_round = commited_certificates.iter().map(|c| c.round()).max().unwrap();
                        self.tx_committed_certificates
                        .send((leader_commit_round, commited_certificates))
                        .await
                        .expect("Failed to send committed round and certificates to primary");
                        self.tx_consensus_round_updates.send(leader_commit_round).expect("Failed to notify primary about committed round!");
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
