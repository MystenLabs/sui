// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{primary::PrimaryMessage, PayloadToken};
use config::{Committee, WorkerId};
use crypto::traits::{EncodeDecodeBase64, VerifyingKey};
use network::PrimaryNetwork;
use store::{Store, StoreError};
use thiserror::Error;
use tokio::{
    sync::{mpsc::Receiver, watch},
    task::JoinHandle,
};
use tracing::{error, instrument};
use types::{BatchDigest, Certificate, CertificateDigest, ReconfigureNotification};

#[cfg(test)]
#[path = "tests/helper_tests.rs"]
mod helper_tests;

#[derive(Debug, Error)]
enum HelperError {
    #[error("Received message from unknown authority {0}")]
    UnknownAuthority(String),

    #[error("Storage failure: {0}")]
    StoreError(#[from] StoreError),

    #[error("Invalid request received: {0}")]
    InvalidRequest(String),
}

/// A task dedicated to help other authorities by replying to their certificate &
/// payload availability requests.
pub struct Helper<PublicKey: VerifyingKey> {
    /// The node's name
    name: PublicKey,
    /// The committee information.
    committee: Committee<PublicKey>,
    /// The certificate persistent storage.
    certificate_store: Store<CertificateDigest, Certificate<PublicKey>>,
    /// The payloads (batches) persistent storage.
    payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
    /// Watch channel to reconfigure the committee.
    rx_committee: watch::Receiver<ReconfigureNotification<PublicKey>>,
    /// Input channel to receive requests.
    rx_primaries: Receiver<PrimaryMessage<PublicKey>>,
    /// A network sender to reply to the sync requests.
    primary_network: PrimaryNetwork,
}

impl<PublicKey: VerifyingKey> Helper<PublicKey> {
    pub fn spawn(
        name: PublicKey,
        committee: Committee<PublicKey>,
        certificate_store: Store<CertificateDigest, Certificate<PublicKey>>,
        payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
        rx_committee: watch::Receiver<ReconfigureNotification<PublicKey>>,
        rx_primaries: Receiver<PrimaryMessage<PublicKey>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                committee,
                certificate_store,
                payload_store,
                rx_committee,
                rx_primaries,
                primary_network: PrimaryNetwork::default(),
            }
            .run()
            .await;
        })
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                Some(request) = self.rx_primaries.recv() => match request {
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
                    // A request that another primary sends us to ask whether we
                    // can serve batch data for the provided certificate_ids.
                    PrimaryMessage::PayloadAvailabilityRequest {
                        certificate_ids,
                        requestor,
                    } => {
                        let _ = self
                            .process_payload_availability(certificate_ids, requestor)
                            .await;
                    }
                    _ => {
                        panic!("Received unexpected message!");
                    }
                },

                result = self.rx_committee.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_committee.borrow().clone();
                    match message {
                        ReconfigureNotification::NewCommittee(new_committee) => {
                            self.committee = new_committee;
                            tracing::debug!("Committee updated to {}", self.committee);
                        },
                        ReconfigureNotification::Shutdown => return
                    }
                }
            }
        }
    }

    /// Processes a payload availability request by checking we have the
    /// certificate & batch data for each certificate digest in digests,
    /// and reports on each fully available item in the request in a
    /// PayloadAvailabilityResponse.
    #[instrument(level="debug", skip_all, fields(num_certificate_ids = digests.len()), err)]
    async fn process_payload_availability(
        &mut self,
        digests: Vec<CertificateDigest>,
        origin: PublicKey,
    ) -> Result<(), HelperError> {
        // get the requestor's address.
        let address = match self.committee.primary(&origin) {
            Ok(x) => x.primary_to_primary,
            Err(_) => {
                return Err(HelperError::UnknownAuthority(origin.encode_base64()));
            }
        };

        let mut result: Vec<(CertificateDigest, bool)> = Vec::new();

        let certificates = match self.certificate_store.read_all(digests.to_owned()).await {
            Ok(certificates) => certificates,
            Err(err) => {
                // just return at this point. Send back to the requestor
                // that we don't have availability - ideally we would like
                // to communicate an error (so they could potentially retry).
                result = digests.into_iter().map(|d| (d, false)).collect();

                let message = PrimaryMessage::<PublicKey>::PayloadAvailabilityResponse {
                    payload_availability: result,
                    from: self.name.clone(),
                };
                self.primary_network
                    .unreliable_send(address, &message)
                    .await;

                return Err(HelperError::StoreError(err));
            }
        };

        for (id, certificate_option) in digests.into_iter().zip(certificates) {
            // Find the batches only for the certificates that exist
            if let Some(certificate) = certificate_option {
                let payload_available = match self
                    .payload_store
                    .read_all(certificate.header.payload)
                    .await
                {
                    Ok(payload_result) => payload_result.into_iter().all(|x| x.is_some()),
                    Err(err) => {
                        // we'll assume that we don't have available the payloads,
                        // otherwise and error response should be sent back.
                        error!("Error while retrieving payloads: {err}");
                        false
                    }
                };

                result.push((id, payload_available));
            } else {
                // We don't have the certificate available in first place,
                // so we can't even look up for the batches.
                result.push((id, false));
            }
        }

        // now send the result back to the requestor
        let message = PrimaryMessage::<PublicKey>::PayloadAvailabilityResponse {
            payload_availability: result,
            from: self.name.clone(),
        };
        self.primary_network
            .unreliable_send(address, &message)
            .await;

        Ok(())
    }

    #[instrument(level="debug", skip_all, fields(num_certificate_ids = digests.len(), mode = batch_mode), err)]
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

        // get the requestor's address.
        let address = match self.committee.primary(&origin) {
            Ok(x) => x.primary_to_primary,
            Err(_) => {
                return Err(HelperError::UnknownAuthority(origin.encode_base64()));
            }
        };

        // TODO [issue #195]: Do some accounting to prevent bad nodes from monopolizing our resources.
        let certificates = match self.certificate_store.read_all(digests.to_owned()).await {
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
            let response: Vec<(CertificateDigest, Option<Certificate<PublicKey>>)> =
                if certificates.is_empty() {
                    digests.into_iter().map(|c| (c, None)).collect()
                } else {
                    digests.into_iter().zip(certificates).collect()
                };

            let message = PrimaryMessage::CertificatesBatchResponse {
                certificates: response,
                from: self.name.clone(),
            };

            self.primary_network
                .unreliable_send(address.clone(), &message)
                .await;
        } else {
            for certificate in certificates.into_iter().flatten() {
                // TODO: Remove this deserialization-serialization in the critical path.
                let message = PrimaryMessage::Certificate(certificate);
                self.primary_network
                    .unreliable_send(address.clone(), &message)
                    .await;
            }
        }

        Ok(())
    }
}
