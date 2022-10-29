// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::primary::PrimaryMessage;
use config::Committee;
use crypto::PublicKey;
use network::{P2pNetwork, UnreliableNetwork};
use storage::CertificateStore;
use store::StoreError;
use thiserror::Error;
use tokio::{sync::watch, task::JoinHandle};
use tracing::{error, info, instrument};
use types::{metered_channel::Receiver, Certificate, CertificateDigest, ReconfigureNotification};

#[cfg(test)]
#[path = "tests/helper_tests.rs"]
mod helper_tests;

#[derive(Debug, Error)]
enum HelperError {
    #[error("Storage failure: {0}")]
    StoreError(#[from] StoreError),

    #[error("Invalid request received: {0}")]
    InvalidRequest(String),
}

/// A task dedicated to help other authorities by replying to their certificate &
/// payload availability requests.
pub struct Helper {
    /// The node's name
    name: PublicKey,
    /// The committee information.
    committee: Committee,
    /// The certificate persistent storage.
    certificate_store: CertificateStore,
    /// Watch channel to reconfigure the committee.
    rx_reconfigure: watch::Receiver<ReconfigureNotification>,
    /// Input channel to receive requests.
    rx_helper_requests: Receiver<PrimaryMessage>,
    /// A network sender to reply to the sync requests.
    primary_network: P2pNetwork,
}

impl Helper {
    #[must_use]
    pub fn spawn(
        name: PublicKey,
        committee: Committee,
        certificate_store: CertificateStore,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        rx_helper_requests: Receiver<PrimaryMessage>,
        primary_network: P2pNetwork,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                committee,
                certificate_store,
                rx_reconfigure,
                rx_helper_requests,
                primary_network,
            }
            .run()
            .await;
        })
    }

    async fn run(&mut self) {
        info!(
            "Helper for availability requests on node {} has started successfully.",
            self.name
        );
        loop {
            tokio::select! {
                Some(request) = self.rx_helper_requests.recv() => match request {
                    // The CertificatesRequest will find any certificates that exist in
                    // the data source (dictated by the digests parameter). The results
                    // will be emitted one by one to the consumer.
                    PrimaryMessage::CertificatesRequest(digests, origin) => {
                        let _ = self.process_certificates(digests, origin, false).await;
                    }
                    // The CertificatesBatchRequest will find any certificates that exist in
                    // the data source (dictated by the digests parameter). The results will
                    // be sent though back to the consumer as a batch - one message.
                    PrimaryMessage::CertificatesBatchRequest {
                        certificate_ids,
                        requestor,
                    } => {
                        let _ = self
                            .process_certificates(certificate_ids, requestor, true)
                            .await;
                    }
                    _ => {
                        panic!("Received unexpected message!");
                    }
                },

                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    match message {
                        ReconfigureNotification::NewEpoch(new_committee) => {
                            self.committee = new_committee;
                        },
                        ReconfigureNotification::UpdateCommittee(new_committee) => {
                            self.committee = new_committee;
                        },
                        ReconfigureNotification::Shutdown => return
                    }
                    tracing::debug!("Committee updated to {}", self.committee);
                }
            }
        }
    }

    #[instrument(level="debug", skip_all, fields(origin = ?origin, num_certificate_ids = digests.len(), mode = batch_mode), err)]
    async fn process_certificates(
        &mut self,
        digests: Vec<CertificateDigest>,
        origin: PublicKey,
        batch_mode: bool,
    ) -> Result<(), HelperError> {
        if digests.is_empty() {
            return Err(HelperError::InvalidRequest(
                "empty digests received - ignore request".to_string(),
            ));
        }

        // TODO [issue #195]: Do some accounting to prevent bad nodes from monopolizing our resources.
        let certificates = match self.certificate_store.read_all(digests.to_owned()) {
            Ok(certificates) => certificates,
            Err(err) => {
                error!("Error while retrieving certificates: {err}");
                vec![]
            }
        };

        // When batch_mode = true, then the requested certificates will be sent back
        // to the consumer as one message over the network. For the non found
        // certificates only the digest will be sent instead.
        //
        // When batch_mode = false, then the requested certificates will be sent
        // back to the consumer as separate messages one by one. If a certificate
        // has not been found, then no message will be sent.
        if batch_mode {
            let response: Vec<(CertificateDigest, Option<Certificate>)> = if certificates.is_empty()
            {
                digests.into_iter().map(|c| (c, None)).collect()
            } else {
                digests.into_iter().zip(certificates).collect()
            };

            let message = PrimaryMessage::CertificatesBatchResponse {
                certificates: response,
                from: self.name.clone(),
            };

            let _ = self
                .primary_network
                .unreliable_send(self.committee.network_key(&origin).unwrap(), &message);
        } else {
            for certificate in certificates.into_iter().flatten() {
                // TODO: Remove this deserialization-serialization in the critical path.
                let message = PrimaryMessage::Certificate(certificate);
                let _ = self
                    .primary_network
                    .unreliable_send(self.committee.network_key(&origin).unwrap(), &message);
            }
        }

        Ok(())
    }
}
