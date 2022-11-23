// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::async_trait;
use config::{
    utils::get_available_port, Authority, Committee, Epoch, SharedWorkerCache, Stake, WorkerCache,
    WorkerId, WorkerIndex, WorkerInfo,
};
use crypto::{KeyPair, NetworkKeyPair, NetworkPublicKey, PublicKey};
use fastcrypto::{
    hash::{Digest, Hash as _},
    traits::{AllowedRng, KeyPair as _, Signer as _},
};
use indexmap::IndexMap;
use multiaddr::Multiaddr;
use rand::{
    rngs::{OsRng, StdRng},
    thread_rng, Rng, SeedableRng,
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    num::NonZeroUsize,
    ops::RangeInclusive,
    sync::Arc,
};
use store::{reopen, rocks, rocks::DBMap, Store};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tracing::info;
use types::{
    Batch, BatchDigest, Certificate, CertificateDigest, CommittedSubDagShell, ConsensusStore,
    FetchCertificatesRequest, FetchCertificatesResponse, GetCertificatesRequest,
    GetCertificatesResponse, Header, HeaderBuilder, PayloadAvailabilityRequest,
    PayloadAvailabilityResponse, PrimaryMessage, PrimaryToPrimary, PrimaryToPrimaryServer,
    PrimaryToWorker, PrimaryToWorkerServer, RequestBatchRequest, RequestBatchResponse,
    RequestVoteRequest, RequestVoteResponse, Round, SequenceNumber, Transaction, Vote,
    WorkerBatchMessage, WorkerDeleteBatchesMessage, WorkerReconfigureMessage,
    WorkerSynchronizeMessage, WorkerToWorker, WorkerToWorkerServer,
};

pub mod cluster;

pub const VOTES_CF: &str = "votes";
pub const HEADERS_CF: &str = "headers";
pub const CERTIFICATES_CF: &str = "certificates";
pub const CERTIFICATE_DIGEST_BY_ROUND_CF: &str = "certificate_digest_by_round";
pub const CERTIFICATE_DIGEST_BY_ORIGIN_CF: &str = "certificate_digest_by_origin";
pub const PAYLOAD_CF: &str = "payload";

pub fn temp_dir() -> std::path::PathBuf {
    tempfile::tempdir()
        .expect("Failed to open temporary directory")
        .into_path()
}

pub fn ensure_test_environment() {
    // One common issue when running tests on Mac is that the default ulimit is too low,
    // leading to I/O errors such as "Too many open files". Raising fdlimit to bypass it.
    // Also we can't do this in Windows, apparently.
    #[cfg(not(target_os = "windows"))]
    fdlimit::raise_fd_limit().expect("Could not raise ulimit");
}

#[macro_export]
macro_rules! test_channel {
    ($e:expr) => {
        types::metered_channel::channel(
            $e,
            &prometheus::IntGauge::new("TEST_COUNTER", "test counter").unwrap(),
        );
    };
}

// Note: use the following macros to initialize your Primary / Consensus channels
// if your test is spawning a primary and you encounter an `AllReg` error.
//
// Rationale:
// The primary initialization will try to edit a specific metric in its registry
// for its new_certificates and committeed_certificates channel. The gauge situated
// in the channel you're passing as an argument to the primary initialization is
// the replacement. If that gauge is a dummy gauge, such as the one above, the
// initialization of the primary will panic (to protect the production code against
// an erroneous mistake in editing this bootstrap logic).
#[macro_export]
macro_rules! test_committed_certificates_channel {
    ($e:expr) => {
        types::metered_channel::channel(
            $e,
            &prometheus::IntGauge::new(
                primary::PrimaryChannelMetrics::NAME_COMMITTED_CERTS,
                primary::PrimaryChannelMetrics::DESC_COMMITTED_CERTS,
            )
            .unwrap(),
        );
    };
}

#[macro_export]
macro_rules! test_new_certificates_channel {
    ($e:expr) => {
        types::metered_channel::channel(
            $e,
            &prometheus::IntGauge::new(
                primary::PrimaryChannelMetrics::NAME_NEW_CERTS,
                primary::PrimaryChannelMetrics::DESC_NEW_CERTS,
            )
            .unwrap(),
        );
    };
}

////////////////////////////////////////////////////////////////
/// Keys, Committee
////////////////////////////////////////////////////////////////

pub fn random_key() -> KeyPair {
    KeyPair::generate(&mut thread_rng())
}

////////////////////////////////////////////////////////////////
/// Headers, Votes, Certificates
////////////////////////////////////////////////////////////////

pub fn make_consensus_store(store_path: &std::path::Path) -> Arc<ConsensusStore> {
    const LAST_COMMITTED_CF: &str = "last_committed";
    const SEQUENCE_CF: &str = "sequence";
    const SUB_DAG_CF: &str = "sub_dag";

    let rocksdb = rocks::open_cf(
        store_path,
        None,
        &[LAST_COMMITTED_CF, SEQUENCE_CF, SUB_DAG_CF],
    )
    .expect("Failed creating database");

    let (last_committed_map, sequence_map, sub_dag_map) = reopen!(&rocksdb,
        LAST_COMMITTED_CF;<PublicKey, Round>,
        SEQUENCE_CF;<SequenceNumber, CertificateDigest>,
        SUB_DAG_CF;<Round, CommittedSubDagShell>
    );

    Arc::new(ConsensusStore::new(
        last_committed_map,
        sequence_map,
        sub_dag_map,
    ))
}

pub fn fixture_payload(number_of_batches: u8) -> IndexMap<BatchDigest, WorkerId> {
    let mut payload: IndexMap<BatchDigest, WorkerId> = IndexMap::new();

    for _ in 0..number_of_batches {
        let batch_digest = batch().digest();

        payload.insert(batch_digest, 0);
    }

    payload
}

// will create a batch with randomly formed transactions
// dictated by the parameter number_of_transactions
pub fn fixture_batch_with_transactions(number_of_transactions: u32) -> Batch {
    let transactions = (0..number_of_transactions)
        .map(|_v| transaction())
        .collect();

    Batch::new(transactions)
}

// Fixture
pub fn transaction() -> Transaction {
    // generate random value transactions, but the length will be always 100 bytes
    (0..100).map(|_v| rand::random::<u8>()).collect()
}

#[derive(Clone)]
pub struct PrimaryToPrimaryMockServer {
    sender: Sender<PrimaryMessage>,
}

impl PrimaryToPrimaryMockServer {
    pub fn spawn(
        network_keypair: NetworkKeyPair,
        address: Multiaddr,
    ) -> (Receiver<PrimaryMessage>, anemo::Network) {
        let addr = network::multiaddr_to_address(&address).unwrap();
        let (sender, receiver) = channel(1);
        let service = PrimaryToPrimaryServer::new(Self { sender });

        let routes = anemo::Router::new().add_rpc_service(service);
        let network = anemo::Network::bind(addr)
            .server_name("narwhal")
            .private_key(network_keypair.private().0.to_bytes())
            .start(routes)
            .unwrap();
        info!("starting network on: {}", network.local_addr());
        (receiver, network)
    }
}

#[async_trait]
impl PrimaryToPrimary for PrimaryToPrimaryMockServer {
    async fn send_message(
        &self,
        request: anemo::Request<PrimaryMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let message = request.into_body();

        self.sender.send(message).await.unwrap();

        Ok(anemo::Response::new(()))
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
        _request: anemo::Request<FetchCertificatesRequest>,
    ) -> Result<anemo::Response<FetchCertificatesResponse>, anemo::rpc::Status> {
        unimplemented!()
    }

    async fn get_payload_availability(
        &self,
        _request: anemo::Request<PayloadAvailabilityRequest>,
    ) -> Result<anemo::Response<PayloadAvailabilityResponse>, anemo::rpc::Status> {
        unimplemented!()
    }
}

pub struct PrimaryToWorkerMockServer {
    // TODO: refactor tests to use mockall for this.
    msg_sender: Sender<WorkerReconfigureMessage>,
    synchronize_sender: Sender<WorkerSynchronizeMessage>,
}

impl PrimaryToWorkerMockServer {
    pub fn spawn(
        keypair: NetworkKeyPair,
        address: Multiaddr,
    ) -> (
        Receiver<WorkerReconfigureMessage>,
        Receiver<WorkerSynchronizeMessage>,
        anemo::Network,
    ) {
        let addr = network::multiaddr_to_address(&address).unwrap();
        let (msg_sender, msg_receiver) = channel(1);
        let (synchronize_sender, synchronize_receiver) = channel(1);
        let service = PrimaryToWorkerServer::new(Self {
            msg_sender,
            synchronize_sender,
        });

        let routes = anemo::Router::new().add_rpc_service(service);
        let network = anemo::Network::bind(addr)
            .server_name("narwhal")
            .private_key(keypair.private().0.to_bytes())
            .start(routes)
            .unwrap();
        info!("starting network on: {}", network.local_addr());
        (msg_receiver, synchronize_receiver, network)
    }
}

#[async_trait]
impl PrimaryToWorker for PrimaryToWorkerMockServer {
    async fn reconfigure(
        &self,
        request: anemo::Request<WorkerReconfigureMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let message = request.into_body();
        self.msg_sender.send(message).await.unwrap();
        Ok(anemo::Response::new(()))
    }
    async fn synchronize(
        &self,
        request: anemo::Request<WorkerSynchronizeMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let message = request.into_body();
        self.synchronize_sender.send(message).await.unwrap();
        Ok(anemo::Response::new(()))
    }

    async fn delete_batches(
        &self,
        _request: anemo::Request<WorkerDeleteBatchesMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        tracing::error!("Not implemented PrimaryToWorkerMockServer::delete_batches");
        Err(anemo::rpc::Status::internal("Unimplemented"))
    }
}

pub struct WorkerToWorkerMockServer {
    batch_sender: Sender<WorkerBatchMessage>,
}

impl WorkerToWorkerMockServer {
    pub fn spawn(
        keypair: NetworkKeyPair,
        address: Multiaddr,
    ) -> (Receiver<WorkerBatchMessage>, anemo::Network) {
        let addr = network::multiaddr_to_address(&address).unwrap();
        let (batch_sender, batch_receiver) = channel(1);
        let service = WorkerToWorkerServer::new(Self { batch_sender });

        let routes = anemo::Router::new().add_rpc_service(service);
        let network = anemo::Network::bind(addr)
            .server_name("narwhal")
            .private_key(keypair.private().0.to_bytes())
            .start(routes)
            .unwrap();
        info!("starting network on: {}", network.local_addr());
        (batch_receiver, network)
    }
}

#[async_trait]
impl WorkerToWorker for WorkerToWorkerMockServer {
    async fn report_batch(
        &self,
        request: anemo::Request<WorkerBatchMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let message = request.into_body();

        self.batch_sender.send(message).await.unwrap();

        Ok(anemo::Response::new(()))
    }
    async fn request_batch(
        &self,
        _request: anemo::Request<RequestBatchRequest>,
    ) -> Result<anemo::Response<RequestBatchResponse>, anemo::rpc::Status> {
        tracing::error!("Not implemented WorkerToWorkerMockServer::request_batch");
        Err(anemo::rpc::Status::internal("Unimplemented"))
    }
}

////////////////////////////////////////////////////////////////
/// Batches
////////////////////////////////////////////////////////////////

// Fixture
pub fn batch() -> Batch {
    Batch::new(vec![transaction(), transaction()])
}

/// generate multiple fixture batches. The number of generated batches
/// are dictated by the parameter num_of_batches.
pub fn batches(num_of_batches: usize) -> Vec<Batch> {
    let mut batches = Vec::new();

    for i in 1..num_of_batches + 1 {
        batches.push(batch_with_transactions(i));
    }

    batches
}

pub fn batch_with_transactions(num_of_transactions: usize) -> Batch {
    let mut transactions = Vec::new();

    for _ in 0..num_of_transactions {
        transactions.push(transaction());
    }

    Batch::new(transactions)
}

const BATCHES_CF: &str = "batches";

pub fn open_batch_store() -> Store<BatchDigest, Batch> {
    let db = DBMap::<BatchDigest, Batch>::open(temp_dir(), None, Some(BATCHES_CF)).unwrap();
    Store::new(db)
}

// Creates one certificate per authority starting and finishing at the specified rounds (inclusive).
// Outputs a VecDeque of certificates (the certificate with higher round is on the front) and a set
// of digests to be used as parents for the certificates of the next round.
// Note : the certificates are unsigned
pub fn make_optimal_certificates(
    committee: &Committee,
    range: RangeInclusive<Round>,
    initial_parents: &BTreeSet<CertificateDigest>,
    keys: &[PublicKey],
) -> (VecDeque<Certificate>, BTreeSet<CertificateDigest>) {
    make_certificates(committee, range, initial_parents, keys, 0.0)
}

// Outputs rounds worth of certificates with optimal parents, signed
pub fn make_optimal_signed_certificates(
    range: RangeInclusive<Round>,
    initial_parents: &BTreeSet<CertificateDigest>,
    committee: &Committee,
    keys: &[KeyPair],
) -> (VecDeque<Certificate>, BTreeSet<CertificateDigest>) {
    make_signed_certificates(range, initial_parents, committee, keys, 0.0)
}

// Bernoulli-samples from a set of ancestors passed as a argument,
fn this_cert_parents(
    ancestors: &BTreeSet<CertificateDigest>,
    failure_prob: f64,
) -> BTreeSet<CertificateDigest> {
    std::iter::from_fn(|| {
        let f: f64 = rand::thread_rng().gen();
        Some(f > failure_prob)
    })
    .take(ancestors.len())
    .zip(ancestors)
    .flat_map(|(parenthood, parent)| parenthood.then_some(*parent))
    .collect::<BTreeSet<_>>()
}

// Utility for making several rounds worth of certificates through iterated parenthood sampling.
// The making of individual certificates once parents are figured out is delegated to the `make_one_certificate` argument
fn rounds_of_certificates(
    range: RangeInclusive<Round>,
    initial_parents: &BTreeSet<CertificateDigest>,
    keys: &[PublicKey],
    failure_probability: f64,
    make_one_certificate: impl Fn(
        PublicKey,
        Round,
        BTreeSet<CertificateDigest>,
    ) -> (CertificateDigest, Certificate),
) -> (VecDeque<Certificate>, BTreeSet<CertificateDigest>) {
    let mut certificates = VecDeque::new();
    let mut parents = initial_parents.iter().cloned().collect::<BTreeSet<_>>();
    let mut next_parents = BTreeSet::new();

    for round in range {
        next_parents.clear();
        for name in keys {
            let this_cert_parents = this_cert_parents(&parents, failure_probability);

            let (digest, certificate) =
                make_one_certificate(name.clone(), round, this_cert_parents);
            certificates.push_back(certificate);
            next_parents.insert(digest);
        }
        parents = next_parents.clone();
    }
    (certificates, next_parents)
}

// make rounds worth of unsigned certificates with the sampled number of parents
pub fn make_certificates(
    committee: &Committee,
    range: RangeInclusive<Round>,
    initial_parents: &BTreeSet<CertificateDigest>,
    keys: &[PublicKey],
    failure_probability: f64,
) -> (VecDeque<Certificate>, BTreeSet<CertificateDigest>) {
    let generator = |pk, round, parents| mock_certificate(committee, pk, round, parents);

    rounds_of_certificates(range, initial_parents, keys, failure_probability, generator)
}

// make rounds worth of unsigned certificates with the sampled number of parents
pub fn make_certificates_with_epoch(
    committee: &Committee,
    range: RangeInclusive<Round>,
    epoch: Epoch,
    initial_parents: &BTreeSet<CertificateDigest>,
    keys: &[PublicKey],
) -> (VecDeque<Certificate>, BTreeSet<CertificateDigest>) {
    let mut certificates = VecDeque::new();
    let mut parents = initial_parents.iter().cloned().collect::<BTreeSet<_>>();
    let mut next_parents = BTreeSet::new();

    for round in range {
        next_parents.clear();
        for name in keys {
            let (digest, certificate) =
                mock_certificate_with_epoch(committee, name.clone(), round, epoch, parents.clone());
            certificates.push_back(certificate);
            next_parents.insert(digest);
        }
        parents = next_parents.clone();
    }
    (certificates, next_parents)
}

// make rounds worth of signed certificates with the sampled number of parents
pub fn make_signed_certificates(
    range: RangeInclusive<Round>,
    initial_parents: &BTreeSet<CertificateDigest>,
    committee: &Committee,
    keys: &[KeyPair],
    failure_probability: f64,
) -> (VecDeque<Certificate>, BTreeSet<CertificateDigest>) {
    let public_keys = keys.iter().map(|k| k.public().clone()).collect::<Vec<_>>();
    let generator =
        |pk, round, parents| mock_signed_certificate(keys, pk, round, parents, committee);

    rounds_of_certificates(
        range,
        initial_parents,
        &public_keys[..],
        failure_probability,
        generator,
    )
}

// Creates a badly signed certificate from its given round, origin and parents,
// Note: the certificate is signed by a random key rather than its author
pub fn mock_certificate(
    committee: &Committee,
    origin: PublicKey,
    round: Round,
    parents: BTreeSet<CertificateDigest>,
) -> (CertificateDigest, Certificate) {
    mock_certificate_with_epoch(committee, origin, round, 0, parents)
}

// Creates a badly signed certificate from its given round, epoch, origin, and parents,
// Note: the certificate is signed by a random key rather than its author
pub fn mock_certificate_with_epoch(
    committee: &Committee,
    origin: PublicKey,
    round: Round,
    epoch: Epoch,
    parents: BTreeSet<CertificateDigest>,
) -> (CertificateDigest, Certificate) {
    let header_builder = HeaderBuilder::default();
    let header = header_builder
        .author(origin)
        .round(round)
        .epoch(epoch)
        .parents(parents)
        .payload(fixture_payload(1))
        .build(&KeyPair::generate(&mut rand::thread_rng()))
        .unwrap();
    let certificate = Certificate::new_unsigned(committee, header, Vec::new()).unwrap();
    (certificate.digest(), certificate)
}

// Creates one signed certificate from a set of signers - the signers must include the origin
pub fn mock_signed_certificate(
    signers: &[KeyPair],
    origin: PublicKey,
    round: Round,
    parents: BTreeSet<CertificateDigest>,
    committee: &Committee,
) -> (CertificateDigest, Certificate) {
    let author = signers.iter().find(|kp| *kp.public() == origin).unwrap();
    let header_builder = HeaderBuilder::default()
        .author(origin.clone())
        .payload(fixture_payload(1))
        .round(round)
        .epoch(0)
        .parents(parents);

    let header = header_builder.build(author).unwrap();

    let cert = Certificate::new_unsigned(committee, header.clone(), Vec::new()).unwrap();

    let mut votes = Vec::new();
    for signer in signers {
        let pk = signer.public();
        let sig = signer
            .try_sign(Digest::from(cert.digest()).as_ref())
            .unwrap();
        votes.push((pk.clone(), sig))
    }
    let cert = Certificate::new(committee, header, votes).unwrap();
    (cert.digest(), cert)
}

pub struct Builder<R = OsRng> {
    rng: R,
    committee_size: NonZeroUsize,
    number_of_workers: NonZeroUsize,
    randomize_ports: bool,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Self {
            rng: OsRng,
            committee_size: NonZeroUsize::new(4).unwrap(),
            number_of_workers: NonZeroUsize::new(4).unwrap(),
            randomize_ports: false,
        }
    }
}

impl<R> Builder<R> {
    pub fn committee_size(mut self, committee_size: NonZeroUsize) -> Self {
        self.committee_size = committee_size;
        self
    }

    pub fn number_of_workers(mut self, number_of_workers: NonZeroUsize) -> Self {
        self.number_of_workers = number_of_workers;
        self
    }

    pub fn randomize_ports(mut self, randomize_ports: bool) -> Self {
        self.randomize_ports = randomize_ports;
        self
    }

    pub fn rng<N: rand::RngCore + rand::CryptoRng>(self, rng: N) -> Builder<N> {
        Builder {
            rng,
            committee_size: self.committee_size,
            number_of_workers: self.number_of_workers,
            randomize_ports: self.randomize_ports,
        }
    }
}

impl<R: rand::RngCore + rand::CryptoRng> Builder<R> {
    pub fn build(mut self) -> CommitteeFixture {
        let authorities = (0..self.committee_size.get())
            .map(|_| {
                AuthorityFixture::generate(
                    StdRng::from_rng(&mut self.rng).unwrap(),
                    self.number_of_workers,
                    |host| {
                        if self.randomize_ports {
                            get_available_port(host)
                        } else {
                            0
                        }
                    },
                )
            })
            .collect();

        CommitteeFixture {
            authorities,
            epoch: Epoch::default(),
        }
    }
}

pub struct CommitteeFixture {
    authorities: Vec<AuthorityFixture>,
    epoch: Epoch,
}

impl CommitteeFixture {
    pub fn authorities(&self) -> impl Iterator<Item = &AuthorityFixture> {
        self.authorities.iter()
    }

    pub fn builder() -> Builder {
        Builder::new()
    }

    pub fn committee(&self) -> Committee {
        Committee {
            epoch: self.epoch,
            authorities: self
                .authorities
                .iter()
                .map(|a| {
                    let pubkey = a.public_key();
                    let authority = a.authority();
                    (pubkey, authority)
                })
                .collect(),
        }
    }

    pub fn worker_cache(&self) -> WorkerCache {
        WorkerCache {
            epoch: self.epoch,
            workers: self
                .authorities
                .iter()
                .map(|a| (a.public_key(), a.worker_index()))
                .collect(),
        }
    }

    pub fn shared_worker_cache(&self) -> SharedWorkerCache {
        self.worker_cache().into()
    }

    // pub fn header(&self, author: PublicKey) -> Header {
    // Currently sign with the last authority
    pub fn header(&self) -> Header {
        self.authorities.last().unwrap().header(&self.committee())
    }

    pub fn headers(&self) -> Vec<Header> {
        let committee = self.committee();

        self.authorities
            .iter()
            .map(|a| a.header_with_round(&committee, 1))
            .collect()
    }

    pub fn headers_next_round(&self) -> Vec<Header> {
        let committee = self.committee();
        self.authorities
            .iter()
            .map(|a| a.header_with_round(&committee, 2))
            .collect()
    }

    pub fn headers_round(
        &self,
        prior_round: Round,
        parents: &BTreeSet<CertificateDigest>,
    ) -> (Round, Vec<Header>) {
        let round = prior_round + 1;
        let next_headers = self
            .authorities
            .iter()
            .map(|a| {
                let builder = types::HeaderBuilder::default();
                builder
                    .author(a.public_key())
                    .round(round)
                    .epoch(0)
                    .parents(parents.clone())
                    .with_payload_batch(fixture_batch_with_transactions(10), 0)
                    .build(a.keypair())
                    .unwrap()
            })
            .collect();

        (round, next_headers)
    }

    pub fn votes(&self, header: &Header) -> Vec<Vote> {
        self.authorities()
            .flat_map(|a| {
                // we should not re-sign using the key of the authority
                // that produced the header
                if a.public_key() == header.author {
                    None
                } else {
                    Some(a.vote(header))
                }
            })
            .collect()
    }

    pub fn certificate(&self, header: &Header) -> Certificate {
        let committee = self.committee();
        let votes: Vec<_> = self
            .votes(header)
            .into_iter()
            .map(|x| (x.author, x.signature))
            .collect();
        Certificate::new(&committee, header.clone(), votes).unwrap()
    }

    /// Add a new authority to the commit by randoming generating a key
    pub fn add_authority(&mut self) {
        let authority = AuthorityFixture::generate(
            StdRng::from_rng(OsRng).unwrap(),
            NonZeroUsize::new(4).unwrap(),
            get_available_port,
        );
        self.authorities.push(authority)
    }

    pub fn bump_epoch(&mut self) {
        self.epoch += 1
    }
}

pub struct AuthorityFixture {
    keypair: KeyPair,
    network_keypair: NetworkKeyPair,
    stake: Stake,
    address: Multiaddr,
    workers: BTreeMap<WorkerId, WorkerFixture>,
}

impl AuthorityFixture {
    pub fn keypair(&self) -> &KeyPair {
        &self.keypair
    }

    pub fn network_keypair(&self) -> NetworkKeyPair {
        self.network_keypair.copy()
    }

    pub fn new_network(&self, router: anemo::Router) -> anemo::Network {
        anemo::Network::bind(network::multiaddr_to_address(&self.address).unwrap())
            .server_name("narwhal")
            .private_key(self.network_keypair().private().0.to_bytes())
            .start(router)
            .unwrap()
    }

    pub fn address(&self) -> &Multiaddr {
        &self.address
    }

    pub fn worker(&self, id: WorkerId) -> &WorkerFixture {
        self.workers.get(&id).unwrap()
    }

    pub fn worker_keypairs(&self) -> Vec<NetworkKeyPair> {
        self.workers
            .values()
            .map(|worker| worker.keypair.copy())
            .collect()
    }

    pub fn public_key(&self) -> PublicKey {
        self.keypair.public().clone()
    }

    pub fn network_public_key(&self) -> NetworkPublicKey {
        self.network_keypair.public().clone()
    }

    pub fn authority(&self) -> Authority {
        Authority {
            stake: self.stake,
            primary_address: self.address.clone(),
            network_key: self.network_keypair.public().clone(),
        }
    }

    pub fn worker_index(&self) -> WorkerIndex {
        WorkerIndex(
            self.workers
                .iter()
                .map(|(id, w)| (*id, w.info.clone()))
                .collect(),
        )
    }

    pub fn header(&self, committee: &Committee) -> Header {
        self.header_builder(committee)
            .payload(Default::default())
            .build(&self.keypair)
            .unwrap()
    }

    pub fn header_with_round(&self, committee: &Committee, round: Round) -> Header {
        self.header_builder(committee)
            .payload(Default::default())
            .round(round)
            .build(&self.keypair)
            .unwrap()
    }

    pub fn header_builder(&self, committee: &Committee) -> types::HeaderBuilder {
        types::HeaderBuilder::default()
            .author(self.public_key())
            .round(1)
            .epoch(committee.epoch)
            .parents(
                Certificate::genesis(committee)
                    .iter()
                    .map(|x| x.digest())
                    .collect(),
            )
    }

    pub fn vote(&self, header: &Header) -> Vote {
        Vote::new_with_signer(header, self.keypair.public(), &self.keypair)
    }

    fn generate<R, P>(mut rng: R, number_of_workers: NonZeroUsize, mut get_port: P) -> Self
    where
        R: AllowedRng,
        P: FnMut(&str) -> u16,
    {
        let keypair = KeyPair::generate(&mut rng);
        let network_keypair = NetworkKeyPair::generate(&mut rng);
        let host = "127.0.0.1";
        let address: Multiaddr = format!("/ip4/{}/tcp/{}/http", host, get_port(host))
            .parse()
            .unwrap();

        let workers = (0..number_of_workers.get())
            .map(|idx| {
                let worker = WorkerFixture::generate(&mut rng, idx as u32, &mut get_port);

                (idx as u32, worker)
            })
            .collect();

        Self {
            keypair,
            network_keypair,
            stake: 1,
            address,
            workers,
        }
    }
}

pub struct WorkerFixture {
    keypair: NetworkKeyPair,
    #[allow(dead_code)]
    id: WorkerId,
    info: WorkerInfo,
}

impl WorkerFixture {
    pub fn keypair(&self) -> NetworkKeyPair {
        self.keypair.copy()
    }

    pub fn info(&self) -> &WorkerInfo {
        &self.info
    }

    pub fn new_network(&self, router: anemo::Router) -> anemo::Network {
        anemo::Network::bind(network::multiaddr_to_address(&self.info().worker_address).unwrap())
            .server_name("narwhal")
            .private_key(self.keypair().private().0.to_bytes())
            .start(router)
            .unwrap()
    }

    fn generate<R, P>(rng: R, id: WorkerId, mut get_port: P) -> Self
    where
        R: rand::RngCore + rand::CryptoRng,
        P: FnMut(&str) -> u16,
    {
        let keypair = NetworkKeyPair::generate(&mut StdRng::from_rng(rng).unwrap());
        let worker_name = keypair.public().clone();
        let host = "127.0.0.1";
        let worker_address = format!("/ip4/{}/tcp/{}/http", host, get_port(host))
            .parse()
            .unwrap();
        let transactions = format!("/ip4/{}/tcp/{}/http", host, get_port(host))
            .parse()
            .unwrap();

        Self {
            keypair,
            id,
            info: WorkerInfo {
                name: worker_name,
                worker_address,
                internal_worker_address: None,
                transactions,
            },
        }
    }
}

pub fn test_network(keypair: NetworkKeyPair, address: &Multiaddr) -> anemo::Network {
    let address = network::multiaddr_to_address(address).unwrap();
    let network_key = keypair.private().0.to_bytes();
    anemo::Network::bind(address)
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap()
}

pub fn random_network() -> anemo::Network {
    let network_key = NetworkKeyPair::generate(&mut StdRng::from_rng(OsRng).unwrap());
    let address = "/ip4/127.0.0.1/udp/0".parse().unwrap();
    test_network(network_key, &address)
}
