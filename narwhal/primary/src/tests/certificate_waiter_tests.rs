// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    certificate_waiter::CertificateWaiter, core::Core, metrics::PrimaryMetrics,
    synchronizer::Synchronizer,
};
use anemo::async_trait;
use anyhow::Result;
use config::{Epoch, WorkerId};
use crypto::{PublicKey, Signature};
use fastcrypto::{hash::Hash, traits::KeyPair, SignatureService};
use indexmap::IndexMap;
use itertools::Itertools;
use network::P2pNetwork;
use once_cell::sync::OnceCell;
use prometheus::Registry;
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use storage::CertificateStore;
use storage::NodeStorage;

use test_utils::{temp_dir, CommitteeFixture};
use tokio::{
    sync::{
        mpsc::{self, error::TryRecvError, Receiver, Sender},
        watch, Mutex,
    },
    time::sleep,
};
use types::{
    BatchDigest, Certificate, CertificateDigest, FetchCertificatesRequest,
    FetchCertificatesResponse, GetCertificatesRequest, GetCertificatesResponse, Header,
    HeaderDigest, Metadata, PayloadAvailabilityRequest, PayloadAvailabilityResponse,
    PrimaryMessage, PrimaryToPrimary, PrimaryToPrimaryServer, ReconfigureNotification,
    RequestVoteRequest, RequestVoteResponse, Round,
};

pub struct NetworkProxy {
    request: Sender<FetchCertificatesRequest>,
    response: Arc<Mutex<Receiver<FetchCertificatesResponse>>>,
}

#[async_trait]
impl PrimaryToPrimary for NetworkProxy {
    async fn send_message(
        &self,
        request: anemo::Request<PrimaryMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        unimplemented!(
            "FetchCertificateProxy::send_message() is unimplemented!! {:#?}",
            request
        );
    }
    async fn request_vote(
        &self,
        _request: anemo::Request<RequestVoteRequest>,
    ) -> Result<anemo::Response<RequestVoteResponse>, anemo::rpc::Status> {
        unimplemented!()
    }
    async fn get_certificates(
        &self,
        _request: anemo::Request<GetCertificatesRequest>,
    ) -> Result<anemo::Response<GetCertificatesResponse>, anemo::rpc::Status> {
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

    async fn get_payload_availability(
        &self,
        _request: anemo::Request<PayloadAvailabilityRequest>,
    ) -> Result<anemo::Response<PayloadAvailabilityResponse>, anemo::rpc::Status> {
        unimplemented!()
    }
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

// Used below to construct malformed Headers
// Note: this should always mimic the Header struct, only changing the visibility of the id field to public
#[allow(dead_code)]
struct BadHeader {
    pub author: PublicKey,
    pub round: Round,
    pub epoch: Epoch,
    pub payload: IndexMap<BatchDigest, WorkerId>,
    pub parents: BTreeSet<CertificateDigest>,
    pub id: OnceCell<HeaderDigest>,
    pub signature: Signature,
    pub metadata: Metadata,
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn fetch_certificates_basic() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());
    let fake_primary = fixture.authorities().nth(1).unwrap();

    // kept empty
    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(fixture.committee()));
    // synchronizer to certificate waiter
    let (tx_certificate_waiter, rx_certificate_waiter) = test_utils::test_channel!(1000);
    // certificates
    let (tx_certificates, rx_certificates) = test_utils::test_channel!(1000);
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

    // Signal rounds
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);
    let (_tx_narwhal_round_updates, rx_narwhal_round_updates) = watch::channel(0u64);

    // Make a synchronizer for the core.
    let synchronizer = Arc::new(Synchronizer::new(
        name.clone(),
        fixture.committee().into(),
        worker_cache.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_waiter,
        rx_consensus_round_updates.clone(),
        None,
    ));

    let fake_primary_addr = network::multiaddr_to_address(fake_primary.address()).unwrap();
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

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let gc_depth: Round = 50;

    // Make a certificate waiter
    let _certificate_waiter_handle = CertificateWaiter::spawn(
        name.clone(),
        fixture.committee(),
        P2pNetwork::new(client_network.clone()),
        certificate_store.clone(),
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
        fixture.committee(),
        worker_cache,
        store.header_store.clone(),
        certificate_store.clone(),
        synchronizer.clone(),
        signature_service,
        rx_consensus_round_updates,
        rx_narwhal_round_updates,
        gc_depth,
        rx_reconfigure,
        rx_certificates,
        rx_certificates_loopback,
        rx_headers,
        tx_consensus,
        tx_parents,
        metrics.clone(),
        P2pNetwork::new(client_network),
    );

    // Generate headers and certificates in successive rounds
    let genesis_certs: Vec<_> = Certificate::genesis(&fixture.committee());
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
        payload_store.async_write((*digest, *worker_id), 0u8).await;
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

    // Send a primary message for a certificate with parents that do not exist locally, to trigger fetching.
    let target_index = 123;
    tx_certificates
        .send((certificates[target_index].clone(), None))
        .await
        .unwrap();

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
    verify_certificates_in_store(
        &certificate_store,
        &certificates[0..(num_written + first_batch_len)],
    )
    .await;
    num_written += first_batch_len;

    // The certificate waiter should send out another fetch request, because it has not received certificate 123.
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
    verify_certificates_in_store(
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

    // Send out a batch of malformed certificates.
    let mut certs = Vec::new();
    // Add cert missing parent info.
    let mut cert = certificates[num_written].clone();
    cert.header.parents.clear();
    certs.push(cert);
    // Add cert with incorrect digest.
    let mut cert = certificates[num_written].clone();
    // This is a bit tedious to craft
    let cert_header = unsafe { std::mem::transmute::<Header, BadHeader>(cert.header) };
    let wrong_header = BadHeader {
        id: OnceCell::with_value(HeaderDigest::default()),
        ..cert_header
    };
    let wolf_header = unsafe { std::mem::transmute::<BadHeader, Header>(wrong_header) };
    cert.header = wolf_header;
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
