// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    block_synchronizer::{
        handler::{BlockSynchronizerHandler, Error, Handler},
        SyncError,
    },
    common::create_db_stores,
    BlockHeader, MockBlockSynchronizer,
};
use fastcrypto::hash::Hash;
use std::{collections::HashSet, time::Duration};
use test_utils::{fixture_payload, latest_protocol_version, CommitteeFixture};
use tokio::sync::mpsc;
use types::{CertificateAPI, CertificateDigest, Header, HeaderAPI};

#[tokio::test]
async fn test_get_and_synchronize_block_headers_when_fetched_from_storage() {
    // GIVEN
    let (_, certificate_store, _) = create_db_stores();
    let (tx_block_synchronizer, rx_block_synchronizer) = test_utils::test_channel!(1);
    let (tx_certificate_synchronizer, _rx_certificate_synchronizer) = mpsc::channel(1);

    let synchronizer = BlockSynchronizerHandler {
        tx_block_synchronizer,
        tx_certificate_synchronizer,
        certificate_store: certificate_store.clone(),
        certificate_deliver_timeout: Duration::from_millis(2_000),
    };

    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let author = fixture.authorities().next().unwrap();

    // AND dummy certificate
    let header = Header::V1(
        author
            .header_builder(&committee)
            .payload(fixture_payload(1, &latest_protocol_version()))
            .build()
            .unwrap(),
    );
    let certificate = fixture.certificate(&header);

    // AND
    let digests = vec![CertificateDigest::default()];

    // AND mock the block_synchronizer
    let mock_synchronizer = MockBlockSynchronizer::new(rx_block_synchronizer);
    let expected_result = vec![Ok(BlockHeader {
        certificate: certificate.clone(),
        fetched_from_storage: true,
    })];
    mock_synchronizer
        .expect_synchronize_block_headers(digests.clone(), expected_result, 1)
        .await;

    // WHEN
    let result = synchronizer
        .get_and_synchronize_block_headers(digests)
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
    let (tx_block_synchronizer, rx_block_synchronizer) = test_utils::test_channel!(1);
    let (tx_certificate_synchronizer, mut rx_certificate_synchronizer) = mpsc::channel(1);

    let synchronizer = BlockSynchronizerHandler {
        tx_block_synchronizer,
        tx_certificate_synchronizer,
        certificate_store: certificate_store.clone(),
        certificate_deliver_timeout: Duration::from_millis(2_000),
    };

    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let author = fixture.authorities().next().unwrap();

    // AND a certificate stored
    let header = Header::V1(
        author
            .header_builder(&committee)
            .payload(fixture_payload(1, &latest_protocol_version()))
            .build()
            .unwrap(),
    );
    let cert_stored = fixture.certificate(&header);
    certificate_store.write(cert_stored.clone()).unwrap();

    // AND a certificate NOT stored
    let header = Header::V1(
        author
            .header_builder(&committee)
            .payload(fixture_payload(2, &latest_protocol_version()))
            .build()
            .unwrap(),
    );
    let cert_missing = fixture.certificate(&header);

    // AND
    let mut digests = HashSet::new();
    digests.insert(cert_stored.digest());
    digests.insert(cert_missing.digest());

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
            digests
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
        match rx_certificate_synchronizer.recv().await {
            Some(c) => {
                assert_eq!(c.digest(), cert_missing.digest());
                certificate_store.write(c).unwrap();
            }
            _ => panic!("Didn't receive certificate message"),
        }
    });

    // WHEN
    let result = synchronizer
        .get_and_synchronize_block_headers(
            digests
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
        assert!(digests.contains(&r.unwrap().digest()))
    }

    // AND
    mock_synchronizer.assert_expectations().await;
}

#[tokio::test]
async fn test_get_and_synchronize_block_headers_timeout_on_causal_completion() {
    // GIVEN
    let (_, certificate_store, _) = create_db_stores();
    let (tx_block_synchronizer, rx_block_synchronizer) = test_utils::test_channel!(1);
    let (tx_certificate_synchronizer, _rx_certificate_synchronizer) = mpsc::channel(1);

    let synchronizer = BlockSynchronizerHandler {
        tx_block_synchronizer,
        tx_certificate_synchronizer,
        certificate_store: certificate_store.clone(),
        certificate_deliver_timeout: Duration::from_millis(2_000),
    };

    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let author = fixture.authorities().next().unwrap();

    // AND a certificate stored
    let header = Header::V1(
        author
            .header_builder(&committee)
            .payload(fixture_payload(1, &latest_protocol_version()))
            .build()
            .unwrap(),
    );
    let cert_stored = fixture.certificate(&header);
    certificate_store.write(cert_stored.clone()).unwrap();

    // AND a certificate NOT stored
    let header = Header::V1(
        author
            .header_builder(&committee)
            .payload(fixture_payload(2, &latest_protocol_version()))
            .build()
            .unwrap(),
    );
    let cert_missing = fixture.certificate(&header);

    // AND
    let digests = vec![cert_stored.digest(), cert_missing.digest()];

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
        .expect_synchronize_block_headers(digests.clone(), expected_result, 1)
        .await;

    // WHEN
    let result = synchronizer
        .get_and_synchronize_block_headers(digests)
        .await;

    // THEN
    assert_eq!(result.len(), 2);

    // AND
    for r in result {
        if let Ok(cert) = r {
            assert_eq!(cert_stored.digest(), cert.digest());
        } else {
            match r.err().unwrap() {
                Error::BlockDeliveryTimeout { digest } => {
                    assert_eq!(cert_missing.digest(), digest)
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
    let (tx_block_synchronizer, rx_block_synchronizer) = test_utils::test_channel!(1);
    let (tx_certificate_synchronizer, _rx_certificate_synchronizer) = mpsc::channel(1);

    let synchronizer = BlockSynchronizerHandler {
        tx_block_synchronizer,
        tx_certificate_synchronizer,
        certificate_store: certificate_store.clone(),
        certificate_deliver_timeout: Duration::from_millis(2_000),
    };

    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let author = fixture.authorities().next().unwrap();

    // AND a certificate with payload already available
    let header = Header::V1(
        author
            .header_builder(&committee)
            .payload(fixture_payload(1, &latest_protocol_version()))
            .build()
            .unwrap(),
    );
    let cert_stored = fixture.certificate(&header);
    for (digest, (worker_id, _)) in cert_stored.clone().header().payload() {
        payload_store.write(digest, worker_id).unwrap();
    }

    // AND a certificate with payload NOT available
    let header = Header::V1(
        author
            .header_builder(&committee)
            .payload(fixture_payload(2, &latest_protocol_version()))
            .build()
            .unwrap(),
    );
    let cert_missing = fixture.certificate(&header);

    // AND
    let digests = vec![cert_stored.digest(), cert_missing.digest()];

    // AND mock the block_synchronizer where the certificate is fetched
    // from peers (fetched_from_storage = false)
    let mock_synchronizer = MockBlockSynchronizer::new(rx_block_synchronizer);
    let expected_result = vec![
        Ok(BlockHeader {
            certificate: cert_stored.clone(),
            fetched_from_storage: true,
        }),
        Err(SyncError::NoResponse {
            digest: cert_missing.digest(),
        }),
    ];
    mock_synchronizer
        .expect_synchronize_block_payload(digests.clone(), expected_result, 1)
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
                Error::PayloadSyncError { digest, .. } => {
                    assert_eq!(cert_missing.digest(), digest)
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
    let (tx_block_synchronizer, _) = test_utils::test_channel!(1);
    let (tx_certificate_synchronizer, _rx_certificate_synchronizer) = mpsc::channel(1);

    let synchronizer = BlockSynchronizerHandler {
        tx_block_synchronizer,
        tx_certificate_synchronizer,
        certificate_store: certificate_store.clone(),
        certificate_deliver_timeout: Duration::from_millis(2_000),
    };

    let result = synchronizer.synchronize_block_payloads(vec![]).await;
    assert!(result.is_empty());

    let result = synchronizer.get_and_synchronize_block_headers(vec![]).await;
    assert!(result.is_empty());
}
