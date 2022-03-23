// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    messages::{BatchDigest, Certificate, CertificateDigest, Header, HeaderDigest, Vote},
    primary::PayloadToken,
};
use bytes::Bytes;
use config::{Authority, Committee, PrimaryAddresses, WorkerAddresses, WorkerId};
use crypto::{
    ed25519::{Ed25519KeyPair, Ed25519PublicKey, Ed25519Signature},
    traits::{KeyPair, Signer, VerifyingKey},
    Digest, Hash as _,
};
use ed25519_dalek::{Digest as _, Sha512};
use futures::{sink::SinkExt as _, stream::StreamExt as _};
use rand::{rngs::StdRng, SeedableRng as _};
use std::{collections::BTreeMap, net::SocketAddr};
use store::{reopen, rocks, rocks::DBMap, Store};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

pub const HEADERS_CF: &str = "headers";
pub const CERTIFICATES_CF: &str = "certificates";
pub const PAYLOAD_CF: &str = "payload";

impl<PublicKey: VerifyingKey> PartialEq for Header<PublicKey> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<PublicKey: VerifyingKey> PartialEq for Vote<PublicKey> {
    fn eq(&self, other: &Self) -> bool {
        self.digest() == other.digest()
    }
}

pub fn temp_dir() -> std::path::PathBuf {
    tempfile::tempdir()
        .expect("Failed to open temporary directory")
        .into_path()
}

// Fixture
pub fn keys() -> Vec<Ed25519KeyPair> {
    let mut rng = StdRng::from_seed([0; 32]);
    (0..4).map(|_| Ed25519KeyPair::generate(&mut rng)).collect()
}

// Fixture
pub fn committee() -> Committee<Ed25519PublicKey> {
    Committee {
        authorities: keys()
            .iter()
            .enumerate()
            .map(|(i, kp)| {
                let id = kp.public();
                let primary = PrimaryAddresses {
                    primary_to_primary: format!("127.0.0.1:{}", 100 + i).parse().unwrap(),
                    worker_to_primary: format!("127.0.0.1:{}", 200 + i).parse().unwrap(),
                };
                let workers = vec![(
                    0,
                    WorkerAddresses {
                        primary_to_worker: format!("127.0.0.1:{}", 300 + i).parse().unwrap(),
                        transactions: format!("127.0.0.1:{}", 400 + i).parse().unwrap(),
                        worker_to_worker: format!("127.0.0.1:{}", 500 + i).parse().unwrap(),
                    },
                )]
                .iter()
                .cloned()
                .collect();
                (
                    id.clone(),
                    Authority {
                        stake: 1,
                        primary,
                        workers,
                    },
                )
            })
            .collect(),
    }
}

// Fixture.
pub fn committee_with_base_port(base_port: u16) -> Committee<Ed25519PublicKey> {
    let mut committee = committee();
    for authority in committee.authorities.values_mut() {
        let primary = &mut authority.primary;

        let port = primary.primary_to_primary.port();
        primary.primary_to_primary.set_port(base_port + port);

        let port = primary.worker_to_primary.port();
        primary.worker_to_primary.set_port(base_port + port);

        for worker in authority.workers.values_mut() {
            let port = worker.primary_to_worker.port();
            worker.primary_to_worker.set_port(base_port + port);

            let port = worker.transactions.port();
            worker.transactions.set_port(base_port + port);

            let port = worker.worker_to_worker.port();
            worker.worker_to_worker.set_port(base_port + port);
        }
    }
    committee
}

// Fixture
pub fn header() -> Header<Ed25519PublicKey> {
    let kp = keys().pop().unwrap();
    let header = Header {
        author: kp.public().clone(),
        round: 1,
        parents: Certificate::genesis(&committee())
            .iter()
            .map(|x| x.digest())
            .collect(),
        ..Header::default()
    };

    let header_digest = header.digest();
    Header {
        id: header_digest,
        signature: kp.sign(Digest::from(header_digest).as_ref()),
        ..header
    }
}

// Fixture
pub fn headers() -> Vec<Header<Ed25519PublicKey>> {
    keys()
        .into_iter()
        .map(|kp| {
            let header = Header {
                author: kp.public().clone(),
                round: 1,
                parents: Certificate::genesis(&committee())
                    .iter()
                    .map(|x| x.digest())
                    .collect(),
                ..Header::default()
            };
            let header_digest = header.digest();
            Header {
                id: header_digest,
                signature: kp.sign(Digest::from(header_digest).as_ref()),
                ..header
            }
        })
        .collect()
}

pub fn fixture_header_with_payload(number_of_batches: u8) -> Header<Ed25519PublicKey> {
    let kp = keys().pop().unwrap();
    let mut payload: BTreeMap<BatchDigest, WorkerId> = BTreeMap::new();

    for i in 0..number_of_batches {
        let batch_digest = BatchDigest(
            Sha512::digest(vec![10u8, 5u8, 8u8, 20u8, i].as_slice()).as_slice()[..32]
                .try_into()
                .unwrap(),
        );
        payload.insert(batch_digest, 0);
    }

    let header = Header {
        author: kp.public().clone(),
        round: 1,
        parents: Certificate::genesis(&committee())
            .iter()
            .map(|x| x.digest())
            .collect(),
        payload,
        ..Header::default()
    };
    let header_digest = header.digest();
    Header {
        id: header_digest,
        signature: kp.sign(Digest::from(header_digest).as_ref()),
        ..header
    }
}

// Fixture
pub fn votes(header: &Header<Ed25519PublicKey>) -> Vec<Vote<Ed25519PublicKey>> {
    keys()
        .into_iter()
        .map(|kp| {
            let vote = Vote {
                id: header.id,
                round: header.round,
                origin: header.author.clone(),
                author: kp.public().clone(),
                signature: Ed25519Signature::default(),
            };
            Vote {
                signature: kp.sign(Digest::from(vote.digest()).as_ref()),
                ..vote
            }
        })
        .collect()
}

// Fixture
pub fn certificate(header: &Header<Ed25519PublicKey>) -> Certificate<Ed25519PublicKey> {
    Certificate {
        header: header.clone(),
        votes: votes(header)
            .into_iter()
            .map(|x| (x.author, x.signature))
            .collect(),
    }
}

// Fixture
pub fn listener(address: SocketAddr) -> JoinHandle<Bytes> {
    tokio::spawn(async move {
        let listener = TcpListener::bind(&address).await.unwrap();
        let (socket, _) = listener.accept().await.unwrap();
        let transport = Framed::new(socket, LengthDelimitedCodec::new());
        let (mut writer, mut reader) = transport.split();
        match reader.next().await {
            Some(Ok(received)) => {
                writer.send(Bytes::from("Ack")).await.unwrap();
                received.freeze()
            }
            _ => panic!("Failed to receive network message"),
        }
    })
}

pub fn create_db_stores() -> (
    Store<HeaderDigest, Header<Ed25519PublicKey>>,
    Store<CertificateDigest, Certificate<Ed25519PublicKey>>,
    Store<(BatchDigest, WorkerId), PayloadToken>,
) {
    // Create a new test store.
    let rocksdb = rocks::open_cf(temp_dir(), None, &[HEADERS_CF, CERTIFICATES_CF, PAYLOAD_CF])
        .expect("Failed creating database");

    let (header_map, certificate_map, payload_map) = reopen!(&rocksdb,
        HEADERS_CF;<HeaderDigest, Header<Ed25519PublicKey>>,
        CERTIFICATES_CF;<CertificateDigest, Certificate<Ed25519PublicKey>>,
        PAYLOAD_CF;<(BatchDigest, WorkerId), PayloadToken>);

    (
        Store::new(header_map),
        Store::new(certificate_map),
        Store::new(payload_map),
    )
}
