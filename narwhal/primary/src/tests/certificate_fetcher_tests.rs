// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    certificate_fetcher::CertificateFetcher, consensus::ConsensusRound, metrics::PrimaryMetrics,
    primary::NUM_SHUTDOWN_RECEIVERS, synchronizer::Synchronizer, PrimaryChannelMetrics,
};
use anemo::async_trait;
use anyhow::Result;
use config::{AuthorityIdentifier, Epoch, WorkerId};
use crypto::AggregateSignatureBytes;
use fastcrypto::{hash::Hash, traits::KeyPair};
use indexmap::IndexMap;
use itertools::Itertools;
use network::client::NetworkClient;
use once_cell::sync::OnceCell;
use prometheus::Registry;
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use storage::CertificateStore;
use storage::NodeStorage;
use test_utils::{get_protocol_config, latest_protocol_version, temp_dir, CommitteeFixture};
use tokio::{
    sync::{
        mpsc::{self, error::TryRecvError, Receiver, Sender},
        watch, Mutex,
    },
    time::sleep,
};
use types::HeaderV1;
use types::TimestampMs;
use types::{
    BatchDigest, Certificate, CertificateAPI, CertificateDigest, FetchCertificatesRequest,
    FetchCertificatesResponse, Header, HeaderAPI, HeaderDigest, PreSubscribedBroadcastSender,
    PrimaryToPrimary, PrimaryToPrimaryServer, RequestVoteRequest, RequestVoteResponse, Round,
    SendCertificateRequest, SendCertificateResponse, SignatureVerificationState,
};

pub struct NetworkProxy {
    request: Sender<FetchCertificatesRequest>,
    response: Arc<Mutex<Receiver<FetchCertificatesResponse>>>,
}

#[async_trait]
impl PrimaryToPrimary for NetworkProxy {
    async fn send_certificate(
        &self,
        request: anemo::Request<SendCertificateRequest>,
    ) -> Result<anemo::Response<SendCertificateResponse>, anemo::rpc::Status> {
        unimplemented!(
            "FetchCertificateProxy::send_certificate() is unimplemented!! {:#?}",
            request
        );
    }

    async fn request_vote(
        &self,
        _request: anemo::Request<RequestVoteRequest>,
    ) -> Result<anemo::Response<RequestVoteResponse>, anemo::rpc::Status> {
        unimplemented!()
    }

    async fn fetch_certificates(
        &self,
        request: anemo::Request<FetchCertificatesRequest>,
    ) -> Result<anemo::Response<FetchCertificatesResponse>, anemo::rpc::Status> {
        self.request
            .send(request.into_body())
            .await
            .map_err(|e| anemo::rpc::Status::from_error(Box::new(e)))?;
        Ok(anemo::Response::new(
            self.response.lock().await.recv().await.unwrap(),
        ))
    }
}

async fn verify_certificates_v2_in_store(
    certificate_store: &CertificateStore,
    certificates: &[Certificate],
    expected_verified_directly_count: u64,
    expected_verified_indirectly_count: u64,
) {
    let mut missing = None;
    let mut verified_indirectly = 0;
    let mut verified_directly = 0;
    for _ in 0..20 {
        missing = None;
        verified_directly = 0;
        verified_indirectly = 0;
        for (i, _) in certificates.iter().enumerate() {
            if let Ok(Some(cert)) = certificate_store.read(certificates[i].digest()) {
                match cert.signature_verification_state() {
                    SignatureVerificationState::VerifiedDirectly(_) => verified_directly += 1,
                    SignatureVerificationState::VerifiedIndirectly(_) => verified_indirectly += 1,
                    _ => panic!(
                        "Found unexpected stored signature state {:?}",
                        cert.signature_verification_state()
                    ),
                };
                continue;
            }
            missing = Some(i);
            break;
        }
        if missing.is_none() {
            break;
        }
        sleep(Duration::from_secs(1)).await;
    }
    if let Some(i) = missing {
        panic!(
            "Missing certificate in store: input index {}, certificate: {:?}",
            i, certificates[i]
        );
    }

    assert_eq!(
        verified_directly, expected_verified_directly_count,
        "Verified {} certificates directly in the store, expected {}",
        verified_directly, expected_verified_directly_count
    );
    assert_eq!(
        verified_indirectly, expected_verified_indirectly_count,
        "Verified {} certificates indirectly in the store, expected {}",
        verified_indirectly, expected_verified_indirectly_count
    );
}

async fn verify_certificates_v1_in_store(
    certificate_store: &CertificateStore,
    certificates: &[Certificate],
) {
    let mut missing = None;
    for _ in 0..20 {
        missing = None;
        for (i, _) in certificates.iter().enumerate() {
            if let Ok(Some(_)) = certificate_store.read(certificates[i].digest()) {
                continue;
            }
            missing = Some(i);
            break;
        }
        if missing.is_none() {
            break;
        }
        sleep(Duration::from_secs(1)).await;
    }
    if let Some(i) = missing {
        panic!(
            "Missing certificate in store: input index {}, certificate: {:?}",
            i, certificates[i]
        );
    }
}

fn verify_certificates_not_in_store(
    certificate_store: &CertificateStore,
    certificates: &[Certificate],
) {
    let found_certificates = certificate_store
        .read_all(certificates.iter().map(|c| c.digest()))
        .unwrap();

    let found_count = found_certificates.iter().filter(|&c| c.is_some()).count();

    assert_eq!(
        found_count, 0,
        "Found {} certificates in the store",
        found_count
    );
}

// Used below to construct malformed Headers
// Note: this should always mimic the Header struct, only changing the visibility of the id field to public
#[allow(dead_code)]
struct BadHeader {
    pub author: AuthorityIdentifier,
    pub round: Round,
    pub epoch: Epoch,
    pub created_at: TimestampMs,
    pub payload: IndexMap<BatchDigest, (WorkerId, TimestampMs)>,
    pub parents: BTreeSet<CertificateDigest>,
    pub id: OnceCell<HeaderDigest>,
}

// TODO: Remove after network has moved to CertificateV2
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn fetch_certificates_v1_basic() {
    let cert_v1_protocol_config = get_protocol_config(28);
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let client = NetworkClient::new_from_keypair(&primary.network_keypair());
    let id = primary.id();
    let fake_primary = fixture.authorities().nth(1).unwrap();
    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let primary_channel_metrics = PrimaryChannelMetrics::new(&Registry::new());
    let gc_depth: Round = 50;

    // kept empty
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    // synchronizer to certificate fetcher
    let (tx_certificate_fetcher, rx_certificate_fetcher) = test_utils::test_channel!(1000);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(1000);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1000);
    // FetchCertificateProxy -> test
    let (tx_fetch_req, mut rx_fetch_req) = mpsc::channel(1000);
    // test -> FetchCertificateProxy
    let (tx_fetch_resp, rx_fetch_resp) = mpsc::channel(1000);

    // Create test stores.
    let store = NodeStorage::reopen(temp_dir(), None);
    let certificate_store = store.certificate_store.clone();
    let payload_store = store.payload_store.clone();

    // Signal rounds
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(0, 0));

    // Make a synchronizer for certificates.
    let synchronizer = Arc::new(Synchronizer::new(
        id,
        fixture.committee(),
        cert_v1_protocol_config.clone(),
        worker_cache.clone(),
        gc_depth,
        client,
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates.clone(),
        tx_parents.clone(),
        rx_consensus_round_updates.clone(),
        metrics.clone(),
        &primary_channel_metrics,
    ));

    let fake_primary_addr = fake_primary.address().to_anemo_address().unwrap();
    let fake_route =
        anemo::Router::new().add_rpc_service(PrimaryToPrimaryServer::new(NetworkProxy {
            request: tx_fetch_req,
            response: Arc::new(Mutex::new(rx_fetch_resp)),
        }));
    let fake_server_network = anemo::Network::bind(fake_primary_addr.clone())
        .server_name("narwhal")
        .private_key(fake_primary.network_keypair().copy().private().0.to_bytes())
        .start(fake_route)
        .unwrap();
    let client_network = test_utils::test_network(primary.network_keypair(), primary.address());
    client_network
        .connect_with_peer_id(fake_primary_addr, fake_server_network.peer_id())
        .await
        .unwrap();

    // Make a certificate fetcher
    let _certificate_fetcher_handle = CertificateFetcher::spawn(
        id,
        fixture.committee(),
        cert_v1_protocol_config.clone(),
        client_network.clone(),
        certificate_store.clone(),
        rx_consensus_round_updates.clone(),
        tx_shutdown.subscribe(),
        rx_certificate_fetcher,
        synchronizer.clone(),
        metrics.clone(),
    );

    // Generate headers and certificates in successive rounds
    let genesis_certs: Vec<_> =
        Certificate::genesis(&cert_v1_protocol_config, &fixture.committee());
    for cert in genesis_certs.iter() {
        certificate_store
            .write(cert.clone())
            .expect("Writing certificate to store failed");
    }

    let mut current_round: Vec<_> = genesis_certs
        .into_iter()
        .map(|cert| cert.header().clone())
        .collect();
    let mut headers = vec![];
    let rounds = 60;
    for i in 0..rounds {
        let parents: BTreeSet<_> = current_round
            .into_iter()
            .map(|header| {
                fixture
                    .certificate(&cert_v1_protocol_config, &header)
                    .digest()
            })
            .collect();
        (_, current_round) = fixture.headers_round(i, &parents, &cert_v1_protocol_config);
        headers.extend(current_round.clone());
    }

    // Avoid any sort of missing payload by pre-populating the batch
    for (digest, (worker_id, _)) in headers.iter().flat_map(|h| h.payload().iter()) {
        payload_store.write(digest, worker_id).unwrap();
    }

    let total_certificates = fixture.authorities().count() * rounds as usize;
    // Create certificates test data.
    let mut certificates = vec![];
    for header in headers.into_iter() {
        certificates.push(fixture.certificate(&cert_v1_protocol_config, &header));
    }
    assert_eq!(certificates.len(), total_certificates); // note genesis is not included
    assert_eq!(240, total_certificates);

    for cert in certificates.iter().take(4) {
        certificate_store
            .write(cert.clone())
            .expect("Writing certificate to store failed");
    }
    let mut num_written = 4;

    // Send a primary message for a certificate with parents that do not exist locally, to trigger fetching.
    let target_index = 123;
    assert!(!synchronizer
        .get_missing_parents(&certificates[target_index].clone())
        .await
        .unwrap()
        .is_empty());

    // Verify the fetch request.
    let mut req = rx_fetch_req.recv().await.unwrap();
    let (lower_bound, skip_rounds) = req.get_bounds();
    assert_eq!(lower_bound, 0);
    assert_eq!(skip_rounds.len(), fixture.authorities().count());
    for rounds in skip_rounds.values() {
        assert_eq!(rounds, &(1..2).collect());
    }

    // Send back another 62 certificates.
    let first_batch_len = 62;
    let first_batch_resp = FetchCertificatesResponse {
        certificates: certificates
            .iter()
            .skip(num_written)
            .take(first_batch_len)
            .cloned()
            .collect_vec(),
    };
    tx_fetch_resp.try_send(first_batch_resp.clone()).unwrap();

    // The certificates up to index 4 + 62 = 66 should be written to store eventually by core.
    verify_certificates_v1_in_store(
        &certificate_store,
        &certificates[0..(num_written + first_batch_len)],
    )
    .await;
    num_written += first_batch_len;

    // The certificate fetcher should send out another fetch request, because it has not received certificate 123.
    loop {
        match rx_fetch_req.recv().await {
            Some(r) => {
                let (_, skip_rounds) = r.get_bounds();
                if skip_rounds.values().next().unwrap().len() == 1 {
                    // Drain the fetch requests sent out before the last reply, when only 1 round in skip_rounds.
                    tx_fetch_resp.try_send(first_batch_resp.clone()).unwrap();
                    continue;
                }
                req = r;
                break;
            }
            None => panic!("Unexpected channel closing!"),
        }
    }
    let (_, skip_rounds) = req.get_bounds();
    assert_eq!(skip_rounds.len(), fixture.authorities().count());
    for (_, rounds) in skip_rounds {
        let rounds = rounds.into_iter().collect_vec();
        assert!(rounds == (1..=16).collect_vec() || rounds == (1..=17).collect_vec());
    }

    // Send back another 123 + 1 - 66 = 58 certificates.
    let second_batch_len = target_index + 1 - num_written;
    let second_batch_resp = FetchCertificatesResponse {
        certificates: certificates
            .iter()
            .skip(num_written)
            .take(second_batch_len)
            .cloned()
            .collect_vec(),
    };
    tx_fetch_resp.try_send(second_batch_resp.clone()).unwrap();

    // The certificates 4 ~ 64 should become available in store eventually.
    verify_certificates_v1_in_store(
        &certificate_store,
        &certificates[0..(num_written + second_batch_len)],
    )
    .await;
    num_written += second_batch_len;

    // No new fetch request is expected.
    sleep(Duration::from_secs(5)).await;
    loop {
        match rx_fetch_req.try_recv() {
            Ok(r) => {
                let (_, skip_rounds) = r.get_bounds();
                let first_num_skip_rounds = skip_rounds.values().next().unwrap().len();
                if first_num_skip_rounds == 16 || first_num_skip_rounds == 17 {
                    // Drain the fetch requests sent out before the last reply.
                    tx_fetch_resp.try_send(second_batch_resp.clone()).unwrap();
                    continue;
                }
                panic!("No more fetch request is expected! {:#?}", r);
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => panic!("Unexpected disconnect!"),
        }
    }

    let target_index = num_written + 8;
    assert!(!synchronizer
        .get_missing_parents(&certificates[target_index].clone())
        .await
        .unwrap()
        .is_empty());

    // Verify the fetch request.
    let req = rx_fetch_req.recv().await.unwrap();
    let (lower_bound, skip_rounds) = req.get_bounds();
    assert_eq!(lower_bound, 0);
    assert_eq!(skip_rounds.len(), fixture.authorities().count());
    for rounds in skip_rounds.values() {
        assert_eq!(rounds, &(1..32).collect());
    }

    // Send out a batch of malformed certificates.
    let mut certs = Vec::new();
    // Add cert missing parent info.
    let mut cert = certificates[num_written].clone();
    cert.header_mut().clear_parents();
    certs.push(cert);
    // Add cert with incorrect digest.
    let mut cert = certificates[num_written].clone();
    // This is a bit tedious to craft
    let cert_header =
        unsafe { std::mem::transmute::<HeaderV1, BadHeader>(cert.header().as_v1().clone()) };
    let wrong_header = BadHeader { ..cert_header };
    let wolf_header = unsafe { std::mem::transmute::<BadHeader, HeaderV1>(wrong_header) };
    cert.update_header(wolf_header.into());
    certs.push(cert);
    // Add cert without all parents in storage.
    certs.push(certificates[num_written + 1].clone());
    tx_fetch_resp
        .try_send(FetchCertificatesResponse {
            certificates: certs,
        })
        .unwrap();

    // Verify no certificate is written to store.
    sleep(Duration::from_secs(1)).await;
    verify_certificates_not_in_store(&certificate_store, &certificates[num_written..]);

    // Send out a batch of certificate V2s.
    let mut cert_v2_config = latest_protocol_version();
    cert_v2_config.set_narwhal_certificate_v2_for_testing(true);
    let mut certs = Vec::new();
    for cert in certificates.iter().skip(num_written).take(8) {
        certs.push(fixture.certificate(&cert_v2_config, cert.header()));
    }
    tx_fetch_resp
        .try_send(FetchCertificatesResponse {
            certificates: certs,
        })
        .unwrap();

    sleep(Duration::from_secs(1)).await;
    verify_certificates_not_in_store(&certificate_store, &certificates[num_written..target_index]);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn fetch_certificates_v2_basic() {
    let cert_v2_config = latest_protocol_version();
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let worker_cache = fixture.worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let client = NetworkClient::new_from_keypair(&primary.network_keypair());
    let id = primary.id();
    let fake_primary = fixture.authorities().nth(1).unwrap();
    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let primary_channel_metrics = PrimaryChannelMetrics::new(&Registry::new());
    let gc_depth: Round = 50;

    // kept empty
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    // synchronizer to certificate fetcher
    let (tx_certificate_fetcher, rx_certificate_fetcher) = test_utils::test_channel!(1000);
    let (tx_new_certificates, _rx_new_certificates) = test_utils::test_channel!(1000);
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1000);
    // FetchCertificateProxy -> test
    let (tx_fetch_req, mut rx_fetch_req) = mpsc::channel(1000);
    // test -> FetchCertificateProxy
    let (tx_fetch_resp, rx_fetch_resp) = mpsc::channel(1000);

    // Create test stores.
    let store = NodeStorage::reopen(temp_dir(), None);
    let certificate_store = store.certificate_store.clone();
    let payload_store = store.payload_store.clone();

    // Signal rounds
    let (_tx_consensus_round_updates, rx_consensus_round_updates) =
        watch::channel(ConsensusRound::new(0, 0));

    // Make a synchronizer for certificates.
    let synchronizer = Arc::new(Synchronizer::new(
        id,
        fixture.committee(),
        cert_v2_config.clone(),
        worker_cache.clone(),
        gc_depth,
        client,
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_fetcher,
        tx_new_certificates.clone(),
        tx_parents.clone(),
        rx_consensus_round_updates.clone(),
        metrics.clone(),
        &primary_channel_metrics,
    ));

    let fake_primary_addr = fake_primary.address().to_anemo_address().unwrap();
    let fake_route =
        anemo::Router::new().add_rpc_service(PrimaryToPrimaryServer::new(NetworkProxy {
            request: tx_fetch_req,
            response: Arc::new(Mutex::new(rx_fetch_resp)),
        }));
    let fake_server_network = anemo::Network::bind(fake_primary_addr.clone())
        .server_name("narwhal")
        .private_key(fake_primary.network_keypair().copy().private().0.to_bytes())
        .start(fake_route)
        .unwrap();
    let client_network = test_utils::test_network(primary.network_keypair(), primary.address());
    client_network
        .connect_with_peer_id(fake_primary_addr, fake_server_network.peer_id())
        .await
        .unwrap();

    // Make a certificate fetcher
    let _certificate_fetcher_handle = CertificateFetcher::spawn(
        id,
        fixture.committee(),
        cert_v2_config.clone(),
        client_network.clone(),
        certificate_store.clone(),
        rx_consensus_round_updates.clone(),
        tx_shutdown.subscribe(),
        rx_certificate_fetcher,
        synchronizer.clone(),
        metrics.clone(),
    );

    // Generate headers and certificates in successive rounds
    let genesis_certs: Vec<_> = Certificate::genesis(&cert_v2_config, &fixture.committee());
    for cert in genesis_certs.iter() {
        certificate_store
            .write(cert.clone())
            .expect("Writing certificate to store failed");
    }

    let mut current_round: Vec<_> = genesis_certs
        .into_iter()
        .map(|cert| cert.header().clone())
        .collect();
    let mut headers = vec![];
    let rounds = 100;
    for i in 0..rounds {
        let parents: BTreeSet<_> = current_round
            .into_iter()
            .map(|header| fixture.certificate(&cert_v2_config, &header).digest())
            .collect();
        (_, current_round) = fixture.headers_round(i, &parents, &cert_v2_config);
        headers.extend(current_round.clone());
    }

    // Avoid any sort of missing payload by pre-populating the batch
    for (digest, (worker_id, _)) in headers.iter().flat_map(|h| h.payload().iter()) {
        payload_store.write(digest, worker_id).unwrap();
    }

    let total_certificates = fixture.authorities().count() * rounds as usize;
    // Create certificates test data.
    let mut certificates = vec![];
    for header in headers.into_iter() {
        certificates.push(fixture.certificate(&cert_v2_config, &header));
    }
    assert_eq!(certificates.len(), total_certificates); // note genesis is not included
    assert_eq!(400, total_certificates);

    for cert in certificates.iter_mut().take(4) {
        // Manually writing the certificates to store so we can consider them verified
        // directly
        cert.set_signature_verification_state(SignatureVerificationState::VerifiedDirectly(
            cert.aggregated_signature()
                .expect("Invalid Signature")
                .clone(),
        ));
        certificate_store
            .write(cert.clone())
            .expect("Writing certificate to store failed");
    }
    let mut num_written = 4;

    // Send a primary message for a certificate with parents that do not exist locally, to trigger fetching.
    let target_index = 123;
    assert!(!synchronizer
        .get_missing_parents(&certificates[target_index].clone())
        .await
        .unwrap()
        .is_empty());

    // Verify the fetch request.
    let mut req = rx_fetch_req.recv().await.unwrap();
    let (lower_bound, skip_rounds) = req.get_bounds();
    assert_eq!(lower_bound, 0);
    assert_eq!(skip_rounds.len(), fixture.authorities().count());
    for rounds in skip_rounds.values() {
        assert_eq!(rounds, &(1..2).collect());
    }

    // Send back another 62 certificates.
    let first_batch_len = 62;
    let first_batch_resp = FetchCertificatesResponse {
        certificates: certificates
            .iter()
            .skip(num_written)
            .take(first_batch_len)
            .cloned()
            .collect_vec(),
    };
    tx_fetch_resp.try_send(first_batch_resp.clone()).unwrap();

    // The certificates up to index 66 (4 + 62) should be written to store eventually by core.
    verify_certificates_v2_in_store(
        &certificate_store,
        &certificates[0..(num_written + first_batch_len)],
        6,  // 2 fetched certs verified directly + the initial 4 inserted
        60, // verified indirectly
    )
    .await;
    num_written += first_batch_len;

    // The certificate fetcher should send out another fetch request, because it has not received certificate 123.
    loop {
        match rx_fetch_req.recv().await {
            Some(r) => {
                let (_, skip_rounds) = r.get_bounds();
                if skip_rounds.values().next().unwrap().len() == 1 {
                    // Drain the fetch requests sent out before the last reply, when only 1 round in skip_rounds.
                    tx_fetch_resp.try_send(first_batch_resp.clone()).unwrap();
                    continue;
                }
                req = r;
                break;
            }
            None => panic!("Unexpected channel closing!"),
        }
    }
    let (_, skip_rounds) = req.get_bounds();
    assert_eq!(skip_rounds.len(), fixture.authorities().count());
    for (_, rounds) in skip_rounds {
        let rounds = rounds.into_iter().collect_vec();
        assert!(rounds == (1..=16).collect_vec() || rounds == (1..=17).collect_vec());
    }

    // Send back another 123 + 1 - 66 = 58 certificates.
    let second_batch_len = target_index + 1 - num_written;
    let second_batch_resp = FetchCertificatesResponse {
        certificates: certificates
            .iter()
            .skip(num_written)
            .take(second_batch_len)
            .cloned()
            .collect_vec(),
    };
    tx_fetch_resp.try_send(second_batch_resp.clone()).unwrap();

    // The certificates up to index 124 (4 + 62 + 58) should become available in store eventually.
    verify_certificates_v2_in_store(
        &certificate_store,
        &certificates[0..(num_written + second_batch_len)],
        10,  // 6 fetched certs verified directly + the initial 4 inserted
        114, // verified indirectly
    )
    .await;
    num_written += second_batch_len;

    // No new fetch request is expected.
    sleep(Duration::from_secs(5)).await;
    loop {
        match rx_fetch_req.try_recv() {
            Ok(r) => {
                let (_, skip_rounds) = r.get_bounds();
                let first_num_skip_rounds = skip_rounds.values().next().unwrap().len();
                if first_num_skip_rounds == 16 || first_num_skip_rounds == 17 {
                    // Drain the fetch requests sent out before the last reply.
                    tx_fetch_resp.try_send(second_batch_resp.clone()).unwrap();
                    continue;
                }
                panic!("No more fetch request is expected! {:#?}", r);
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => panic!("Unexpected disconnect!"),
        }
    }

    let target_index = num_written + 204;
    assert!(!synchronizer
        .get_missing_parents(&certificates[target_index].clone())
        .await
        .unwrap()
        .is_empty());

    // Verify the fetch request.
    let req = rx_fetch_req.recv().await.unwrap();
    let (lower_bound, skip_rounds) = req.get_bounds();
    assert_eq!(lower_bound, 0);
    assert_eq!(skip_rounds.len(), fixture.authorities().count());
    for rounds in skip_rounds.values() {
        assert_eq!(rounds, &(1..32).collect());
    }

    // Send out a batch of malformed certificates.
    let mut certs = Vec::new();
    // Add cert missing parent info.
    let mut cert = certificates[num_written].clone();
    cert.header_mut().clear_parents();
    certs.push(cert);
    // Add cert with incorrect digest.
    let mut cert = certificates[num_written].clone();
    // This is a bit tedious to craft
    let cert_header =
        unsafe { std::mem::transmute::<HeaderV1, BadHeader>(cert.header().as_v1().clone()) };
    let wrong_header = BadHeader { ..cert_header };
    let wolf_header = unsafe { std::mem::transmute::<BadHeader, HeaderV1>(wrong_header) };
    cert.update_header(Header::from(wolf_header));
    certs.push(cert);
    // Add cert without all parents in storage.
    certs.push(certificates[num_written + 1].clone());
    tx_fetch_resp
        .try_send(FetchCertificatesResponse {
            certificates: certs,
        })
        .unwrap();

    // Verify no certificate is written to store.
    sleep(Duration::from_secs(1)).await;
    verify_certificates_not_in_store(&certificate_store, &certificates[num_written..target_index]);

    // Send out a batch of certificates with bad signatures for all certificates.
    let mut certs = Vec::new();
    for cert in certificates.iter().skip(num_written).take(204) {
        let mut cert = cert.clone();
        cert.set_signature_verification_state(SignatureVerificationState::Unverified(
            AggregateSignatureBytes::default(),
        ));
        certs.push(cert);
    }
    tx_fetch_resp
        .try_send(FetchCertificatesResponse {
            certificates: certs,
        })
        .unwrap();

    sleep(Duration::from_secs(1)).await;
    verify_certificates_not_in_store(&certificate_store, &certificates[num_written..target_index]);

    // Send out a batch of certificate V1s.
    let mut certs = Vec::new();
    for cert in certificates.iter().skip(num_written).take(204) {
        certs.push(fixture.certificate(&get_protocol_config(28), cert.header()));
    }
    tx_fetch_resp
        .try_send(FetchCertificatesResponse {
            certificates: certs,
        })
        .unwrap();

    sleep(Duration::from_secs(1)).await;
    verify_certificates_not_in_store(&certificate_store, &certificates[num_written..target_index]);

    // Send out a batch of certificates with good signatures.
    // The certificates 4 + 62 + 58 + 204 = 328 should become available in store eventually.let mut certs = Vec::new();
    let mut certs = Vec::new();
    for cert in certificates.iter().skip(num_written).take(204) {
        certs.push(cert.clone());
    }
    tx_fetch_resp
        .try_send(FetchCertificatesResponse {
            certificates: certs,
        })
        .unwrap();

    verify_certificates_v2_in_store(
        &certificate_store,
        &certificates[0..(target_index)],
        18,  // 14 fetched certs verified directly + the initial 4 inserted
        310, // to be verified indirectly
    )
    .await;

    // Additional testcases cannot be added, because it seems impossible now to receive from
    // the tx_fetch_resp channel after a certain number of messages.
    // TODO: find the root cause of this issue.
}
