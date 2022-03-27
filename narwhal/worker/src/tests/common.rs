// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{processor::SerializedBatchMessage, worker::WorkerMessage};
use bytes::Bytes;
use config::{Authority, Committee, PrimaryAddresses, WorkerAddresses};
use crypto::ed25519::{Ed25519PrivateKey, Ed25519PublicKey};
use ed25519_dalek::{Digest as _, Sha512};
use futures::{sink::SinkExt as _, stream::StreamExt as _};
use primary::{Batch, BatchDigest, Transaction};
use rand::{rngs::StdRng, SeedableRng as _};
use std::{convert::TryInto as _, net::SocketAddr};
use store::{rocks, Store};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

pub fn temp_dir() -> std::path::PathBuf {
    tempfile::tempdir()
        .expect("Failed to open temporary directory")
        .into_path()
}

const BATCHES_CF: &str = "batches";

// Fixture
pub fn keys() -> Vec<(Ed25519PublicKey, Ed25519PrivateKey)> {
    let mut rng = StdRng::from_seed([0; 32]);
    (0..4)
        .map(|_| ed25519_dalek::Keypair::generate(&mut rng))
        .map(|kp| (Ed25519PublicKey(kp.public), Ed25519PrivateKey(kp.secret)))
        .collect()
}

// Fixture
pub fn committee() -> Committee<Ed25519PublicKey> {
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
pub fn transaction() -> Transaction {
    vec![0; 100]
}

// Fixture
pub fn batch() -> Batch {
    Batch(vec![transaction(), transaction()])
}

pub fn batch_with_transactions(num_of_transactions: usize) -> Batch {
    let mut transactions = Vec::new();

    for _ in 0..num_of_transactions {
        transactions.push(transaction());
    }

    Batch(transactions)
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

// Fixture
pub fn serialized_batch() -> Vec<u8> {
    serialise_batch(batch())
}

pub fn serialise_batch(batch: Batch) -> Vec<u8> {
    let message = WorkerMessage::<Ed25519PublicKey>::Batch(batch);
    bincode::serialize(&message).unwrap()
}

// Fixture
pub fn batch_digest() -> BatchDigest {
    resolve_batch_digest(serialized_batch())
}

// Fixture
pub fn resolve_batch_digest(batch_serialised: Vec<u8>) -> BatchDigest {
    BatchDigest::new(
        Sha512::digest(&batch_serialised).as_slice()[..crypto::DIGEST_LEN]
            .try_into()
            .unwrap(),
    )
}

// Fixture
pub fn listener(address: SocketAddr, expected: Option<Bytes>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let listener = TcpListener::bind(&address).await.unwrap();
        let (socket, _) = listener.accept().await.unwrap();
        let transport = Framed::new(socket, LengthDelimitedCodec::new());
        let (mut writer, mut reader) = transport.split();
        match reader.next().await {
            Some(Ok(received)) => {
                writer.send(Bytes::from("Ack")).await.unwrap();
                if let Some(expected) = expected {
                    assert_eq!(received.freeze(), expected);
                }
            }
            _ => panic!("Failed to receive network message"),
        }
    })
}

pub fn open_batch_store() -> Store<BatchDigest, SerializedBatchMessage> {
    let db = rocks::DBMap::<BatchDigest, SerializedBatchMessage>::open(
        temp_dir(),
        None,
        Some(BATCHES_CF),
    )
    .unwrap();
    Store::new(db)
}
