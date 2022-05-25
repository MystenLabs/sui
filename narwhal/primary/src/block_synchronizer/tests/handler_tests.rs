// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    block_synchronizer::{
        handler::{BlockSynchronizerHandler, Error, Handler},
        SyncError,
    },
    common::create_db_stores,
    BlockHeader, MockBlockSynchronizer,
};
use crypto::{ed25519::Ed25519PublicKey, Hash};
use std::{collections::HashSet, time::Duration};
use test_utils::{certificate, fixture_header_with_payload};
use tokio::sync::mpsc::channel;
use types::{Certificate, CertificateDigest, PrimaryMessage};

#[tokio::test]
async fn test_get_and_synchronize_block_headers_when_fetched_from_storage() {
    // GIVEN
    let (_, certificate_store, _) = create_db_stores();
    let (tx_block_synchronizer, rx_block_synchronizer) = channel(1);
    let (tx_core, _rx_core) = channel(1);

    let synchronizer = BlockSynchronizerHandler {
        tx_block_synchronizer,
        tx_core,
        certificate_store: certificate_store.clone(),
        certificate_deliver_timeout: Duration::from_millis(2_000),
    };

    // AND dummy certificate
    let certificate = certificate(&fixture_header_with_payload(1));

    // AND
    let block_ids = vec![CertificateDigest::default()];

    // AND mock the block_synchronizer
    let mock_synchronizer = MockBlockSynchronizer::new(rx_block_synchronizer);
    let expected_result = vec![Ok(BlockHeader {
        certificate: certificate.clone(),
        fetched_from_storage: true,
    })];
    mock_synchronizer
        .expect_synchronize_block_headers(block_ids.clone(), expected_result, 1)
        .await;

    // WHEN
    let result = synchronizer
        .get_and_synchronize_block_headers(block_ids)
        .await;

    // THEN
    assert_eq!(result.len(), 1);

    // AND
    if let Ok(result_certificate) = result.first().unwrap().to_owned() {
        assert_eq!(result_certificate, certificate, "Certificates do not match");
    } else {
        panic!("Should have received the certificate successfully");
    }

    // AND
    mock_synchronizer.assert_expectations().await;
}

#[tokio::test]
async fn test_get_and_synchronize_block_headers_when_fetched_from_peers() {
    // GIVEN
    let (_, certificate_store, _) = create_db_stores();
    let (tx_block_synchronizer, rx_block_synchronizer) = channel(1);
    let (tx_core, mut rx_core) = channel(1);

    let synchronizer = BlockSynchronizerHandler {
        tx_block_synchronizer,
        tx_core,
        certificate_store: certificate_store.clone(),
        certificate_deliver_timeout: Duration::from_millis(2_000),
    };

    // AND a certificate stored
    let cert_stored = certificate(&fixture_header_with_payload(1));
    certificate_store
        .write(cert_stored.digest(), cert_stored.clone())
        .await;

    // AND a certificate NOT stored
    let cert_missing = certificate(&fixture_header_with_payload(2));

    // AND
    let mut block_ids = HashSet::new();
    block_ids.insert(cert_stored.digest());
    block_ids.insert(cert_missing.digest());

    // AND mock the block_synchronizer where the certificate is fetched
    // from peers (fetched_from_storage = false)
    let mock_synchronizer = MockBlockSynchronizer::new(rx_block_synchronizer);
    let expected_result = vec![
        Ok(BlockHeader {
            certificate: cert_stored.clone(),
            fetched_from_storage: true,
        }),
        Ok(BlockHeader {
            certificate: cert_missing.clone(),
            fetched_from_storage: false,
        }),
    ];
    mock_synchronizer
        .expect_synchronize_block_headers(
            block_ids
                .clone()
                .into_iter()
                .collect::<Vec<CertificateDigest>>(),
            expected_result,
            1,
        )
        .await;

    // AND mock the "core" module. We assume that the certificate will be
    // stored after validated and causally complete the history.
    tokio::spawn(async move {
        match rx_core.recv().await {
            Some(PrimaryMessage::Certificate(c)) => {
                assert_eq!(c.digest(), cert_missing.digest());
                certificate_store.write(c.digest(), c).await;
            }
            _ => panic!("Didn't receive certificate message"),
        }
    });

    // WHEN
    let result = synchronizer
        .get_and_synchronize_block_headers(
            block_ids
                .clone()
                .into_iter()
                .collect::<Vec<CertificateDigest>>(),
        )
        .await;

    // THEN
    assert_eq!(result.len(), 2);

    // AND
    for r in result {
        assert!(r.is_ok());
        assert!(block_ids.contains(&r.unwrap().digest()))
    }

    // AND
    mock_synchronizer.assert_expectations().await;
}

#[tokio::test]
async fn test_get_and_synchronize_block_headers_timeout_on_causal_completion() {
    // GIVEN
    let (_, certificate_store, _) = create_db_stores();
    let (tx_block_synchronizer, rx_block_synchronizer) = channel(1);
    let (tx_core, _rx_core) = channel(1);

    let synchronizer = BlockSynchronizerHandler {
        tx_block_synchronizer,
        tx_core,
        certificate_store: certificate_store.clone(),
        certificate_deliver_timeout: Duration::from_millis(2_000),
    };

    // AND a certificate stored
    let cert_stored = certificate(&fixture_header_with_payload(1));
    certificate_store
        .write(cert_stored.digest(), cert_stored.clone())
        .await;

    // AND a certificate NOT stored
    let cert_missing = certificate(&fixture_header_with_payload(2));

    // AND
    let block_ids = vec![cert_stored.digest(), cert_missing.digest()];

    // AND mock the block_synchronizer where the certificate is fetched
    // from peers (fetched_from_storage = false)
    let mock_synchronizer = MockBlockSynchronizer::new(rx_block_synchronizer);
    let expected_result = vec![
        Ok(BlockHeader {
            certificate: cert_stored.clone(),
            fetched_from_storage: true,
        }),
        Ok(BlockHeader {
            certificate: cert_missing.clone(),
            fetched_from_storage: false,
        }),
    ];
    mock_synchronizer
        .expect_synchronize_block_headers(block_ids.clone(), expected_result, 1)
        .await;

    // WHEN
    let result = synchronizer
        .get_and_synchronize_block_headers(block_ids)
        .await;

    // THEN
    assert_eq!(result.len(), 2);

    // AND
    for r in result {
        if let Ok(cert) = r {
            assert_eq!(cert_stored.digest(), cert.digest());
        } else {
            match r.err().unwrap() {
                Error::BlockDeliveryTimeout { block_id } => {
                    assert_eq!(cert_missing.digest(), block_id)
                }
                _ => panic!("Unexpected error returned"),
            }
        }
    }

    // AND
    mock_synchronizer.assert_expectations().await;
}

#[tokio::test]
async fn test_synchronize_block_payload() {
    // GIVEN
    let (_, certificate_store, payload_store) = create_db_stores();
    let (tx_block_synchronizer, rx_block_synchronizer) = channel(1);
    let (tx_core, _rx_core) = channel(1);

    let synchronizer = BlockSynchronizerHandler {
        tx_block_synchronizer,
        tx_core,
        certificate_store: certificate_store.clone(),
        certificate_deliver_timeout: Duration::from_millis(2_000),
    };

    // AND a certificate with payload already available
    let cert_stored: Certificate<Ed25519PublicKey> = certificate(&fixture_header_with_payload(1));
    for e in cert_stored.clone().header.payload {
        payload_store.write(e, 1).await;
    }

    // AND a certificate with payload NOT available
    let cert_missing = certificate(&fixture_header_with_payload(2));

    // AND
    let block_ids = vec![cert_stored.digest(), cert_missing.digest()];

    // AND mock the block_synchronizer where the certificate is fetched
    // from peers (fetched_from_storage = false)
    let mock_synchronizer = MockBlockSynchronizer::new(rx_block_synchronizer);
    let expected_result = vec![
        Ok(BlockHeader {
            certificate: cert_stored.clone(),
            fetched_from_storage: true,
        }),
        Err(SyncError::NoResponse {
            block_id: cert_missing.digest(),
        }),
    ];
    mock_synchronizer
        .expect_synchronize_block_payload(block_ids.clone(), expected_result, 1)
        .await;

    // WHEN
    let result = synchronizer
        .synchronize_block_payloads(vec![cert_stored.clone(), cert_missing.clone()])
        .await;

    // THEN
    assert_eq!(result.len(), 2);

    // AND
    for r in result {
        if let Ok(cert) = r {
            assert_eq!(cert_stored.digest(), cert.digest());
        } else {
            match r.err().unwrap() {
                Error::PayloadSyncError { block_id, .. } => {
                    assert_eq!(cert_missing.digest(), block_id)
                }
                _ => panic!("Unexpected error returned"),
            }
        }
    }

    // AND
    mock_synchronizer.assert_expectations().await;
}

#[tokio::test]
async fn test_call_methods_with_empty_input() {
    // GIVEN
    let (_, certificate_store, _) = create_db_stores();
    let (tx_block_synchronizer, _) = channel(1);
    let (tx_core, _rx_core) = channel(1);

    let synchronizer = BlockSynchronizerHandler {
        tx_block_synchronizer,
        tx_core,
        certificate_store: certificate_store.clone(),
        certificate_deliver_timeout: Duration::from_millis(2_000),
    };

    let result = synchronizer.synchronize_block_payloads(vec![]).await;
    assert!(result.is_empty());

    let result = synchronizer.get_and_synchronize_block_headers(vec![]).await;
    assert!(result.is_empty());
}
