// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{Committee, SharedCommittee, SharedWorkerCache, WorkerId};
use consensus::dag::Dag;
use crypto::PublicKey;
use fastcrypto::hash::Hash as _;
use network::{anemo_ext::NetworkExt, RetryConfig};
use parking_lot::Mutex;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use storage::{CertificateStore, PayloadToken};
use store::Store;
use tokio::sync::{broadcast, watch};
use tracing::{debug, error, trace, warn};
use types::{
    ensure,
    error::{DagError, DagResult},
    metered_channel::Sender,
    BatchDigest, Certificate, CertificateDigest, Header, PrimaryToWorkerClient, Round,
    WorkerSynchronizeMessage,
};

use crate::metrics::PrimaryMetrics;

#[cfg(test)]
#[path = "tests/synchronizer_tests.rs"]
pub mod synchronizer_tests;

struct SuspendedCertificate {
    certificate: Option<Certificate>,
    missing_parents: HashSet<CertificateDigest>,
    accepted: broadcast::Sender<()>,
}

#[derive(Default)]
struct Inner {
    highest_accepted_round: Round,
    highest_received_round: Round,
    suspened: HashMap<CertificateDigest, SuspendedCertificate>,
    missing: HashMap<CertificateDigest, HashSet<CertificateDigest>>,
}

impl Inner {
    fn new(highest_accepted_round: Round) -> Self {
        Self {
            highest_accepted_round,
            ..Default::default()
        }
    }
}

/// The `Synchronizer` provides functions for retrieving missing certificates and batches.
pub struct Synchronizer {
    /// The public key of this primary.
    name: PublicKey,
    /// Committee of the current epoch.
    committee: Committee,
    /// The worker information cache.
    worker_cache: SharedWorkerCache,
    /// The depth of the garbage collector.
    gc_depth: Round,
    /// Temporary storage for certificates that cannot be accepted yet.
    inner: Mutex<Inner>,
    /// The persistent storage.
    certificate_store: CertificateStore,
    payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
    /// Send commands to the `CertificateFetcher`.
    tx_certificate_fetcher: Sender<Certificate>,
    /// Get a signal when the round changes.
    rx_consensus_round_updates: watch::Receiver<Round>,
    /// The genesis and its digests.
    genesis: HashMap<CertificateDigest, Certificate>,
    /// The dag used for the external consensus
    dag: Option<Arc<Dag>>,
    /// Contains Synchronizer specific metrics among other Primary metrics.
    metrics: Arc<PrimaryMetrics>,
}

impl Synchronizer {
    pub fn new(
        name: PublicKey,
        committee: SharedCommittee,
        worker_cache: SharedWorkerCache,
        gc_depth: Round,
        certificate_store: CertificateStore,
        payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
        tx_certificate_fetcher: Sender<Certificate>,
        rx_consensus_round_updates: watch::Receiver<Round>,
        dag: Option<Arc<Dag>>,
        metrics: Arc<PrimaryMetrics>,
    ) -> Self {
        let committee: &Committee = &committee.load();
        let genesis = Self::make_genesis(committee);
        let highest_accepted_round = certificate_store.highest_round_number();
        Self {
            name,
            committee: committee.clone(),
            worker_cache,
            gc_depth,
            inner: Mutex::new(Inner::new(highest_accepted_round)),
            certificate_store,
            payload_store,
            tx_certificate_fetcher,
            rx_consensus_round_updates,
            genesis,
            dag,
            metrics,
        }
    }

    fn make_genesis(committee: &Committee) -> HashMap<CertificateDigest, Certificate> {
        Certificate::genesis(committee)
            .into_iter()
            .map(|x| (x.digest(), x))
            .collect()
    }

    async fn process_certificate(&mut self, certificate: Certificate) -> DagResult<()> {
        let digest = certificate.digest();
        if self.certificate_store.read(digest)?.is_some() {
            trace!("Certificate {digest:?} has already been processed. Skip processing.");
            self.metrics.duplicate_certificates_processed.inc();
            return Ok(());
        }

        self.sanitize_certificate(&certificate).await?;
        let result = self.process_certificate_internal(certificate).await;
        match result {
            Err(DagError::Suspended) => {
                // if let Some(notify) = notify {
                //     self.pending_certificates
                //         .entry(digest)
                //         .or_insert_with(Vec::new)
                //         .push(notify);
                // }
                Ok(())
            }
            result => {
                // if let Some(notifies) = self.pending_certificates.remove(&digest) {
                //     for notify in notifies {
                //         let _ = notify.send(result.clone()); // no problem if remote side isn't listening
                //     }
                // }
                result
            }
        }
    }

    async fn sanitize_certificate(&mut self, certificate: &Certificate) -> DagResult<()> {
        ensure!(
            self.committee.epoch() == certificate.epoch(),
            DagError::InvalidEpoch {
                expected: self.committee.epoch(),
                received: certificate.epoch()
            }
        );
        // Ok to drop old certificate, because it will never be included into the consensus dag.
        let gc_round = *self.rx_consensus_round_updates.borrow() - self.gc_depth;
        ensure!(
            gc_round < certificate.round(),
            DagError::TooOld(certificate.digest().into(), certificate.round(), gc_round)
        );
        // Verify the certificate (and the embedded header).
        certificate
            .verify(&self.committee, self.worker_cache.clone())
            .map_err(DagError::from)
    }

    // Logs Core errors as appropriate.
    fn process_result(result: &DagResult<()>) {
        match result {
            Ok(()) => (),
            Err(DagError::StoreError(e)) => {
                error!("{e}");
                panic!("Storage failure: killing node.");
            }
            Err(
                e @ DagError::TooOld(..)
                | e @ DagError::VoteTooOld(..)
                | e @ DagError::InvalidEpoch { .. },
            ) => debug!("{e}"),
            Err(e) => warn!("{e}"),
        }
    }

    async fn process_certificate_internal(&mut self, certificate: Certificate) -> DagResult<()> {
        Ok(())
    }

    // async fn process_certificate_internal(&mut self, certificate: Certificate) -> DagResult<()> {
    //     debug!(
    //         "Processing certificate {:?} round:{:?}",
    //         certificate,
    //         certificate.round()
    //     );

    //     let certificate_source = if self.name.eq(&certificate.header.author) {
    //         "own"
    //     } else {
    //         "other"
    //     };
    //     self.highest_received_round = self.highest_received_round.max(certificate.round());
    //     self.metrics
    //         .highest_received_round
    //         .with_label_values(&[certificate_source])
    //         .set(self.highest_received_round as i64);

    //     // Let the proposer draw early conclusions from a certificate at this round and epoch, without its
    //     // parents or payload (which we may not have yet).
    //     //
    //     // Since our certificate is well-signed, it shows a majority of honest signers stand at round r,
    //     // so to make a successful proposal, our proposer must use parents at least at round r-1.
    //     //
    //     // This allows the proposer not to fire proposals at rounds strictly below the certificate we witnessed.
    //     let minimal_round_for_parents = certificate.round().saturating_sub(1);
    //     self.tx_parents
    //         .send((vec![], minimal_round_for_parents, certificate.epoch()))
    //         .await
    //         .map_err(|_| DagError::ShuttingDown)?;

    //     // Instruct workers to download any missing batches referenced in this certificate.
    //     // Since this header got certified, we are sure that all the data it refers to (ie. its batches and its parents) are available.
    //     // We can thus continue the processing of the certificate without blocking on batch synchronization.
    //     let header = certificate.header.clone();
    //     let network = self.network.clone();
    //     let max_age = self.gc_depth.saturating_sub(1);
    //     self.background_tasks
    //         .spawn(async move { self.sync_batches(&header, network, max_age).await });

    //     // Ensure either we have all the ancestors of this certificate, or the parents have been garbage collected.
    //     // If we don't, the synchronizer will start fetching missing certificates.
    //     if certificate.round() > self.gc_round + 1 && !self.check_parents(&certificate).await? {
    //         debug!(
    //             "Processing certificate {:?} suspended: missing ancestors",
    //             certificate
    //         );
    //         self.metrics
    //             .certificates_suspended
    //             .with_label_values(&["missing_parents"])
    //             .inc();
    //         return Err(DagError::Suspended);
    //     }

    //     // Store the certificate. Afterwards, the certificate must be sent to consensus
    //     // or Narwhal needs to shutdown, to avoid insistencies certificate store and
    //     // consensus dag.
    //     self.certificate_store.write(certificate.clone())?;

    //     // Update metrics for processed certificates.
    //     self.highest_processed_round = self.highest_processed_round.max(certificate.round());
    //     self.metrics
    //         .highest_processed_round
    //         .with_label_values(&[certificate_source])
    //         .set(self.highest_processed_round as i64);
    //     self.metrics
    //         .certificates_processed
    //         .with_label_values(&[certificate_source])
    //         .inc();

    //     // Append the certificate to the aggregator of the
    //     // corresponding round.
    //     let digest = certificate.digest();
    //     if let Err(e) = self
    //         .append_certificate_in_aggregator(certificate.clone())
    //         .await
    //     {
    //         warn!(
    //             "Failed to aggregate certificate {} for header: {}",
    //             digest, e
    //         );
    //         return Err(DagError::ShuttingDown);
    //     }

    //     // Send it to the consensus layer.
    //     if let Err(e) = self.tx_new_certificates.send(certificate).await {
    //         warn!(
    //             "Failed to deliver certificate {} to the consensus: {}",
    //             digest, e
    //         );
    //         return Err(DagError::ShuttingDown);
    //     }

    //     Ok(())
    // }

    /// Synchronizes batches in the given header with other nodes (through our workers).
    /// Blocks until either synchronization is complete, or the current consensus rounds advances
    /// past the max allowed age. (`max_age == 0` means the header's round must match current
    /// round.)
    pub async fn sync_batches(
        &self,
        header: &Header,
        network: anemo::Network,
        max_age: Round,
    ) -> DagResult<()> {
        if header.author == self.name {
            debug!("skipping sync_batches for header {header}: no need to store payload of our own workers");
            return Ok(());
        }

        // Clone the round updates channel so we can get update notifications specific to
        // this RPC handler.
        let mut rx_consensus_round_updates = self.rx_consensus_round_updates.clone();
        let mut consensus_round = *rx_consensus_round_updates.borrow();
        ensure!(
            header.round >= consensus_round.saturating_sub(max_age),
            DagError::TooOld(
                header.digest().into(),
                header.round,
                consensus_round.saturating_sub(max_age)
            )
        );

        let mut missing = HashMap::new();
        for (digest, (worker_id, _)) in header.payload.iter() {
            // Check whether we have the batch. If one of our worker has the batch, the primary stores the pair
            // (digest, worker_id) in its own storage. It is important to verify that we received the batch
            // from the correct worker id to prevent the following attack:
            //      1. A Bad node sends a batch X to 2f good nodes through their worker #0.
            //      2. The bad node proposes a malformed block containing the batch X and claiming it comes
            //         from worker #1.
            //      3. The 2f good nodes do not need to sync and thus don't notice that the header is malformed.
            //         The bad node together with the 2f good nodes thus certify a block containing the batch X.
            //      4. The last good node will never be able to sync as it will keep sending its sync requests
            //         to workers #1 (rather than workers #0). Also, clients will never be able to retrieve batch
            //         X as they will be querying worker #1.
            if self
                .payload_store
                .read((*digest, *worker_id))
                .await?
                .is_none()
            {
                missing
                    .entry(*worker_id)
                    .or_insert_with(Vec::new)
                    .push(*digest);
            }
        }

        // Build Synchronize requests to workers.
        let mut synchronize_handles = Vec::new();
        for (worker_id, digests) in missing {
            let worker_name = self
                .worker_cache
                .load()
                .worker(&self.name, &worker_id)
                .expect("Author of valid header is not in the worker cache")
                .name;
            let network = network.clone();
            let retry_config = RetryConfig {
                retrying_max_elapsed_time: None, // Retry forever.
                ..Default::default()
            };
            let handle = retry_config.retry(move || {
                let network = network.clone();
                let digests = digests.clone();
                let message = WorkerSynchronizeMessage {
                    digests: digests.clone(),
                    target: header.author.clone(),
                };
                let peer = network.waiting_peer(anemo::PeerId(worker_name.0.to_bytes()));
                let mut client = PrimaryToWorkerClient::new(peer);
                async move {
                    let result = client.synchronize(message).await.map_err(|e| {
                        backoff::Error::transient(DagError::NetworkError(format!("{e:?}")))
                    });
                    if result.is_ok() {
                        for digest in digests.clone() {
                            self.payload_store
                                .async_write((digest, worker_id), 0u8)
                                .await;
                        }
                    }
                    result
                }
            });
            synchronize_handles.push(handle);
        }

        // Wait until results are back, or this request gets too old to continue.
        let mut wait_synchronize = futures::future::try_join_all(synchronize_handles);
        loop {
            tokio::select! {
                results = &mut wait_synchronize => {
                    break results
                        .map(|_| ())
                        .map_err(|e| DagError::NetworkError(format!("error synchronizing batches: {e:?}")))
                },
                // This aborts based on consensus round and not narwhal round. When this function
                // is used as part of handling vote requests, this may cause us to wait a bit
                // longer than needed to give up on synchronizing batches for headers that are
                // too old to receive a vote. This shouldn't be a big deal (the requester can
                // always abort their request at any point too), however if the extra resources
                // used to attempt to synchronize batches for longer than strictly needed become
                // problematic, this function could be augmented to also support cancellation based
                // on narwhal round.
                Ok(()) = rx_consensus_round_updates.changed() => {
                    consensus_round = *rx_consensus_round_updates.borrow();
                    ensure!(
                        header.round >= consensus_round.saturating_sub(max_age),
                        DagError::TooOld(
                            header.digest().into(),
                            header.round,
                            consensus_round.saturating_sub(max_age),
                        )
                    );
                },
            }
        }
    }

    /// Returns the parent certificates of the given header, and a list of digests for any
    /// that are missing.
    pub fn get_parents(
        &self,
        header: &Header,
    ) -> DagResult<(Vec<Certificate>, Vec<CertificateDigest>)> {
        let mut missing = Vec::new();
        let mut parents = Vec::new();
        for digest in &header.parents {
            let cert = if header.round == 1 {
                self.genesis.get(digest).cloned()
            } else {
                self.certificate_store.read(*digest)?
            };
            match cert {
                Some(certificate) => parents.push(certificate),
                None => missing.push(*digest),
            };
        }

        Ok((parents, missing))
    }

    /// Checks whether we have seen all the ancestors of the certificate. If we don't, send the
    /// certificate to the `CertificateFetcher` which will trigger range fetching of missing
    /// certificates.
    pub async fn check_parents(&self, certificate: &Certificate) -> DagResult<bool> {
        if certificate.round() == 1 {
            for digest in &certificate.header.parents {
                if self.genesis.contains_key(digest) {
                    continue;
                }
                return Ok(false);
            }
            return Ok(true);
        }

        for digest in &certificate.header.parents {
            if !self.has_processed_certificate(*digest).await? {
                self.tx_certificate_fetcher
                    .send(certificate.clone())
                    .await
                    .map_err(|_| DagError::ShuttingDown)?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// This method answers to the question of whether the certificate with the
    /// provided digest has ever been successfully processed (seen) by this
    /// node. Depending on the mode of running the node (internal Vs external
    /// consensus) either the dag will be used to confirm that or the
    /// certificate_store.
    async fn has_processed_certificate(&self, digest: CertificateDigest) -> DagResult<bool> {
        if let Some(dag) = &self.dag {
            return Ok(dag.has_ever_contained(digest).await);
        }
        Ok(self.certificate_store.contains(&digest)?)
    }
}
