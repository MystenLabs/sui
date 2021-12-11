// Copyright(C) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0
use crate::messages::{Certificate, Header, Vote};
use bytes::Bytes;
use config::{Authority, Committee, PrimaryAddresses, WorkerAddresses};
use crypto::{generate_keypair, Hash as _, PublicKey, SecretKey, Signature};
use futures::{sink::SinkExt as _, stream::StreamExt as _};
use rand::{rngs::StdRng, SeedableRng as _};
use std::net::SocketAddr;
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

impl PartialEq for Header {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl PartialEq for Vote {
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
pub fn keys() -> Vec<(PublicKey, SecretKey)> {
    let mut rng = StdRng::from_seed([0; 32]);
    (0..4).map(|_| generate_keypair(&mut rng)).collect()
}

// Fixture
pub fn committee() -> Committee {
    Committee {
        authorities: keys()
            .iter()
            .enumerate()
            .map(|(i, (id, _))| {
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
                    *id,
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
pub fn committee_with_base_port(base_port: u16) -> Committee {
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
pub fn header() -> Header {
    let (author, secret) = keys().pop().unwrap();
    let header = Header {
        author,
        round: 1,
        parents: Certificate::genesis(&committee())
            .iter()
            .map(|x| x.digest())
            .collect(),
        ..Header::default()
    };
    Header {
        id: header.digest(),
        signature: Signature::new(&header.digest(), &secret),
        ..header
    }
}

// Fixture
pub fn headers() -> Vec<Header> {
    keys()
        .into_iter()
        .map(|(author, secret)| {
            let header = Header {
                author,
                round: 1,
                parents: Certificate::genesis(&committee())
                    .iter()
                    .map(|x| x.digest())
                    .collect(),
                ..Header::default()
            };
            Header {
                id: header.digest(),
                signature: Signature::new(&header.digest(), &secret),
                ..header
            }
        })
        .collect()
}

// Fixture
pub fn votes(header: &Header) -> Vec<Vote> {
    keys()
        .into_iter()
        .map(|(author, secret)| {
            let vote = Vote {
                id: header.id.clone(),
                round: header.round,
                origin: header.author,
                author,
                signature: Signature::default(),
            };
            Vote {
                signature: Signature::new(&vote.digest(), &secret),
                ..vote
            }
        })
        .collect()
}

// Fixture
pub fn certificate(header: &Header) -> Certificate {
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
