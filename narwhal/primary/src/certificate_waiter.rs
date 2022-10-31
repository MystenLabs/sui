// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::metrics::PrimaryMetrics;
use config::Committee;
use crypto::{NetworkPublicKey, PublicKey};
use futures::{future::BoxFuture, stream::FuturesUnordered, FutureExt, StreamExt};
use network::{P2pNetwork, PrimaryToPrimaryRpc};
use rand::{rngs::ThreadRng, seq::SliceRandom};
use std::{collections::BTreeMap, future::pending, sync::Arc, time::Duration};
use storage::CertificateStore;
use tokio::{
    sync::{oneshot, watch},
    task::{JoinError, JoinHandle},
    time::{self, Instant},
};
use tracing::{debug, error, instrument, trace, warn};
use types::{
    error::{DagError, DagResult},
    metered_channel::{Receiver, Sender},
    Certificate, ConsensusStore, FetchCertificatesRequest, FetchCertificatesResponse,
    ReconfigureNotification, Round,
};

#[cfg(test)]
#[path = "tests/certificate_waiter_tests.rs"]
pub mod certificate_waiter_tests;

// Maximum number of certificates to fetch with one request.
const MAX_CERTIFICATES_TO_FETCH: usize = 1000;
// Seconds to wait for a response before issuing another parallel fetch request.
const PARALLEL_FETCH_REQUEST_INTERVAL_SECS: u64 = 5;

/// Message format from CertificateWaiter to core on the loopback channel.
pub struct CertificateLoopbackMessage {
    /// Certificates to be processed by the core.
    /// In normal case processing the certificates in order should not encounter any missing parent.
    pub certificates: Vec<Certificate>,
    /// Used by core to signal back that it is done with the certificates.
    pub done: oneshot::Sender<()>,
}

/// The CertificateWaiter is responsible for fetching certificates that this node is missing
/// from other primaries. It operates two loops:
/// Loop 1: listens for certificates missing parents from the core, tracks the highest missing
/// round per origin, and kicks start fetch tasks if needed.
/// Loop 2: runs fetch task to request certificates from other primaries continuously, until all
/// highest missing rounds have been met.
pub(crate) struct CertificateWaiter {
    /// Internal state of CertificateWaiter.
    state: Arc<CertificateWaiterState>,
    /// The committee information.
    committee: Committee,
    /// Persistent storage for certificates. Read-only usage.
    certificate_store: CertificateStore,
    /// Persistent storage for consensus when using internal consensus. Read-only usage.
    consensus_store: Option<Arc<ConsensusStore>>,
    /// Receiver for signal of round changes. Used for calculating gc_round.
    rx_consensus_round_updates: watch::Receiver<u64>,
    /// The depth of the garbage collector.
    gc_depth: Round,
    /// Watch channel notifying of epoch changes, it is only used for cleanup.
    rx_reconfigure: watch::Receiver<ReconfigureNotification>,
    /// Receives certificates with missing parents from the `Synchronizer`.
    rx_certificate_waiter: Receiver<Certificate>,
    /// Map of validator to target rounds that local store must catch up to.
    /// The targets are updated with each certificate missing parents sent from the core.
    /// Each fetch task may satisfy some / all / none of the targets.
    /// TODO: rethink the stopping criteria for fetching, balance simplicity with completeness
    /// of certificates (for avoiding jitters of voting / processing certificates instead of
    /// correctness).
    targets: BTreeMap<PublicKey, Round>,
    /// Keeps the handle to the inflight fetch certificates task.
    /// Contains a pending future that never returns, and at most 1 other task.
    fetch_certificates_task: FuturesUnordered<BoxFuture<'static, Result<(), JoinError>>>,
}

/// Thread-safe internal state of CertificateWaiter shared with its fetch task.
struct CertificateWaiterState {
    /// Identity of the current authority.
    name: PublicKey,
    /// Network client to fetch certificates from other primaries.
    network: P2pNetwork,
    /// Loops fetched certificates back to the core. Certificates are ensured to have all parents.
    tx_certificates_loopback: Sender<CertificateLoopbackMessage>,
    /// The metrics handler
    metrics: Arc<PrimaryMetrics>,
}

impl CertificateWaiter {
    #[must_use]
    pub fn spawn(
        name: PublicKey,
        committee: Committee,
        network: P2pNetwork,
        certificate_store: CertificateStore,
        consensus_store: Option<Arc<ConsensusStore>>,
        rx_consensus_round_updates: watch::Receiver<u64>,
        gc_depth: Round,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        rx_certificate_waiter: Receiver<Certificate>,
        tx_certificates_loopback: Sender<CertificateLoopbackMessage>,
        metrics: Arc<PrimaryMetrics>,
    ) -> JoinHandle<()> {
        let state = Arc::new(CertificateWaiterState {
            name,
            network,
            tx_certificates_loopback,
            metrics,
        });
        // Add a future that never returns to fetch_certificates_task, so it is blocked when empty.
        let fetch_certificates_task = FuturesUnordered::new();
        fetch_certificates_task.push(pending().boxed());
        tokio::spawn(async move {
            Self {
                state,
                committee,
                certificate_store,
                consensus_store,
                rx_consensus_round_updates,
                gc_depth,
                rx_reconfigure,
                rx_certificate_waiter,
                targets: BTreeMap::new(),
                fetch_certificates_task,
            }
            .run()
            .await;
        })
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                Some(certificate) = self.rx_certificate_waiter.recv() => {
                    let header = &certificate.header;
                    if header.epoch != self.committee.epoch() {
                        continue;
                    }
                    // Unnecessary to validate the header and certificate further, since it has
                    // already been validated.

                    if let Some(r) = self.targets.get(&header.author) {
                        if header.round <= *r {
                            // Ignore fetch request when we already need to sync to a later
                            // certificate from the same authority. Although this certificate may
                            // not be the parent of the later certificate, this should be ok
                            // because eventually a child of this certificate will miss parents and
                            // get inserted into the targets.
                            //
                            // Basically, it is ok to stop fetching without this certificate.
                            // If this certificate becomes a parent of other certificates, another
                            // fetch will be triggered eventually because of missing certificates.
                            continue;
                        }
                    }

                    // The header should have been verified as part of the certificate.
                    match self.last_committed_round(&header.author) {
                        Ok(r) => {
                            if r >= header.round {
                                // Ignore fetch request. Possibly the certificate was processed
                                // while the message is in the queue.
                                continue;
                            }
                            // Otherwise, continue to update fetch targets.
                        }
                        Err(e) => {
                            // If this happens, it is most likely due to bincode serialization error.
                            error!("Failed to read latest round for {}: {}", header.author, e);
                            continue;
                        }
                    };

                    // Update the target rounds for the authority.
                    self.targets.insert(header.author.clone(), header.round);

                    // Kick start a fetch task if there is no task other than the pending task running.
                    if self.fetch_certificates_task.len() == 1 {
                        self.kickstart();
                    }
                },
                _ = self.fetch_certificates_task.next() => {
                    // Kick start another fetch task after the previous one terminates.
                    // If all targets have been fetched, the new task will clean up the targets and exit.
                    if self.fetch_certificates_task.len() == 1 {
                        self.kickstart();
                    }
                },
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow_and_update().clone();
                    match message {
                        ReconfigureNotification::NewEpoch(committee) => {
                            self.committee = committee;
                            self.targets.clear();
                            self.fetch_certificates_task = FuturesUnordered::new();
                            self.fetch_certificates_task.push(pending().boxed());
                        },
                        ReconfigureNotification::UpdateCommittee(committee) => {
                            self.committee = committee;
                            // There should be no committee membership change so self.targets does
                            // not need to be updated.
                        },
                        ReconfigureNotification::Shutdown => return
                    }
                    debug!("Committee updated to {}", self.committee);
                }
            }
        }
    }

    // Starts a task to fetch missing certificates from other primaries.
    // A call to kickstart() can be triggered by a certificate with missing parents or the end of a
    // fetch task. Each iteration of kickstart() updates the target rounds, and iterations will
    // continue until there are no more target rounds to catch up to.
    #[allow(clippy::mutable_key_type)]
    fn kickstart(&mut self) {
        let gc_round = self.gc_round();
        let committed_rounds: BTreeMap<_, _> = match self.all_committed_rounds() {
            Ok(committed_rounds) => committed_rounds,
            Err(e) => {
                warn!("Failed to read rounds per authority from the certificate store: {e}");
                return;
            }
        }
        .into_iter()
        .map(|(key, r)| (key, r.max(gc_round)))
        .collect();
        self.targets.retain(|origin, target_round| {
            let committed_round = committed_rounds.get(origin).unwrap_or(&gc_round);
            // Drop sync target when cert store already has an equal or higher round for the origin.
            // This will apply GC to targets as well.
            //
            // Similarly, even if the store actually does not have target_round for the origin,
            // it is ok to stop fetching without this certificate.
            // If this certificate becomes a parent of other certificates, another
            // fetch will be triggered eventually because of missing certificates.
            committed_round < target_round
        });
        if self.targets.is_empty() {
            trace!("Certificates have caught up. Skip fetching.");
            return;
        }

        let state = self.state.clone();
        let committee = self.committee.clone();

        debug!(
            "Starting task to fetch missing certificates: max target {}, committed {:?}",
            self.targets.values().max().unwrap_or(&0),
            committed_rounds.values()
        );
        self.fetch_certificates_task.push(
            tokio::task::spawn(async move {
                state
                    .metrics
                    .certificate_waiter_inflight_fetch
                    .with_label_values(&[&committee.epoch.to_string()])
                    .inc();
                state
                    .metrics
                    .certificate_waiter_fetch_attempts
                    .with_label_values(&[&committee.epoch.to_string()])
                    .inc();

                let now = Instant::now();
                match run_fetch_task(state.clone(), committee.clone(), committed_rounds).await {
                    Ok(_) => {
                        debug!(
                            "Finished task to fetch certificates successfully, elapsed = {}s",
                            now.elapsed().as_secs_f64()
                        );
                    }
                    Err(e) => {
                        debug!("Error from task to fetch certificates: {e}");
                    }
                };

                state
                    .metrics
                    .certificate_waiter_op_latency
                    .with_label_values(&[&committee.epoch.to_string()])
                    .observe(now.elapsed().as_secs_f64());
                state
                    .metrics
                    .certificate_waiter_inflight_fetch
                    .with_label_values(&[&committee.epoch.to_string()])
                    .dec();
            })
            .boxed(),
        );
    }

    fn gc_round(&self) -> Round {
        self.rx_consensus_round_updates
            .borrow()
            .to_owned()
            .saturating_sub(self.gc_depth)
    }

    fn last_committed_round(&self, authority: &PublicKey) -> DagResult<Round> {
        match &self.consensus_store {
            // Last round is 0 (genesis) when authority is not found in store.
            Some(consensus_store) => Ok(consensus_store
                .read_last_committed_round(authority)?
                .unwrap_or(0)),
            // When using external consensus, consensus_store is empty.
            // TODO: use all origins and rounds of written certs above gc_round, instead of just
            // the last written rounds.
            None => Ok(self
                .certificate_store
                .last_round_number(authority)?
                .unwrap_or(0)),
        }
    }

    #[allow(clippy::mutable_key_type)]
    fn all_committed_rounds(&self) -> DagResult<BTreeMap<PublicKey, Round>> {
        let mut all_committed_rounds = BTreeMap::new();
        for (name, _) in self.committee.authorities() {
            // Last round is 0 (genesis) when authority is not found in store.
            let last_round = self.last_committed_round(name)?;
            all_committed_rounds.insert(name.clone(), last_round);
        }
        Ok(all_committed_rounds)
    }
}

#[allow(clippy::mutable_key_type)]
#[instrument(level = "debug", skip_all)]
async fn run_fetch_task(
    state: Arc<CertificateWaiterState>,
    committee: Committee,
    committed_rounds: BTreeMap<PublicKey, Round>,
) -> DagResult<()> {
    // Send request to fetch certificates.
    // TODO: use a BTreeSet / RoaringTreemap to represent existing certificates per validator,
    // from the last committed round or even gc round.
    let request = FetchCertificatesRequest {
        exclusive_lower_bounds: committed_rounds.into_iter().collect(),
        max_items: MAX_CERTIFICATES_TO_FETCH,
    };
    let response =
        fetch_certificates_helper(&state.name, &state.network, &committee, request).await;

    // Process and store fetched certificates.
    let num_certs_fetched = response.certificates.len();
    process_certificates_helper(response, &state.tx_certificates_loopback).await?;
    state
        .metrics
        .certificate_waiter_num_certificates_processed
        .with_label_values(&[&committee.epoch().to_string()])
        .add(num_certs_fetched as i64);

    debug!("Successfully fetched and processed {num_certs_fetched} certificates");
    Ok(())
}

/// Fetches certificates from other primaries concurrently, with ~5 sec interval between each request.
/// Terminates after the 1st successful response is received.
#[instrument(level = "debug", skip_all)]
async fn fetch_certificates_helper(
    name: &PublicKey,
    network: &P2pNetwork,
    committee: &Committee,
    request: FetchCertificatesRequest,
) -> FetchCertificatesResponse {
    trace!("Start sending fetch certificates requests");
    // TODO: make this a config parameter.
    let request_interval = Duration::from_secs(PARALLEL_FETCH_REQUEST_INTERVAL_SECS);
    let mut peers: Vec<NetworkPublicKey> = committee
        .others_primaries(name)
        .into_iter()
        .map(|(_, _, network_key)| network_key)
        .collect();
    loop {
        // TODO: order by stake weight instead. Possibly shuffle within a few buckets.
        peers.shuffle(&mut ThreadRng::default());
        let mut fut = FuturesUnordered::new();
        for peer in peers.iter() {
            let network = network.network();
            let request = request.clone();
            fut.push(async move { network.fetch_certificates(peer, request).await });
            let mut interval = Box::pin(time::sleep(request_interval));
            tokio::select! {
                res = fut.next() => match res {
                    Some(Ok(resp)) => {
                        return resp;
                    }
                    Some(Err(e)) => {
                        debug!("Failed to fetch certificates: {e}");
                        // Issue request to another primary immediately.
                    }
                    None => {}
                },
                _ = &mut interval => {
                    debug!("fetch_certificates_helper: no response yet. Sending out an additional fetch request in parallel.");
                }
            };
        }
    }
}

#[instrument(level = "debug", skip_all)]
async fn process_certificates_helper(
    response: FetchCertificatesResponse,
    tx_certificates_loopback: &Sender<CertificateLoopbackMessage>,
) -> DagResult<()> {
    trace!("Start sending fetched certificates to processing");
    if response.certificates.len() > MAX_CERTIFICATES_TO_FETCH {
        return Err(DagError::TooManyFetchedCertificatesReturned(
            response.certificates.len(),
            MAX_CERTIFICATES_TO_FETCH,
        ));
    }
    let (tx_done, rx_done) = oneshot::channel();
    if let Err(e) = tx_certificates_loopback
        .send(CertificateLoopbackMessage {
            certificates: response.certificates,
            done: tx_done,
        })
        .await
    {
        return Err(DagError::ClosedChannel(format!(
            "Failed to send fetched certificate to processing. tx_certificates_loopback error: {}",
            e
        )));
    }
    if let Err(e) = rx_done.await {
        return Err(DagError::ClosedChannel(format!(
            "Failed to wait for core to process loopback certificates: {}",
            e
        )));
    }
    trace!("Fetched certificates have finished processing");

    Ok(())
}
