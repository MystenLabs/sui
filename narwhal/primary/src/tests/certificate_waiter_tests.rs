// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    certificate_waiter::CertificateWaiter, common::create_test_vote_store, core::Core,
    header_waiter::HeaderWaiter, metrics::PrimaryMetrics, synchronizer::Synchronizer,
};
use anemo::async_trait;
use anyhow::Result;
use config::Committee;
use crypto::PublicKey;
use fastcrypto::{hash::Hash, traits::KeyPair, SignatureService};
use itertools::Itertools;
use network::P2pNetwork;
use node::NodeStorage;
use prometheus::Registry;
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
    time::Duration,
};
use storage::CertificateStore;
use test_utils::{temp_dir, CommitteeFixture};
use tokio::{
    sync::{
        mpsc::{self, error::TryRecvError, Receiver, Sender},
        watch, Mutex,
    },
    time::sleep,
};
use types::{
    Certificate, CertificateDigest, ConsensusStore, FetchCertificatesRequest,
    FetchCertificatesResponse, PayloadAvailabilityRequest, PayloadAvailabilityResponse,
    PrimaryMessage, PrimaryToPrimary, PrimaryToPrimaryServer, ReconfigureNotification, Round,
};

struct FetchCertificateProxy {
    request: Sender<FetchCertificatesRequest>,
    response: Arc<Mutex<Receiver<FetchCertificatesResponse>>>,
}

#[async_trait]
impl PrimaryToPrimary for FetchCertificateProxy {
    async fn send_message(
        &self,
        request: anemo::Request<PrimaryMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        unimplemented!(
            "FetchCertificateProxy::send_message() is unimplemented!! {:#?}",
            request
        );
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
    async fn get_payload_availability(
        &self,
        _request: anemo::Request<PayloadAvailabilityRequest>,
    ) -> Result<anemo::Response<PayloadAvailabilityResponse>, anemo::rpc::Status> {
        unimplemented!()
    }
}

// Simulate consensus committing all certificates written to store, by updating last committed
// rounds to the last written rounds.
#[allow(clippy::mutable_key_type)]
fn write_last_committed(
    committee: &Committee,
    certificate_store: &CertificateStore,
    consensus_sotre: &Arc<ConsensusStore>,
) {
    let committed_rounds: HashMap<PublicKey, Round> = committee
        .authorities()
        .map(|(name, _)| {
            (
                name.clone(),
                certificate_store
                    .last_round_number(name)
                    .unwrap()
                    .unwrap_or(0),
            )
        })
        .collect();
    consensus_sotre
        .write_consensus_state(&committed_rounds, &0, &CertificateDigest::default())
        .expect("Write to consensus store failed!");
}

async fn verify_certificates_in_store(
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
    assert!(certificate_store
        .read_all(certificates.iter().map(|c| c.digest()))
        .unwrap()
        .into_iter()
        .map_while(|c| c)
        .next()
        .is_none());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn fetch_certificates_basic() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());
    let fake_primary = fixture.authorities().nth(1).unwrap();

    // kept empty
    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    // synchronizer to header waiter
    let (tx_header_waiter, rx_header_waiter) = test_utils::test_channel!(1000);
    // synchronizer to certificate waiter
    let (tx_certificate_waiter, rx_certificate_waiter) = test_utils::test_channel!(1000);
    // primary messages
    let (tx_primary_messages, rx_primary_messages) = test_utils::test_channel!(1000);
    // header waiter to primary
    let (tx_headers_loopback, rx_headers_loopback) = test_utils::test_channel!(1000);
    // certificate waiter to primary
    let (tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1000);
    // proposer back to the core
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1000);
    // core -> consensus, we store the output of process_certificate here, a small channel limit may backpressure the test into failure
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(1000);
    // core -> proposers, byproduct of certificate processing, a small channel limit could backpressure the test into failure
    let (tx_parents, _rx_parents) = test_utils::test_channel!(1000);
    // FetchCertificateProxy -> test
    let (tx_fetch_req, mut rx_fetch_req) = mpsc::channel(1000);
    // test -> FetchCertificateProxy
    let (tx_fetch_resp, rx_fetch_resp) = mpsc::channel(1000);

    // Create test stores.
    let store = NodeStorage::reopen(temp_dir());
    let certificate_store = store.certificate_store.clone();
    let payload_store = store.payload_store.clone();
    let consensus_store = store.consensus_store.clone();

    // Signal consensus round
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificate_store.clone(),
        payload_store.clone(),
        tx_header_waiter,
        tx_certificate_waiter,
        None,
    );

    let fake_primary_addr = network::multiaddr_to_address(fake_primary.address()).unwrap();
    let fake_route =
        anemo::Router::new().add_rpc_service(PrimaryToPrimaryServer::new(FetchCertificateProxy {
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

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let gc_depth: Round = 50;

    // Make a headerWaiter
    let _header_waiter_handle = HeaderWaiter::spawn(
        name.clone(),
        committee.clone(),
        worker_cache.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        rx_consensus_round_updates.clone(),
        gc_depth,
        rx_reconfigure.clone(),
        rx_header_waiter,
        tx_headers_loopback,
        metrics.clone(),
        P2pNetwork::new(client_network.clone()),
    );

    // Make a certificate waiter
    let _certificate_waiter_handle = CertificateWaiter::spawn(
        name.clone(),
        committee.clone(),
        P2pNetwork::new(client_network.clone()),
        certificate_store.clone(),
        Some(consensus_store.clone()),
        rx_consensus_round_updates.clone(),
        gc_depth,
        rx_reconfigure.clone(),
        rx_certificate_waiter,
        tx_certificates_loopback,
        metrics.clone(),
    );

    // Spawn the core.
    let _core_handle = Core::spawn(
        name.clone(),
        committee.clone(),
        worker_cache,
        store.header_store.clone(),
        certificate_store.clone(),
        create_test_vote_store(),
        synchronizer,
        signature_service,
        rx_consensus_round_updates,
        gc_depth,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        rx_headers_loopback,
        rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
        P2pNetwork::new(client_network),
    );

    // Generate headers and certificates in successive rounds
    let genesis_certs: Vec<_> = Certificate::genesis(&committee);
    for cert in genesis_certs.iter() {
        certificate_store
            .write(cert.clone())
            .expect("Writing certificate to store failed");
    }

    let mut current_round: Vec<_> = genesis_certs.into_iter().map(|cert| cert.header).collect();
    let mut headers = vec![];
    let rounds = 60;
    for i in 0..rounds {
        let parents: BTreeSet<_> = current_round
            .into_iter()
            .map(|header| fixture.certificate(&header).digest())
            .collect();
        (_, current_round) = fixture.headers_round(i, &parents);
        headers.extend(current_round.clone());
    }

    // Avoid any sort of missing payload by pre-populating the batch
    for (digest, worker_id) in headers.iter().flat_map(|h| h.payload.iter()) {
        payload_store.write((*digest, *worker_id), 0u8).await;
    }

    let total_certificates = fixture.authorities().count() * rounds as usize;
    // Create certificates test data.
    let mut certificates = vec![];
    for header in headers.into_iter() {
        certificates.push(fixture.certificate(&header));
    }
    assert_eq!(certificates.len(), total_certificates); // note genesis is not included
    assert_eq!(240, total_certificates);

    for cert in certificates.iter().take(4) {
        certificate_store
            .write(cert.clone())
            .expect("Writing certificate to store failed");
    }
    let mut num_written = 4;
    write_last_committed(&committee, &certificate_store, &consensus_store);

    // Send a primary message for a certificate with parents that do not exist locally, to trigger fetching.
    let target_index = 123;
    tx_primary_messages
        .send(PrimaryMessage::Certificate(
            certificates[target_index].clone(),
        ))
        .await
        .unwrap();

    // Verify the fetch request.
    let mut req = rx_fetch_req.recv().await.unwrap();
    assert_eq!(
        req.exclusive_lower_bounds.len(),
        fixture.authorities().count()
    );
    for (_, round) in &req.exclusive_lower_bounds {
        assert_eq!(round, &1);
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
    verify_certificates_in_store(
        &certificate_store,
        &certificates[0..(num_written + first_batch_len)],
    )
    .await;
    num_written += first_batch_len;
    write_last_committed(&committee, &certificate_store, &consensus_store);

    // The certificate waiter should send out another fetch request, because it has not received certificate 123.
    loop {
        match rx_fetch_req.recv().await {
            Some(r) => {
                if r.exclusive_lower_bounds[0].1 == 1 {
                    // Drain the fetch requests sent out before the last reply.
                    tx_fetch_resp.try_send(first_batch_resp.clone()).unwrap();
                    continue;
                }
                req = r;
                break;
            }
            None => panic!("Unexpected channel closing!"),
        }
    }
    assert_eq!(
        req.exclusive_lower_bounds.len(),
        fixture.authorities().count()
    );
    let mut rounds = req
        .exclusive_lower_bounds
        .iter()
        .map(|(_, round)| round)
        .cloned()
        .collect_vec();
    rounds.sort();
    // Expected rounds are calculated from current num_written index.
    assert_eq!(rounds, vec![16, 16, 17, 17]);

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
    verify_certificates_in_store(
        &certificate_store,
        &certificates[0..(num_written + second_batch_len)],
    )
    .await;
    num_written += second_batch_len;
    write_last_committed(&committee, &certificate_store, &consensus_store);

    // No new fetch request is expected.
    sleep(Duration::from_secs(5)).await;
    loop {
        match rx_fetch_req.try_recv() {
            Ok(r) => {
                if r.exclusive_lower_bounds[0].1 == 16 || r.exclusive_lower_bounds[0].1 == 17 {
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

    // Send out a batch of malformed certificates.
    let mut certs = Vec::new();
    // Add cert missing parent info.
    let mut cert = certificates[num_written].clone();
    cert.header.parents.clear();
    certs.push(cert);
    // Add cert with incorrect digest.
    let mut cert = certificates[num_written].clone();
    cert.header.id = Default::default();
    certs.push(cert);
    // Add cert without all parents in storage.
    certs.push(certificates[num_written + 1].clone());
    tx_fetch_resp
        .try_send(FetchCertificatesResponse {
            certificates: certs,
        })
        .unwrap();

    // Verify no certificate is written to store.
    sleep(Duration::from_secs(5)).await;
    verify_certificates_not_in_store(&certificate_store, &certificates[num_written..]);
}
