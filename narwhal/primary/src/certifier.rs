// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{aggregators::VotesAggregator, metrics::PrimaryMetrics, synchronizer::Synchronizer};

use config::{AuthorityIdentifier, Committee};
use crypto::{NetworkPublicKey, Signature};
use fastcrypto::signature_service::SignatureService;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use mysten_metrics::{monitored_future, spawn_logged_monitored_task};
use network::anemo_ext::NetworkExt;
use std::sync::Arc;
use std::time::Duration;
use storage::{CertificateStore, HeaderStore};
use sui_macros::fail_point_async;
use tokio::{
    sync::oneshot,
    task::{JoinHandle, JoinSet},
};
use tracing::{debug, enabled, error, info, instrument, warn};
use types::{
    ensure,
    error::{DagError, DagResult},
    metered_channel::Receiver,
    Certificate, CertificateDigest, ConditionalBroadcastReceiver, Header, HeaderAPI,
    PrimaryToPrimaryClient, RequestVoteRequest, Vote, VoteAPI,
};

#[cfg(test)]
#[path = "tests/certifier_tests.rs"]
pub mod certifier_tests;

/// This component is responisble for proposing headers to peers, collecting votes on headers,
/// and certifying headers into certificates.
///
/// It receives headers to propose from Proposer via `rx_headers`, and sends out certificates to be
/// broadcasted by calling `Synchronizer::accept_own_certificate()`.
pub struct Certifier {
    /// The identifier of this primary.
    authority_id: AuthorityIdentifier,
    /// The committee information.
    committee: Committee,
    /// The persistent storage keyed to headers.
    header_store: HeaderStore,
    /// The persistent storage keyed to certificates.
    certificate_store: CertificateStore,
    /// Handles synchronization with other nodes and our workers.
    synchronizer: Arc<Synchronizer>,
    /// Service to sign headers.
    signature_service: SignatureService<Signature, { crypto::INTENT_MESSAGE_LENGTH }>,
    /// Receiver for shutdown.
    rx_shutdown: ConditionalBroadcastReceiver,
    /// Receives our newly created headers from the `Proposer`.
    rx_headers: Receiver<Header>,
    /// Used to cancel vote requests for a previously-proposed header that is being replaced
    /// before a certificate could be formed.
    cancel_proposed_header: Option<oneshot::Sender<()>>,
    /// Handle to propose_header task. Our target is to have only one task running always, thus
    /// we cancel the previously running before we spawn the next one. However, we don't wait for
    /// the previous to finish to spawn the new one, so we might temporarily have more that one
    /// parallel running, which should be fine though.
    propose_header_tasks: JoinSet<DagResult<Certificate>>,
    /// A network sender to send the batches to the other workers.
    network: anemo::Network,
    /// Metrics handler
    metrics: Arc<PrimaryMetrics>,
}

impl Certifier {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn spawn(
        authority_id: AuthorityIdentifier,
        committee: Committee,
        header_store: HeaderStore,
        certificate_store: CertificateStore,
        synchronizer: Arc<Synchronizer>,
        signature_service: SignatureService<Signature, { crypto::INTENT_MESSAGE_LENGTH }>,
        rx_shutdown: ConditionalBroadcastReceiver,
        rx_headers: Receiver<Header>,
        metrics: Arc<PrimaryMetrics>,
        primary_network: anemo::Network,
    ) -> JoinHandle<()> {
        spawn_logged_monitored_task!(
            async move {
                Self {
                    authority_id,
                    committee,
                    header_store,
                    certificate_store,
                    synchronizer,
                    signature_service,
                    rx_shutdown,
                    rx_headers,
                    cancel_proposed_header: None,
                    propose_header_tasks: JoinSet::new(),
                    network: primary_network,
                    metrics,
                }
                .run_inner()
                .await
            },
            "CertifierTask"
        )
    }

    #[instrument(level = "info", skip_all)]
    async fn run_inner(self) {
        let core = async move { self.run().await };

        match core.await {
            Err(err @ DagError::ShuttingDown) => debug!("{:?}", err),
            Err(err) => panic!("{:?}", err),
            Ok(_) => {}
        }
    }

    // Requests a vote for a Header from the given peer. Retries indefinitely until either a
    // vote is received, or a permanent error is returned.
    #[instrument(level = "debug", skip_all, fields(header_digest = ?header.digest()))]
    async fn request_vote(
        network: anemo::Network,
        committee: Committee,
        certificate_store: CertificateStore,
        authority: AuthorityIdentifier,
        target: NetworkPublicKey,
        header: Header,
    ) -> DagResult<Vote> {
        let peer_id = anemo::PeerId(target.0.to_bytes());
        let peer = network.waiting_peer(peer_id);

        let mut client = PrimaryToPrimaryClient::new(peer);

        let mut missing_parents: Vec<CertificateDigest> = Vec::new();
        let mut attempt: u32 = 0;
        let vote: Vote = loop {
            attempt += 1;

            let parents = if missing_parents.is_empty() {
                Vec::new()
            } else {
                let expected_count = missing_parents.len();
                let parents: Vec<_> = certificate_store
                    .read_all(
                        missing_parents
                            .into_iter()
                            // Only provide certs that are parents for the requested vote.
                            .filter(|parent| header.parents().contains(parent)),
                    )?
                    .into_iter()
                    .flatten()
                    .collect();
                if parents.len() != expected_count {
                    warn!("tried to read {expected_count} missing certificates requested by remote primary for vote request, but only found {}", parents.len());
                    return Err(DagError::ProposedHeaderMissingCertificates);
                }
                parents
            };

            // TODO: Remove timeout from this RPC once anemo issue #10 is resolved.
            match client
                .request_vote(RequestVoteRequest {
                    header: header.clone(),
                    parents,
                })
                .await
            {
                Ok(response) => {
                    let response = response.into_body();
                    if response.vote.is_some() {
                        break response.vote.unwrap();
                    }
                    missing_parents = response.missing;
                }
                Err(status) => {
                    if status.status() == anemo::types::response::StatusCode::BadRequest {
                        return Err(DagError::NetworkError(format!(
                            "unrecoverable error requesting vote for {header}: {status:?}"
                        )));
                    }
                    missing_parents = Vec::new();
                }
            }

            // Retry delay. Using custom values here because pure exponential backoff is hard to
            // configure without it being either too aggressive or too slow. We want the first
            // retry to be instantaneous, next couple to be fast, and to slow quickly thereafter.
            tokio::time::sleep(Duration::from_millis(match attempt {
                1 => 0,
                2 => 100,
                3 => 500,
                4 => 1_000,
                5 => 2_000,
                6 => 5_000,
                _ => 10_000,
            }))
            .await;
        };

        // Verify the vote. Note that only the header digest is signed by the vote.
        ensure!(
            vote.header_digest() == header.digest()
                && vote.origin() == header.author()
                && vote.author() == authority,
            DagError::UnexpectedVote(vote.header_digest())
        );
        // Possible equivocations.
        ensure!(
            header.epoch() == vote.epoch(),
            DagError::InvalidEpoch {
                expected: header.epoch(),
                received: vote.epoch()
            }
        );
        ensure!(
            header.round() == vote.round(),
            DagError::InvalidRound {
                expected: header.round(),
                received: vote.round()
            }
        );

        // Ensure the header is from the correct epoch.
        ensure!(
            vote.epoch() == committee.epoch(),
            DagError::InvalidEpoch {
                expected: committee.epoch(),
                received: vote.epoch()
            }
        );

        // Ensure the authority has voting rights.
        ensure!(
            committee.stake_by_id(vote.author()) > 0,
            DagError::UnknownAuthority(vote.author().to_string())
        );

        Ok(vote)
    }

    #[instrument(level = "debug", skip_all, fields(header_digest = ?header.digest()))]
    async fn propose_header(
        authority_id: AuthorityIdentifier,
        committee: Committee,
        header_store: HeaderStore,
        certificate_store: CertificateStore,
        signature_service: SignatureService<Signature, { crypto::INTENT_MESSAGE_LENGTH }>,
        metrics: Arc<PrimaryMetrics>,
        network: anemo::Network,
        header: Header,
        mut cancel: oneshot::Receiver<()>,
    ) -> DagResult<Certificate> {
        if header.epoch() != committee.epoch() {
            debug!(
                "Certifier received mismatched header proposal for epoch {}, currently at epoch {}",
                header.epoch(),
                committee.epoch()
            );
            return Err(DagError::InvalidEpoch {
                expected: committee.epoch(),
                received: header.epoch(),
            });
        }

        // Process the header.
        header_store.write(&header)?;
        metrics.proposed_header_round.set(header.round() as i64);

        // Reset the votes aggregator and sign our own header.
        let mut votes_aggregator = VotesAggregator::new(metrics.clone());
        let vote = Vote::new(&header, &authority_id, &signature_service).await;
        let mut certificate = votes_aggregator.append(vote, &committee, &header)?;

        // Trigger vote requests.
        let peers = committee
            .others_primaries_by_id(authority_id)
            .into_iter()
            .map(|(name, _, network_key)| (name, network_key));
        let mut requests: FuturesUnordered<_> = peers
            .map(|(name, target)| {
                let header = header.clone();
                Self::request_vote(
                    network.clone(),
                    committee.clone(),
                    certificate_store.clone(),
                    name,
                    target,
                    header,
                )
            })
            .collect();
        loop {
            if certificate.is_some() {
                break;
            }
            tokio::select! {
                result = &mut requests.next() => {
                    match result {
                        Some(Ok(vote)) => {
                            certificate = votes_aggregator.append(
                                vote,
                                &committee,
                                &header,
                            )?;
                        },
                        Some(Err(e)) => debug!("failed to get vote for header {header:?}: {e:?}"),
                        None => break,
                    }
                },
                _ = &mut cancel => {
                    debug!("canceling Header proposal {header} for round {}", header.round());
                    return Err(DagError::Canceled)
                },
            }
        }

        let certificate = certificate.ok_or_else(|| {
            // Log detailed header info if we failed to form a certificate.
            if enabled!(tracing::Level::WARN) {
                let mut msg = format!(
                    "Failed to form certificate from header {header:?} with parent certificates:\n"
                );
                for parent_digest in header.parents().iter() {
                    let parent_msg = match certificate_store.read(*parent_digest) {
                        Ok(Some(cert)) => format!("{cert:?}\n"),
                        Ok(None) => {
                            format!("!!!missing certificate for digest {parent_digest:?}!!!\n")
                        }
                        Err(e) => format!(
                            "!!!error retrieving certificate for digest {parent_digest:?}: {e:?}\n"
                        ),
                    };
                    msg.push_str(&parent_msg);
                }
                warn!(msg);
            }
            DagError::CouldNotFormCertificate(header.digest())
        })?;
        debug!("Assembled {certificate:?}");

        Ok(certificate)
    }

    // Logs Certifier errors as appropriate.
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

    // Main loop listening to incoming messages.
    pub async fn run(mut self) -> DagResult<Self> {
        info!(
            "Core on node {} has started successfully.",
            self.authority_id
        );
        loop {
            let result = tokio::select! {
                // We also receive here our new headers created by the `Proposer`.
                // TODO: move logic into Proposer.
                Some(header) = self.rx_headers.recv() => {
                    let (tx_cancel, rx_cancel) = oneshot::channel();
                    if let Some(cancel) = self.cancel_proposed_header {
                        let _ = cancel.send(());
                    }
                    self.cancel_proposed_header = Some(tx_cancel);

                    let name = self.authority_id;
                    let committee = self.committee.clone();
                    let header_store = self.header_store.clone();
                    let certificate_store = self.certificate_store.clone();
                    let signature_service = self.signature_service.clone();
                    let metrics = self.metrics.clone();
                    let network = self.network.clone();
                    fail_point_async!("narwhal-delay");
                    self.propose_header_tasks.spawn(monitored_future!(Self::propose_header(
                        name,
                        committee,
                        header_store,
                        certificate_store,
                        signature_service,
                        metrics,
                        network,
                        header,
                        rx_cancel,
                    )));
                    Ok(())
                },

                // Process certificates formed after receiving enough votes.
                // TODO: move logic into Proposer.
                Some(result) = self.propose_header_tasks.join_next() => {
                    match result {
                        Ok(Ok(certificate)) => {
                            fail_point_async!("narwhal-delay");
                            self.synchronizer.accept_own_certificate(certificate).await
                        },
                        Ok(Err(e)) => Err(e),
                        Err(e) => {
                            if e.is_cancelled() {
                                // Ungraceful shutdown.
                                Err(DagError::ShuttingDown)
                            } else if e.is_panic() {
                                // propagate panics.
                                std::panic::resume_unwind(e.into_panic());
                            } else {
                                panic!("propose header task failed: {e}");
                            }
                        },
                    }
                },

                _ = self.rx_shutdown.receiver.recv() => {
                    return Ok(self);
                }
            };

            Self::process_result(&result);
        }
    }
}
