// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    common::{create_db_stores, fixture_header_builder},
    helper::Helper,
    primary::PrimaryMessage,
};
use bincode::deserialize;
use crypto::{ed25519::Ed25519PublicKey, Hash};
use ed25519_dalek::Signer;
use futures::StreamExt;
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    time::Duration,
};
use test_utils::{certificate, fixture_batch_with_transactions, keys, resolve_name_and_committee};
use tokio::{net::TcpListener, sync::mpsc::channel, task::JoinHandle, time::timeout};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use types::CertificateDigest;

#[tokio::test]
async fn test_process_certificates_stream_mode() {
    // GIVEN
    let (_, certificate_store, _) = create_db_stores();
    let key = keys().pop().unwrap();
    let (name, committee) = resolve_name_and_committee(13010);
    let (tx_primaries, rx_primaries) = channel(10);

    // AND a helper
    Helper::spawn(committee.clone(), certificate_store.clone(), rx_primaries);

    // AND some mock certificates
    let mut certificates = HashMap::new();
    for _ in 0..5 {
        let header = fixture_header_builder()
            .with_payload_batch(fixture_batch_with_transactions(10), 0)
            .build(|payload| key.sign(payload));

        let certificate = certificate(&header);
        let id = certificate.clone().digest();

        // write the certificate
        certificate_store.write(id, certificate.clone()).await;

        certificates.insert(id, certificate.clone());
    }

    // AND spin up a mock node
    let address = committee.primary(&name).unwrap();
    let handler = listener(certificates.len(), address.primary_to_primary);

    // WHEN requesting the certificates
    tx_primaries
        .send(PrimaryMessage::CertificatesRequest(
            certificates.keys().copied().collect(),
            name,
        ))
        .await
        .expect("Couldn't send message");

    if let Ok(result) = timeout(Duration::from_millis(4_000), handler).await {
        assert!(result.is_ok(), "Error returned");

        let result_digests: HashSet<CertificateDigest> = result
            .unwrap()
            .into_iter()
            .map(|message| match message {
                PrimaryMessage::Certificate(certificate) => certificate,
                msg => {
                    panic!("Didn't expect message {:?}", msg);
                }
            })
            .map(|c| c.digest())
            .collect();

        assert_eq!(
            result_digests.len(),
            certificates.len(),
            "Returned unique number of certificates don't match the expected"
        );
    } else {
        panic!(
            "Timed out while waiting for results. Did not receive all the expected certificates."
        );
    }
}

#[tokio::test]
async fn test_process_certificates_batch_mode() {
    // GIVEN
    let (_, certificate_store, _) = create_db_stores();
    let key = keys().pop().unwrap();
    let (name, committee) = resolve_name_and_committee(13010);
    let (tx_primaries, rx_primaries) = channel(10);

    // AND a helper
    Helper::spawn(committee.clone(), certificate_store.clone(), rx_primaries);

    // AND some mock certificates
    let mut certificates = HashMap::new();
    let mut missing_certificates = HashSet::new();

    for i in 0..10 {
        let header = fixture_header_builder()
            .with_payload_batch(fixture_batch_with_transactions(10), 0)
            .build(|payload| key.sign(payload));

        let certificate = certificate(&header);
        let id = certificate.clone().digest();

        certificates.insert(id, certificate.clone());

        // We want to simulate the scenario of both having some certificates
        // found and some non found. Store only the half. The other half
        // should be returned back as non found.
        if i < 5 {
            // write the certificate
            certificate_store.write(id, certificate.clone()).await;
        } else {
            missing_certificates.insert(id);
        }
    }

    // AND spin up a mock node
    let address = committee.primary(&name).unwrap();
    let handler = listener(1, address.primary_to_primary);

    // WHEN requesting the certificates in batch mode
    tx_primaries
        .send(PrimaryMessage::CertificatesBatchRequest {
            certificate_ids: certificates.keys().copied().collect(),
            requestor: name,
        })
        .await
        .expect("Couldn't send message");

    if let Ok(result) = timeout(Duration::from_millis(4_000), handler).await {
        assert!(result.is_ok(), "Error returned");

        for message in result.unwrap() {
            match message {
                PrimaryMessage::CertificatesBatchResponse {
                    certificates: result_certificates,
                } => {
                    let result_digests: HashSet<CertificateDigest> = result_certificates
                        .iter()
                        .map(|(digest, _)| *digest)
                        .collect();

                    assert_eq!(
                        result_digests.len(),
                        certificates.len(),
                        "Returned unique number of certificates don't match the expected"
                    );

                    // ensure that we have non found certificates
                    let non_found_certificates: usize = result_certificates
                        .into_iter()
                        .filter(|(digest, certificate)| {
                            missing_certificates.contains(digest) && certificate.is_none()
                        })
                        .count();
                    assert_eq!(
                        non_found_certificates, 5,
                        "Expected to have non found certificates"
                    );
                }
                msg => {
                    panic!("Didn't expect message {:?}", msg);
                }
            }
        }
    } else {
        panic!(
            "Timed out while waiting for results. Did not receive all the expected certificates."
        );
    }
}

pub fn listener(
    num_of_expected_responses: usize,
    address: SocketAddr,
) -> JoinHandle<Vec<PrimaryMessage<Ed25519PublicKey>>> {
    tokio::spawn(async move {
        let listener = TcpListener::bind(&address).await.unwrap();
        let (socket, _) = listener.accept().await.unwrap();
        let transport = Framed::new(socket, LengthDelimitedCodec::new());
        let (_writer, mut reader) = transport.split();

        let mut responses = Vec::new();
        loop {
            match reader.next().await {
                Some(Ok(received)) => {
                    let message = received.freeze();
                    match deserialize(&message) {
                        Ok(msg) => {
                            responses.push(msg);

                            if responses.len() == num_of_expected_responses {
                                return responses;
                            }
                        }
                        Err(err) => {
                            panic!("Error occurred {err}");
                        }
                    }
                }
                _ => panic!("Failed to receive network message"),
            }
        }
    })
}
