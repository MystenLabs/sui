// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::WorkerId;
use crypto::ed25519::Ed25519PublicKey;
use rand::{rngs::StdRng, RngCore, SeedableRng};
use serde::Serialize;
use std::collections::BTreeMap;
use store::{
    reopen,
    rocks::{open_cf, DBMap},
    Store,
};
use types::{
    serialized_batch_digest, Batch, BatchDigest, Certificate, Header, SerializedBatchMessage,
};
use worker::WorkerMessage;

/// A test batch containing specific transactions.
pub fn test_batch<T: Serialize>(transactions: Vec<T>) -> (BatchDigest, SerializedBatchMessage) {
    let batch = transactions
        .iter()
        .map(|x| bincode::serialize(x).unwrap())
        .collect();
    let message = WorkerMessage::<Ed25519PublicKey>::Batch(Batch(batch));
    let serialized = bincode::serialize(&message).unwrap();
    let digest = serialized_batch_digest(&serialized).unwrap();
    (digest, serialized)
}

/// A test certificate with a specific payload.
pub fn test_certificate(payload: BTreeMap<BatchDigest, WorkerId>) -> Certificate<Ed25519PublicKey> {
    Certificate {
        header: Header {
            payload,
            ..Header::default()
        },
        ..Certificate::default()
    }
}

/// Make a test storage to hold transaction data.
pub fn test_store() -> Store<BatchDigest, SerializedBatchMessage> {
    let store_path = tempfile::tempdir().unwrap();
    const BATCHES_CF: &str = "batches";
    let rocksdb = open_cf(store_path, None, &[BATCHES_CF]).unwrap();
    let batch_map = reopen!(&rocksdb, BATCHES_CF;<BatchDigest, SerializedBatchMessage>);
    Store::new(batch_map)
}

/// Create a number of test certificates containing transactions of type u64.
pub fn test_u64_certificates(
    certificates: usize,
    batches_per_certificate: usize,
    transactions_per_batch: usize,
) -> Vec<(
    Certificate<Ed25519PublicKey>,
    Vec<(BatchDigest, SerializedBatchMessage)>,
)> {
    let mut rng = StdRng::from_seed([0; 32]);
    (0..certificates)
        .map(|_| {
            let batches: Vec<_> = (0..batches_per_certificate)
                .map(|_| {
                    test_batch(
                        (0..transactions_per_batch)
                            .map(|_| rng.next_u64())
                            .collect(),
                    )
                })
                .collect();

            let payload: BTreeMap<_, _> = batches
                .iter()
                .enumerate()
                .map(|(i, (digest, _))| (*digest, /* worker_id */ i as WorkerId))
                .collect();

            let certificate = test_certificate(payload);

            (certificate, batches)
        })
        .collect()
}
