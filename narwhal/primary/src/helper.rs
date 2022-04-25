// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::primary::PrimaryMessage;
use config::Committee;
use crypto::traits::VerifyingKey;
use network::PrimaryNetwork;
use store::Store;
use tokio::sync::mpsc::Receiver;
use tracing::{error, warn};
use types::{Certificate, CertificateDigest};

#[cfg(test)]
#[path = "tests/helper_tests.rs"]
mod helper_tests;

/// A task dedicated to help other authorities by replying to their certificates requests.
pub struct Helper<PublicKey: VerifyingKey> {
    /// The committee information.
    committee: Committee<PublicKey>,
    /// The persistent storage.
    store: Store<CertificateDigest, Certificate<PublicKey>>,
    /// Input channel to receive certificates requests.
    rx_primaries: Receiver<PrimaryMessage<PublicKey>>,
    /// A network sender to reply to the sync requests.
    primary_network: PrimaryNetwork,
}

impl<PublicKey: VerifyingKey> Helper<PublicKey> {
    pub fn spawn(
        committee: Committee<PublicKey>,
        store: Store<CertificateDigest, Certificate<PublicKey>>,
        rx_primaries: Receiver<PrimaryMessage<PublicKey>>,
    ) {
        tokio::spawn(async move {
            Self {
                committee,
                store,
                rx_primaries,
                primary_network: PrimaryNetwork::default(),
            }
            .run()
            .await;
        });
    }

    async fn run(&mut self) {
        while let Some(request) = self.rx_primaries.recv().await {
            match request {
                // The CertificatesRequest will find any certificates that exist in
                // the data source (dictated by the digests parameter). The results
                // will be emitted one by one to the consumer.
                PrimaryMessage::CertificatesRequest(digests, origin) => {
                    self.process_certificates(digests, origin, false).await;
                }
                // The CertificatesBatchRequest will find any certificates that exist in
                // the data source (dictated by the digests parameter). The results will
                // be sent though back to the consumer as a batch - one message.
                PrimaryMessage::CertificatesBatchRequest {
                    certificate_ids,
                    requestor,
                } => {
                    self.process_certificates(certificate_ids, requestor, true)
                        .await;
                }
                _ => {
                    panic!("Received unexpected message!");
                }
            }
        }
    }

    async fn process_certificates(
        &mut self,
        digests: Vec<CertificateDigest>,
        origin: PublicKey,
        batch_mode: bool,
    ) {
        // get the requestors address.
        let address = match self.committee.primary(&origin) {
            Ok(x) => x.primary_to_primary,
            Err(e) => {
                warn!("Unexpected certificate request: {e}");
                return;
            }
        };

        // TODO [issue #195]: Do some accounting to prevent bad nodes from monopolizing our resources.

        let certificates = match self.store.read_all(digests.to_owned()).await {
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
                digests.into_iter().zip(certificates).collect();

            let message = PrimaryMessage::CertificatesBatchResponse {
                certificates: response,
            };

            self.primary_network
                .unreliable_send(address, &message)
                .await;
        } else {
            for certificate in certificates {
                if certificate.is_some() {
                    // TODO: Remove this deserialization-serialization in the critical path.
                    let message = PrimaryMessage::Certificate(certificate.unwrap());
                    self.primary_network
                        .unreliable_send(address, &message)
                        .await;
                }
            }
        }
    }
}
