// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::WorkerId;
use fastcrypto::Hash;
use indexmap::IndexMap;
use rand::{rngs::StdRng, RngCore, SeedableRng};
use serde::Serialize;
use store::{
    reopen,
    rocks::{open_cf, DBMap},
    Store,
};
use types::{Batch, BatchDigest, Certificate, Header};

/// A test batch containing specific transactions.
pub fn test_batch<T: Serialize>(transactions: Vec<T>) -> (BatchDigest, Batch) {
    let serialised_transactions = transactions
        .iter()
        .map(|x| bincode::serialize(x).unwrap())
        .collect();

    let batch = Batch(serialised_transactions);

    (batch.digest(), batch)
}

/// A test certificate with a specific payload.
pub fn test_certificate(payload: IndexMap<BatchDigest, WorkerId>) -> Certificate {
    Certificate {
        header: Header {
            payload,
            ..Header::default()
        },
        ..Certificate::default()
    }
}

/// Make a test storage to hold transaction data.
pub fn test_store() -> Store<BatchDigest, Batch> {
    let store_path = tempfile::tempdir().unwrap();
    const TEMP_BATCHES_CF: &str = "temp_batches";
    let rocksdb = open_cf(store_path, None, &[TEMP_BATCHES_CF]).unwrap();
    let temp_batch_map = reopen!(&rocksdb, TEMP_BATCHES_CF;<BatchDigest, Batch>);
    Store::new(temp_batch_map)
}

/// Create a number of test certificates containing transactions of type u64.
pub fn test_u64_certificates(
    certificates: usize,
    batches_per_certificate: usize,
    transactions_per_batch: usize,
) -> Vec<(Certificate, Vec<(BatchDigest, Batch)>)> {
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

            let payload: IndexMap<_, _> = batches
                .iter()
                .enumerate()
                .map(|(i, (digest, _))| (*digest, /* worker_id */ i as WorkerId))
                .collect();

            let certificate = test_certificate(payload);

            (certificate, batches)
        })
        .collect()
}
