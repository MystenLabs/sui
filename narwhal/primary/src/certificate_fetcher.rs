// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{metrics::PrimaryMetrics, synchronizer::Synchronizer};
use anemo::Request;
use config::{AuthorityIdentifier, Committee};
use consensus::consensus::ConsensusRound;
use crypto::NetworkPublicKey;
use futures::{stream::FuturesUnordered, StreamExt};
use itertools::Itertools;
use mysten_metrics::metered_channel::Receiver;
use mysten_metrics::{monitored_future, monitored_scope, spawn_logged_monitored_task};
use network::PrimaryToPrimaryRpc;
use rand::{rngs::ThreadRng, seq::SliceRandom};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
use storage::CertificateStore;
use tokio::task::{spawn_blocking, JoinSet};
use tokio::{
    sync::watch,
    task::JoinHandle,
    time::{sleep, timeout, Instant},
};
use tracing::{debug, error, instrument, trace, warn};
use types::{
    error::{DagError, DagResult},
    Certificate, CertificateAPI, ConditionalBroadcastReceiver, FetchCertificatesRequest,
    FetchCertificatesResponse, HeaderAPI, Round,
};

#[cfg(test)]
#[path = "tests/certificate_fetcher_tests.rs"]
pub mod certificate_fetcher_tests;

// Maximum number of certificates to fetch with one request.
const MAX_CERTIFICATES_TO_FETCH: usize = 2_000;
// Seconds to wait for a response before issuing another parallel fetch request.
const PARALLEL_FETCH_REQUEST_INTERVAL_SECS: Duration = Duration::from_secs(5);
// The timeout for an iteration of parallel fetch requests over all peers would be
// num peers * PARALLEL_FETCH_REQUEST_INTERVAL_SECS + PARALLEL_FETCH_REQUEST_ADDITIONAL_TIMEOUT
const PARALLEL_FETCH_REQUEST_ADDITIONAL_TIMEOUT: Duration = Duration::from_secs(15);
// Number of certificates to verify in a batch. Verifications in each batch run serially.
// Batch size is chosen so that verifying a batch takes non-trival
// time (verifying a batch of 200 certificates should take > 100ms).
const VERIFY_CERTIFICATES_BATCH_SIZE: usize = 200;

#[derive(Clone, Debug)]
pub enum CertificateFetcherCommand {
    /// Fetch the certificate and its ancestors.
    Ancestors(Certificate),
    /// Fetch once from a random primary.
    Kick,
}

/// The CertificateFetcher is responsible for fetching certificates that this primary is missing
/// from peers. It operates a loop which listens for commands to fetch a specific certificate's
/// ancestors, or just to start one fetch attempt.
///
/// In each fetch, the CertificateFetcher first scans locally available certificates. Then it sends
/// this information to a random peer. The peer would reply with the missing certificates that can
/// be accepted by this primary. After a fetch completes, another one will start immediately if
/// there are more certificates missing ancestors.
pub(crate) struct CertificateFetcher {
    /// Internal state of CertificateFetcher.
    state: Arc<CertificateFetcherState>,
    /// The committee information.
    committee: Committee,
    /// Persistent storage for certificates. Read-only usage.
    certificate_store: CertificateStore,
    /// Receiver for signal of round changes.
    rx_consensus_round_updates: watch::Receiver<ConsensusRound>,
    /// Receiver for shutdown.
    rx_shutdown: ConditionalBroadcastReceiver,
    /// Receives certificates with missing parents from the `Synchronizer`.
    rx_certificate_fetcher: Receiver<CertificateFetcherCommand>,
    /// Map of validator to target rounds that local store must catch up to.
    /// The targets are updated with each certificate missing parents sent from the core.
    /// Each fetch task may satisfy some / all / none of the targets.
    /// TODO: rethink the stopping criteria for fetching, balance simplicity with completeness
    /// of certificates (for avoiding jitters of voting / processing certificates instead of
    /// correctness).
    targets: BTreeMap<AuthorityIdentifier, Round>,
    /// Keeps the handle to the (at most one) inflight fetch certificates task.
    fetch_certificates_task: JoinSet<()>,
}

/// Thread-safe internal state of CertificateFetcher shared with its fetch task.
struct CertificateFetcherState {
    /// Identity of the current authority.
    authority_id: AuthorityIdentifier,
    /// Network client to fetch certificates from other primaries.
    network: anemo::Network,
    /// Accepts Certificates into local storage.
    synchronizer: Arc<Synchronizer>,
    /// The metrics handler
    metrics: Arc<PrimaryMetrics>,
}

impl CertificateFetcher {
    #[must_use]
    pub fn spawn(
        authority_id: AuthorityIdentifier,
        committee: Committee,
        network: anemo::Network,
        certificate_store: CertificateStore,
        rx_consensus_round_updates: watch::Receiver<ConsensusRound>,
        rx_shutdown: ConditionalBroadcastReceiver,
        rx_certificate_fetcher: Receiver<CertificateFetcherCommand>,
        synchronizer: Arc<Synchronizer>,
        metrics: Arc<PrimaryMetrics>,
    ) -> JoinHandle<()> {
        let state = Arc::new(CertificateFetcherState {
            authority_id,
            network,
            synchronizer,
            metrics,
        });

        spawn_logged_monitored_task!(
            async move {
                Self {
                    state,
                    committee,
                    certificate_store,
                    rx_consensus_round_updates,
                    rx_shutdown,
                    rx_certificate_fetcher,
                    targets: BTreeMap::new(),
                    fetch_certificates_task: JoinSet::new(),
                }
                .run()
                .await;
            },
            "CertificateFetcherTask"
        )
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                Some(command) = self.rx_certificate_fetcher.recv() => {
                    let certificate = match command {
                        CertificateFetcherCommand::Ancestors(certificate) => certificate,
                        CertificateFetcherCommand::Kick => {
                            // Kick start a fetch task if there is no other task running.
                            if self.fetch_certificates_task.is_empty() {
                                self.kickstart();
                            }
                            continue;
                        }
                    };
                    let header = &certificate.header();
                    if header.epoch() != self.committee.epoch() {
                        continue;
                    }
                    // Unnecessary to validate the header and certificate further, since it has
                    // already been validated.

                    if let Some(r) = self.targets.get(&header.author()) {
                        if header.round() <= *r {
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
                    match self.certificate_store.last_round_number(header.author()) {
                        Ok(r) => {
                            if header.round() <= r.unwrap_or(0) {
                                // Ignore fetch request. Possibly the certificate was processed
                                // while the message is in the queue.
                                continue;
                            }
                            // Otherwise, continue to update fetch targets.
                        }
                        Err(e) => {
                            // If this happens, it is most likely due to serialization error.
                            error!("Failed to read latest round for {}: {}", header.author(), e);
                            continue;
                        }
                    };

                    // Update the target rounds for the authority.
                    self.targets.insert(header.author(), header.round());

                    // Kick start a fetch task if there is no other task running.
                    if self.fetch_certificates_task.is_empty() {
                        self.kickstart();
                    }
                },
                Some(result) = self.fetch_certificates_task.join_next(), if !self.fetch_certificates_task.is_empty() => {
                    match result {
                        Ok(()) => {},
                        Err(e) => {
                            if e.is_cancelled() {
                                // avoid crashing on ungraceful shutdown
                            } else if e.is_panic() {
                                // propagate panics.
                                std::panic::resume_unwind(e.into_panic());
                            } else {
                                panic!("fetch certificates task failed: {e}");
                            }
                        },
                    };

                    // Kick start another fetch task after the previous one terminates.
                    // If all targets have been fetched, the new task will clean up the targets and exit.
                    if self.fetch_certificates_task.is_empty() {
                        self.kickstart();
                    }
                },
                _ = self.rx_shutdown.receiver.recv() => {
                    return
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
        let mut written_rounds = BTreeMap::<AuthorityIdentifier, BTreeSet<Round>>::new();
        for authority in self.committee.authorities() {
            // Initialize written_rounds for all authorities, because the handler only sends back
            // certificates for the set of authorities here.
            written_rounds.insert(authority.id(), BTreeSet::new());
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
                rounds.last().unwrap_or(&gc_round).to_owned()
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
        self.fetch_certificates_task
            .spawn(monitored_future!(async move {
                let _scope = monitored_scope("CertificatesFetching");
                state.metrics.certificate_fetcher_inflight_fetch.inc();

                let now = Instant::now();
                match run_fetch_task(state.clone(), committee, gc_round, written_rounds).await {
                    Ok(_) => {
                        debug!(
                            "Finished task to fetch certificates successfully, elapsed = {}s",
                            now.elapsed().as_secs_f64()
                        );
                    }
                    Err(e) => {
                        warn!("Error from task to fetch certificates: {e}");
                    }
                };

                state.metrics.certificate_fetcher_inflight_fetch.dec();
            }));
    }

    fn gc_round(&self) -> Round {
        self.rx_consensus_round_updates.borrow().gc_round
    }
}

#[allow(clippy::mutable_key_type)]
#[instrument(level = "debug", skip_all)]
async fn run_fetch_task(
    state: Arc<CertificateFetcherState>,
    committee: Committee,
    gc_round: Round,
    written_rounds: BTreeMap<AuthorityIdentifier, BTreeSet<Round>>,
) -> DagResult<()> {
    // Send request to fetch certificates.
    let request = FetchCertificatesRequest::default()
        .set_bounds(gc_round, written_rounds)
        .set_max_items(MAX_CERTIFICATES_TO_FETCH);
    let Some(response) =
        fetch_certificates_helper(state.authority_id, &state.network, &committee, request).await else {
            return Err(DagError::NoCertificateFetched);
        };

    // Process and store fetched certificates.
    let num_certs_fetched = response.certificates.len();
    process_certificates_helper(response, &state.synchronizer, state.metrics.clone()).await?;
    state
        .metrics
        .certificate_fetcher_num_certificates_processed
        .inc_by(num_certs_fetched as u64);

    debug!("Successfully fetched and processed {num_certs_fetched} certificates");
    Ok(())
}

/// Fetches certificates from other primaries concurrently, with ~5 sec interval between each request.
/// Terminates after the 1st successful response is received.
#[instrument(level = "debug", skip_all)]
async fn fetch_certificates_helper(
    name: AuthorityIdentifier,
    network: &anemo::Network,
    committee: &Committee,
    request: FetchCertificatesRequest,
) -> Option<FetchCertificatesResponse> {
    let _scope = monitored_scope("FetchingCertificatesFromPeers");
    trace!("Start sending fetch certificates requests");
    // TODO: make this a config parameter.
    let request_interval = PARALLEL_FETCH_REQUEST_INTERVAL_SECS;
    let mut peers: Vec<NetworkPublicKey> = committee
        .others_primaries_by_id(name)
        .into_iter()
        .map(|(_, _, network_key)| network_key)
        .collect();
    peers.shuffle(&mut ThreadRng::default());
    let fetch_timeout = PARALLEL_FETCH_REQUEST_INTERVAL_SECS * peers.len().try_into().unwrap()
        + PARALLEL_FETCH_REQUEST_ADDITIONAL_TIMEOUT;
    let fetch_callback = async move {
        debug!("Starting to fetch certificates");
        let mut fut = FuturesUnordered::new();
        // Loop until one peer returns with certificates, or no peer does.
        loop {
            if let Some(peer) = peers.pop() {
                let request = Request::new(request.clone())
                    .with_timeout(PARALLEL_FETCH_REQUEST_INTERVAL_SECS * 2);
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
            let mut interval = Box::pin(sleep(request_interval));
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
                        continue;
                    }
                    None => {
                        debug!("No peer can be reached for fetching certificates!");
                        // Last or all requests to peers may have failed immediately, so wait
                        // before returning to avoid retrying fetching immediately.
                        sleep(request_interval).await;
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
    synchronizer: &Synchronizer,
    metrics: Arc<PrimaryMetrics>,
) -> DagResult<()> {
    trace!("Start sending fetched certificates to processing");
    if response.certificates.len() > MAX_CERTIFICATES_TO_FETCH {
        return Err(DagError::TooManyFetchedCertificatesReturned(
            response.certificates.len(),
            MAX_CERTIFICATES_TO_FETCH,
        ));
    }
    // Verify certificates in parallel.
    // In PrimaryReceiverHandler, certificates already in storage are ignored.
    // The check is unnecessary here, because there is no concurrent processing of older
    // certificates. For byzantine failures, the check will not be effective anyway.
    let _verify_scope = monitored_scope("VerifyingFetchedCertificates");
    let all_certificates = response.certificates;
    let verify_tasks = all_certificates
        .chunks(VERIFY_CERTIFICATES_BATCH_SIZE)
        .map(|certs| {
            let certs = certs.to_vec();
            let sync = synchronizer.clone();
            let metrics = metrics.clone();
            // Use threads dedicated to computation heavy work.
            spawn_blocking(move || {
                let now = Instant::now();
                for c in &certs {
                    sync.sanitize_certificate(c)?;
                }
                metrics
                    .certificate_fetcher_total_verification_us
                    .inc_by(now.elapsed().as_micros() as u64);
                Ok::<Vec<Certificate>, DagError>(certs)
            })
        })
        .collect_vec();
    // Process verified certificates in the same order as received.
    for task in verify_tasks {
        let certificates = task.await.map_err(|_| DagError::Canceled)??;
        let now = Instant::now();
        for cert in certificates {
            if let Err(e) = synchronizer.try_accept_fetched_certificate(cert).await {
                // It is possible that subsequent certificates are useful,
                // so not stopping early.
                warn!("Failed to accept fetched certificate: {e}");
            }
        }
        metrics
            .certificate_fetcher_total_accept_us
            .inc_by(now.elapsed().as_micros() as u64);
    }

    trace!("Fetched certificates have been processed");

    Ok(())
}
