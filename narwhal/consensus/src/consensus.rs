// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{metrics::ConsensusMetrics, ConsensusOutput, SequenceNumber};
use config::Committee;
use crypto::{traits::VerifyingKey, Hash};
use std::{
    cmp::{max, Ordering},
    collections::HashMap,
    sync::Arc,
};
use store::Store;
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        watch,
    },
    task::JoinHandle,
};
use tracing::{info, instrument};
use types::{
    Certificate, CertificateDigest, ConsensusStore, ReconfigureNotification, Round, StoreResult,
};

/// The representation of the DAG in memory.
pub type Dag<PublicKey> =
    HashMap<Round, HashMap<PublicKey, (CertificateDigest, Certificate<PublicKey>)>>;

/// The state that needs to be persisted for crash-recovery.
pub struct ConsensusState<PublicKey: VerifyingKey> {
    /// The last committed round.
    pub last_committed_round: Round,
    // Keeps the last committed round for each authority. This map is used to clean up the dag and
    // ensure we don't commit twice the same certificate.
    pub last_committed: HashMap<PublicKey, Round>,
    /// Keeps the latest committed certificate (and its parents) for every authority. Anything older
    /// must be regularly cleaned up through the function `update`.
    pub dag: Dag<PublicKey>,
    /// Metrics handler
    pub metrics: Arc<ConsensusMetrics>,
}

impl<PublicKey: VerifyingKey> ConsensusState<PublicKey> {
    pub fn new(genesis: Vec<Certificate<PublicKey>>, metrics: Arc<ConsensusMetrics>) -> Self {
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
            dag: [(0, genesis)]
                .iter()
                .cloned()
                .collect::<HashMap<_, HashMap<_, _>>>(),
            metrics,
        }
    }

    #[instrument(level = "info", skip_all)]
    pub async fn new_from_store(
        genesis: Vec<Certificate<PublicKey>>,
        metrics: Arc<ConsensusMetrics>,
        recover_last_committed: HashMap<PublicKey, Round>,
        cert_store: Store<CertificateDigest, Certificate<PublicKey>>,
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

        let dag =
            Self::construct_dag_from_cert_store(cert_store, last_committed_round, gc_depth).await;

        Self {
            last_committed_round,
            last_committed: recover_last_committed,
            dag,
            metrics,
        }
    }

    #[instrument(level = "info", skip_all)]
    pub async fn construct_dag_from_cert_store(
        cert_store: Store<CertificateDigest, Certificate<PublicKey>>,
        last_committed_round: Round,
        gc_depth: Round,
    ) -> Dag<PublicKey> {
        let mut dag: Dag<PublicKey> = HashMap::new();
        info!(
            "Recreating dag from last committed round: {}",
            last_committed_round
        );

        let min_round = last_committed_round.saturating_sub(gc_depth);
        let cert_map = cert_store
            .iter(Some(Box::new(move |(_dig, cert)| {
                cert.header.round > min_round
            })))
            .await;

        let num_certs = cert_map.len();
        for (digest, cert) in cert_map {
            let inner = dag.get_mut(&cert.header.round);
            match inner {
                Some(m) => {
                    m.insert(cert.header.author.clone(), (digest, cert.clone()));
                }
                None => {
                    dag.entry(cert.header.round)
                        .or_insert_with(|| HashMap::new())
                        .insert(cert.header.author.clone(), (digest, cert.clone()));
                }
            }
        }
        info!(
            "Dag was restored and contains {} certs for {} rounds",
            num_certs,
            dag.len()
        );

        dag
    }

    /// Update and clean up internal state base on committed certificates.
    pub fn update(&mut self, certificate: &Certificate<PublicKey>, gc_depth: Round) {
        self.last_committed
            .entry(certificate.origin())
            .and_modify(|r| *r = max(*r, certificate.round()))
            .or_insert_with(|| certificate.round());

        let last_committed_round = *std::iter::Iterator::max(self.last_committed.values()).unwrap();
        self.last_committed_round = last_committed_round;

        self.metrics
            .last_committed_round
            .with_label_values(&[])
            .set(last_committed_round as i64);

        // We purge all certificates past the gc depth
        self.dag.retain(|r, _| r + gc_depth >= last_committed_round);
        for (name, round) in &self.last_committed {
            self.dag.retain(|r, authorities| {
                // We purge certificates for `name` prior to its latest commit
                if r < round {
                    authorities.retain(|n, _| n != name);
                }
                !authorities.is_empty()
            });
        }

        self.metrics
            .consensus_dag_size
            .with_label_values(&[])
            .set(self.dag.len() as i64);
    }
}

/// Describe how to sequence input certificates.
pub trait ConsensusProtocol<PublicKey: VerifyingKey> {
    fn process_certificate(
        &mut self,
        // The state of the consensus protocol.
        state: &mut ConsensusState<PublicKey>,
        // The latest consensus index.
        consensus_index: SequenceNumber,
        // The new certificate.
        certificate: Certificate<PublicKey>,
    ) -> StoreResult<Vec<ConsensusOutput<PublicKey>>>;

    fn update_committee(&mut self, new_committee: Committee<PublicKey>) -> StoreResult<()>;
}

pub struct Consensus<PublicKey: VerifyingKey, ConsensusProtocol> {
    /// The committee information.
    committee: Committee<PublicKey>,

    /// Receive reconfiguration update.
    rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
    /// Receives new certificates from the primary. The primary should send us new certificates only
    /// if it already sent us its whole history.
    rx_primary: Receiver<Certificate<PublicKey>>,
    /// Outputs the sequence of ordered certificates to the primary (for cleanup and feedback).
    tx_primary: Sender<Certificate<PublicKey>>,
    /// Outputs the sequence of ordered certificates to the application layer.
    tx_output: Sender<ConsensusOutput<PublicKey>>,

    /// The (global) consensus index. We assign one index to each sequenced certificate. this is
    /// helpful for clients.
    consensus_index: SequenceNumber,

    /// The consensus protocol to run.
    protocol: ConsensusProtocol,

    /// Metrics handler
    metrics: Arc<ConsensusMetrics>,
}

impl<PublicKey, Protocol> Consensus<PublicKey, Protocol>
where
    PublicKey: VerifyingKey,
    Protocol: ConsensusProtocol<PublicKey> + Send + 'static,
{
    pub fn spawn(
        committee: Committee<PublicKey>,
        store: Arc<ConsensusStore<PublicKey>>,
        cert_store: Store<CertificateDigest, Certificate<PublicKey>>,
        rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
        rx_primary: Receiver<Certificate<PublicKey>>,
        tx_primary: Sender<Certificate<PublicKey>>,
        tx_output: Sender<ConsensusOutput<PublicKey>>,
        protocol: Protocol,
        metrics: Arc<ConsensusMetrics>,
        gc_depth: Round,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let consensus_index = store
                .read_last_consensus_index()
                .expect("Failed to load consensus index from store");
            let recovered_last_committed = store.read_last_committed();
            Self {
                committee,
                rx_reconfigure,
                rx_primary,
                tx_primary,
                tx_output,
                consensus_index,
                protocol,
                metrics,
            }
            .run(recovered_last_committed, cert_store, gc_depth)
            .await
            .expect("Failed to run consensus")
        })
    }

    fn reconfigure(
        &mut self,
        new_committee: Committee<PublicKey>,
    ) -> StoreResult<ConsensusState<PublicKey>> {
        self.committee = new_committee.clone();
        self.protocol.update_committee(new_committee)?;
        tracing::debug!("Committee updated to {}", self.committee);

        self.consensus_index = 0;

        let genesis = Certificate::genesis(&self.committee);
        Ok(ConsensusState::new(genesis, self.metrics.clone()))
    }

    async fn run(
        &mut self,
        recover_last_committed: HashMap<PublicKey, Round>,
        cert_store: Store<CertificateDigest, Certificate<PublicKey>>,
        gc_depth: Round,
    ) -> StoreResult<()> {
        // The consensus state (everything else is immutable).
        let genesis = Certificate::genesis(&self.committee);
        let mut state = ConsensusState::new_from_store(
            genesis,
            self.metrics.clone(),
            recover_last_committed,
            cert_store,
            gc_depth,
        )
        .await;

        // Listen to incoming certificates.
        loop {
            tokio::select! {
                Some(certificate) = self.rx_primary.recv() => {
                    // If the core already moved to the next epoch we should pull the next
                    // committee as well.
                    match certificate.epoch().cmp(&self.committee.epoch()) {
                        Ordering::Greater => {
                            let message = self.rx_reconfigure.borrow_and_update().clone();
                            match message  {
                                ReconfigureNotification::NewCommittee(new_committee) => {
                                    state = self.reconfigure(new_committee)?;
                                },
                                ReconfigureNotification::Shutdown => return Ok(()),
                            }
                        }
                        Ordering::Less => {
                            // We already updated committee but the core is slow.
                            tracing::debug!("Already moved to the next epoch");
                            continue
                        },
                        Ordering::Equal => {
                            // Nothing to do, we can proceed.
                        }
                    }

                    // Process the certificate using the selected consensus protocol.
                    let sequence =
                        self.protocol
                            .process_certificate(&mut state, self.consensus_index, certificate)?;

                    // Update the consensus index.
                    self.consensus_index += sequence.len() as u64;

                    // Output the sequence in the right order.
                    for output in sequence {
                        let certificate = &output.certificate;
                        #[cfg(not(feature = "benchmark"))]
                        if output.consensus_index % 5_000 == 0 {
                            tracing::debug!("Committed {}", certificate.header);
                        }

                        #[cfg(feature = "benchmark")]
                        for digest in certificate.header.payload.keys() {
                            // NOTE: This log entry is used to compute performance.
                            tracing::info!("Committed {} -> {:?}", certificate.header, digest);
                        }

                        self.tx_primary
                            .send(certificate.clone())
                            .await
                            .expect("Failed to send certificate to primary");

                        if let Err(e) = self.tx_output.send(output).await {
                            tracing::warn!("Failed to output certificate: {e}");
                        }
                    }
                },

                // Check whether the committee changed.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    match message {
                        ReconfigureNotification::NewCommittee(new_committee) => {
                            state = self.reconfigure(new_committee)?;
                        },
                        ReconfigureNotification::Shutdown => return Ok(())
                    }
                }
            }
        }
    }
}
