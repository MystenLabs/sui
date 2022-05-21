// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};
use crypto::{ed25519::Ed25519PublicKey, Hash};
use rand::Rng;
use types::{serialized_batch_digest, Batch, WorkerMessage};

pub fn batch_digest(c: &mut Criterion) {
    let mut digest_group = c.benchmark_group("Batch digests");
    digest_group.sampling_mode(SamplingMode::Flat);

    static BATCH_SIZES: [usize; 4] = [100, 500, 1000, 5000];

    for size in BATCH_SIZES {
        let tx_gen = || {
            (0..512)
                .map(|_| rand::thread_rng().gen())
                .collect::<Vec<u8>>()
        };
        let batch = Batch((0..size).map(|_| tx_gen()).collect::<Vec<_>>());
        let message = WorkerMessage::<Ed25519PublicKey>::Batch(batch.clone());
        let serialized_batch = bincode::serialize(&message).unwrap();

        digest_group.throughput(Throughput::Bytes(512 * size as u64));

        digest_group.bench_with_input(
            BenchmarkId::new("serialized batch digest", size),
            &serialized_batch,
            |b, i| b.iter(|| serialized_batch_digest(i)),
        );
        digest_group.bench_with_input(BenchmarkId::new("batch digest", size), &batch, |b, i| {
            b.iter(|| i.digest())
        });
    }
}

criterion_group! {
    name = consensus_group;
    config = Criterion::default();
    targets = batch_digest
}
criterion_main!(consensus_group);
