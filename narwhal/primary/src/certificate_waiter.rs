// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::metrics::PrimaryMetrics;
use config::Committee;
use crypto::{NetworkPublicKey, PublicKey};
use futures::{future::BoxFuture, stream::FuturesUnordered, FutureExt, StreamExt};
use network::{P2pNetwork, PrimaryToPrimaryRpc};
use rand::{rngs::ThreadRng, seq::SliceRandom};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
use storage::CertificateStore;
use sui_metrics::{monitored_future, spawn_monitored_task};
use tokio::{
    sync::{oneshot, watch},
    task::{JoinError, JoinHandle},
    time::{self, timeout, Instant},
};
use tracing::{debug, error, instrument, trace, warn};
use types::{
    error::{DagError, DagResult},
    metered_channel::{Receiver, Sender},
    Certificate, FetchCertificatesRequest, FetchCertificatesResponse, ReconfigureNotification,
    Round,
};

#[cfg(test)]
#[path = "tests/certificate_waiter_tests.rs"]
pub mod certificate_waiter_tests;

// Maximum number of certificates to fetch with one request.
const MAX_CERTIFICATES_TO_FETCH: usize = 1000;
// Seconds to wait for a response before issuing another parallel fetch request.
const PARALLEL_FETCH_REQUEST_INTERVAL_SECS: Duration = Duration::from_secs(5);
// The timeout for an iteration of parallel fetch requests over all peers would be
// num peers * PARALLEL_FETCH_REQUEST_INTERVAL_SECS + PARALLEL_FETCH_REQUEST_ADDITIONAL_TIMEOUT
const PARALLEL_FETCH_REQUEST_ADDITIONAL_TIMEOUT: Duration = Duration::from_secs(15);

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
    /// Keeps the handle to the (at most one) inflight fetch certificates task.
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
        spawn_monitored_task!(async move {
            Self {
                state,
                committee,
                certificate_store,
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
                    match self
                    .certificate_store
                    .last_round_number(&header.author) {
                        Ok(r) => {
                            if header.round <= r.unwrap_or(0) {
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

                    // Kick start a fetch task if there is no other task running.
                    if self.fetch_certificates_task.is_empty() {
                        self.kickstart();
                    }
                },
                _ = self.fetch_certificates_task.next(), if !self.fetch_certificates_task.is_empty() => {
                    // Kick start another fetch task after the previous one terminates.
                    // If all targets have been fetched, the new task will clean up the targets and exit.
                    if self.fetch_certificates_task.is_empty() {
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
        // Skip fetching certificates at or below the gc round.
        let gc_round = self.gc_round();
        // Skip fetching certificates that already exist locally.
        let mut written_rounds = BTreeMap::<PublicKey, BTreeSet<Round>>::new();
        for (origin, _) in self.committee.authorities() {
            // Initialize written_rounds for all authorities, because the handler only sends back
            // certificates for the set of authorities here.
            written_rounds.insert(origin.clone(), BTreeSet::new());
        }
        // NOTE: origins_after_round() is inclusive.
        match self.certificate_store.origins_after_round(gc_round + 1) {
            Ok(origins) => {
                for (round, origins) in origins {
                    for origin in origins {
                        written_rounds.entry(origin).or_default().insert(round);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to read from certificate store: {e}");
                return;
            }
        };

        self.targets.retain(|origin, target_round| {
            let last_written_round = written_rounds.get(origin).map_or(gc_round, |rounds| {
                // TODO: switch to last() after it stabilizes for BTreeSet.
                rounds.iter().rev().next().unwrap_or(&gc_round).to_owned()
            });
            // Drop sync target when cert store already has an equal or higher round for the origin.
            // This applies GC to targets as well.
            //
            // NOTE: even if the store actually does not have target_round for the origin,
            // it is ok to stop fetching without this certificate.
            // If this certificate becomes a parent of other certificates, another
            // fetch will be triggered eventually because of missing certificates.
            last_written_round < *target_round
        });
        if self.targets.is_empty() {
            debug!("Certificates have caught up. Skip fetching.");
            return;
        }

        let state = self.state.clone();
        let committee = self.committee.clone();

        debug!(
            "Starting task to fetch missing certificates: max target {}, gc round {:?}",
            self.targets.values().max().unwrap_or(&0),
            gc_round
        );
        self.fetch_certificates_task.push(
            spawn_monitored_task!(async move {
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
                match run_fetch_task(state.clone(), committee.clone(), gc_round, written_rounds)
                    .await
                {
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
}

#[allow(clippy::mutable_key_type)]
#[instrument(level = "debug", skip_all)]
async fn run_fetch_task(
    state: Arc<CertificateWaiterState>,
    committee: Committee,
    gc_round: Round,
    written_rounds: BTreeMap<PublicKey, BTreeSet<Round>>,
) -> DagResult<()> {
    // Send request to fetch certificates.
    let request = FetchCertificatesRequest::default()
        .set_bounds(gc_round, written_rounds)
        .set_max_items(MAX_CERTIFICATES_TO_FETCH);
    let Some(response) =
        fetch_certificates_helper(&state.name, &state.network, &committee, request).await else {
            return Ok(());
        };

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
) -> Option<FetchCertificatesResponse> {
    trace!("Start sending fetch certificates requests");
    // TODO: make this a config parameter.
    let request_interval = PARALLEL_FETCH_REQUEST_INTERVAL_SECS;
    let mut peers: Vec<NetworkPublicKey> = committee
        .others_primaries(name)
        .into_iter()
        .map(|(_, _, network_key)| network_key)
        .collect();
    peers.shuffle(&mut ThreadRng::default());
    let fetch_timeout = PARALLEL_FETCH_REQUEST_INTERVAL_SECS * peers.len().try_into().unwrap()
        + PARALLEL_FETCH_REQUEST_ADDITIONAL_TIMEOUT;
    let fetch_callback = async move {
        // TODO: shuffle by stake weight instead.
        debug!("Starting to fetch certificates");
        let mut fut = FuturesUnordered::new();
        // Loop until one peer returns with certificates, or no peer does.
        loop {
            if let Some(peer) = peers.pop() {
                let network = network.network();
                let request = request.clone();
                fut.push(monitored_future!(async move {
                    debug!("Sending out fetch request in parallel to {peer}");
                    let result = network.fetch_certificates(&peer, request).await;
                    if let Ok(resp) = &result {
                        debug!(
                            "Fetched {} certificates from peer {peer}",
                            resp.certificates.len()
                        );
                    }
                    result
                }));
            }
            let mut interval = Box::pin(time::sleep(request_interval));
            tokio::select! {
                res = fut.next() => match res {
                    Some(Ok(resp)) => {
                        if resp.certificates.is_empty() {
                            // Issue request to another primary immediately.
                            continue;
                        }
                        return Some(resp);
                    }
                    Some(Err(e)) => {
                        debug!("Failed to fetch certificates: {e}");
                        // Issue request to another primary immediately.
                    }
                    None => {
                        debug!("No certificate is fetched across all peers!");
                        return None;
                    }
                },
                _ = &mut interval => {
                    // Not response received in the last interval. Send out another fetch request
                    // in parallel, if there is a peer that has not been sent to.
                }
            };
        }
    };
    match timeout(fetch_timeout, fetch_callback).await {
        Ok(result) => result,
        Err(e) => {
            debug!("Timed out fetching certificates: {e}");
            None
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
